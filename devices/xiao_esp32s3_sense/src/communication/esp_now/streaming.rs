/// ESP-NOW Streaming Protocol Implementation
/// 
/// Issue #12: 大容量画像のストリーミング送信対応
/// - チャンク分割送信機能
/// - シーケンス管理とリトライ機構
/// - チェックサム検証

#[allow(dead_code)] // Issue #12 実装中のため一時的に警告を抑制

use crate::hardware::camera::StreamingCameraConfig;
use crate::communication::esp_now::sender::{EspNowSender, EspNowError};

/// メッセージタイプ
#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
pub enum MessageType {
    StartFrame = 1,
    DataChunk = 2,
    EndFrame = 3,
    Ack = 4,
    Nack = 5,
}

impl MessageType {
    /// u8値からMessageTypeに変換
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(MessageType::StartFrame),
            2 => Some(MessageType::DataChunk),
            3 => Some(MessageType::EndFrame),
            4 => Some(MessageType::Ack),
            5 => Some(MessageType::Nack),
            _ => None,
        }
    }
}

/// ストリーミングメッセージヘッダー
#[derive(Debug, PartialEq, Clone)]
pub struct StreamingHeader {
    pub message_type: MessageType,
    pub sequence_id: u16,
    pub frame_id: u32,
    pub chunk_index: u16,
    pub total_chunks: u16,
    pub data_length: u16,
    pub checksum: u32,
}

impl StreamingHeader {
    pub fn new(
        message_type: MessageType,
        sequence_id: u16,
        frame_id: u32,
        chunk_index: u16,
        total_chunks: u16,
        data_length: u16,
    ) -> Self {
        Self {
            message_type,
            sequence_id,
            frame_id,
            chunk_index,
            total_chunks,
            data_length,
            checksum: 0, // Will be calculated
        }
    }
    
    pub fn calculate_checksum(&mut self, data: &[u8]) {
        let mut checksum: u32 = 0;
        checksum = checksum.wrapping_add(self.sequence_id as u32);
        checksum = checksum.wrapping_add(self.frame_id);
        checksum = checksum.wrapping_add(self.chunk_index as u32);
        checksum = checksum.wrapping_add(self.total_chunks as u32);
        checksum = checksum.wrapping_add(self.data_length as u32);
        
        for byte in data {
            checksum = checksum.wrapping_add(*byte as u32);
        }
        
        self.checksum = checksum;
    }
    
    pub fn verify_checksum(&self, data: &[u8]) -> bool {
        let mut calculated: u32 = 0;
        calculated = calculated.wrapping_add(self.sequence_id as u32);
        calculated = calculated.wrapping_add(self.frame_id);
        calculated = calculated.wrapping_add(self.chunk_index as u32);
        calculated = calculated.wrapping_add(self.total_chunks as u32);
        calculated = calculated.wrapping_add(self.data_length as u32);
        
        for byte in data {
            calculated = calculated.wrapping_add(*byte as u32);
        }
        
        calculated == self.checksum
    }
}

/// ストリーミングメッセージ
#[derive(Debug, PartialEq, Clone)]
pub struct StreamingMessage {
    pub header: StreamingHeader,
    pub data: Vec<u8>,
}

impl StreamingMessage {
    pub fn new(header: StreamingHeader, data: Vec<u8>) -> Self {
        Self { header, data }
    }
    
    /// メッセージをバイト配列にシリアライズする
    pub fn serialize(&self) -> Vec<u8> {
        let mut serialized = Vec::new();
        
        // ヘッダーをシリアライズ
        serialized.push(self.header.message_type as u8);
        serialized.extend_from_slice(&self.header.sequence_id.to_le_bytes());
        serialized.extend_from_slice(&self.header.frame_id.to_le_bytes());
        serialized.extend_from_slice(&self.header.chunk_index.to_le_bytes());
        serialized.extend_from_slice(&self.header.total_chunks.to_le_bytes());
        serialized.extend_from_slice(&self.header.data_length.to_le_bytes());
        serialized.extend_from_slice(&self.header.checksum.to_le_bytes());
        
        // データを追加
        serialized.extend_from_slice(&self.data);
        
        serialized
    }
    
    /// バイト配列からメッセージをデシリアライズする
    pub fn deserialize(data: &[u8]) -> Result<Self, StreamingError> {
        if data.len() < 15 { // 最小ヘッダーサイズ
            return Err(StreamingError::InvalidFrame("Data too short for header".to_string()));
        }
        
        let mut offset = 0;
        
        // ヘッダーをデシリアライズ
        let message_type = MessageType::from_u8(data[offset])
            .ok_or_else(|| StreamingError::InvalidFrame("Invalid message type".to_string()))?;
        offset += 1;
        
        let sequence_id = u16::from_le_bytes([data[offset], data[offset + 1]]);
        offset += 2;
        
        let frame_id = u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]);
        offset += 4;
        
        let chunk_index = u16::from_le_bytes([data[offset], data[offset + 1]]);
        offset += 2;
        
        let total_chunks = u16::from_le_bytes([data[offset], data[offset + 1]]);
        offset += 2;
        
        let data_length = u16::from_le_bytes([data[offset], data[offset + 1]]);
        offset += 2;
        
        let checksum = u32::from_le_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]);
        offset += 4;
        
        // データ部分を抽出
        let payload = if offset < data.len() {
            data[offset..].to_vec()
        } else {
            Vec::new()
        };
        
        let header = StreamingHeader {
            message_type,
            sequence_id,
            frame_id,
            chunk_index,
            total_chunks,
            data_length,
            checksum,
        };
        
        Ok(StreamingMessage::new(header, payload))
    }
    
    pub fn start_frame(frame_id: u32, sequence_id: u16) -> Self {
        let header = StreamingHeader::new(
            MessageType::StartFrame,
            sequence_id,
            frame_id,
            0,
            0,
            0,
        );
        Self::new(header, Vec::new())
    }
    
    pub fn end_frame(frame_id: u32, sequence_id: u16) -> Self {
        let header = StreamingHeader::new(
            MessageType::EndFrame,
            sequence_id,
            frame_id,
            0,
            0,
            0,
        );
        Self::new(header, Vec::new())
    }
    
    pub fn data_chunk(
        frame_id: u32,
        sequence_id: u16,
        chunk_index: u16,
        total_chunks: u16,
        data: Vec<u8>,
    ) -> Self {
        let mut header = StreamingHeader::new(
            MessageType::DataChunk,
            sequence_id,
            frame_id,
            chunk_index,
            total_chunks,
            data.len() as u16,
        );
        header.calculate_checksum(&data);
        Self::new(header, data)
    }
    
    pub fn ack(sequence_id: u16) -> Self {
        let header = StreamingHeader::new(
            MessageType::Ack,
            sequence_id,
            0,
            0,
            0,
            0,
        );
        Self::new(header, Vec::new())
    }
    
    pub fn nack(sequence_id: u16) -> Self {
        let header = StreamingHeader::new(
            MessageType::Nack,
            sequence_id,
            0,
            0,
            0,
            0,
        );
        Self::new(header, Vec::new())
    }
}

/// ストリーミング送信エラー
#[derive(Debug, PartialEq)]
pub enum StreamingError {
    ChunkSizeInvalid,
    SendTimeout,
    AckTimeout,
    ChecksumMismatch,
    MaxRetriesExceeded,
    CameraError(&'static str),
    InvalidFrame(String),
    EspNowError(EspNowError),
}

impl From<EspNowError> for StreamingError {
    fn from(error: EspNowError) -> Self {
        StreamingError::EspNowError(error)
    }
}

/// ストリーミング送信状態
#[derive(Debug, PartialEq)]
pub enum StreamingState {
    Idle,
    Sending,
    WaitingAck,
    Complete,
    Error(StreamingError),
}

/// ストリーミング送信統計
#[derive(Debug, Default)]
pub struct StreamingStats {
    pub frames_sent: u32,
    pub chunks_sent: u32,
    pub bytes_sent: u64,
    pub retries: u32,
    pub errors: u32,
}

#[cfg(test)]
#[derive(Debug)]
struct MockEspNowSender;

#[cfg(test)]
impl MockEspNowSender {
    fn send(&self, _data: &[u8], _timeout_ms: u32) -> Result<(), EspNowError> {
        Ok(()) // Mock implementation always succeeds
    }
}

/// ストリーミング送信機
#[derive(Debug)]
pub struct StreamingSender {
    config: StreamingCameraConfig,
    #[cfg(not(test))]
    esp_now_sender: EspNowSender,
    #[cfg(test)]
    esp_now_sender: MockEspNowSender,
    frame_id: u32,
    sequence_id: u16,
    state: StreamingState,
    stats: StreamingStats,
}

impl StreamingSender {
    #[cfg(not(test))]
    pub fn new(config: StreamingCameraConfig, esp_now_sender: EspNowSender) -> Result<Self, StreamingError> {
        if config.chunk_size == 0 || config.chunk_size > 4096 {
            return Err(StreamingError::ChunkSizeInvalid);
        }
        
        Ok(Self {
            config,
            esp_now_sender,
            frame_id: 0,
            sequence_id: 0,
            state: StreamingState::Idle,
            stats: StreamingStats::default(),
        })
    }

    #[cfg(test)]
    pub fn new(config: StreamingCameraConfig) -> Result<Self, StreamingError> {        
        if config.chunk_size == 0 || config.chunk_size > 4096 {
            return Err(StreamingError::ChunkSizeInvalid);
        }
        
        // For testing purposes, create a dummy sender that won't actually send
        let mock_sender = MockEspNowSender;
        
        Ok(Self {
            config,
            esp_now_sender: mock_sender,
            frame_id: 0,
            sequence_id: 0,
            state: StreamingState::Idle,
            stats: StreamingStats::default(),
        })
    }
    
    pub fn send_frame(&mut self, image_data: &[u8]) -> Result<(), StreamingError> {
        if image_data.is_empty() {
            return Err(StreamingError::CameraError("Empty image data"));
        }
        
        self.state = StreamingState::Sending;
        self.frame_id = self.frame_id.wrapping_add(1);
        
        // Calculate total chunks needed
        let total_chunks = ((image_data.len() + self.config.chunk_size - 1) / self.config.chunk_size) as u16;
        
        // Send start frame message
        self.sequence_id = self.sequence_id.wrapping_add(1);
        let start_msg = StreamingMessage::start_frame(self.frame_id, self.sequence_id);
        self.send_message(&start_msg)?;
        
        // Send data chunks
        for chunk_index in 0..total_chunks {
            let start_offset = (chunk_index as usize) * self.config.chunk_size;
            let end_offset = std::cmp::min(start_offset + self.config.chunk_size, image_data.len());
            let chunk_data = image_data[start_offset..end_offset].to_vec();
            
            self.sequence_id = self.sequence_id.wrapping_add(1);
            let chunk_msg = StreamingMessage::data_chunk(
                self.frame_id,
                self.sequence_id,
                chunk_index,
                total_chunks,
                chunk_data,
            );
            
            self.send_message_with_retry(&chunk_msg)?;
            self.stats.chunks_sent += 1;
            self.stats.bytes_sent += chunk_msg.data.len() as u64;
        }
        
        // Send end frame message
        self.sequence_id = self.sequence_id.wrapping_add(1);
        let end_msg = StreamingMessage::end_frame(self.frame_id, self.sequence_id);
        self.send_message(&end_msg)?;
        
        self.state = StreamingState::Complete;
        self.stats.frames_sent += 1;
        Ok(())
    }
    
    fn send_message(&self, message: &StreamingMessage) -> Result<(), StreamingError> {
        let serialized = message.serialize();
        self.esp_now_sender.send(&serialized, 1000)?; // 1秒タイムアウト
        Ok(())
    }
    
    fn send_message_with_retry(&mut self, message: &StreamingMessage) -> Result<(), StreamingError> {
        for attempt in 0..self.config.max_retries {
            match self.send_message(message) {
                Ok(_) => {
                    // Wait for ACK (simulated)
                    if self.wait_for_ack(message.header.sequence_id)? {
                        return Ok(());
                    }
                },
                Err(e) => {
                    self.stats.errors += 1;
                    if attempt == self.config.max_retries - 1 {
                        return Err(e);
                    }
                }
            }
            self.stats.retries += 1;
        }
        
        Err(StreamingError::MaxRetriesExceeded)
    }
    
    fn wait_for_ack(&self, sequence_id: u16) -> Result<bool, StreamingError> {
        // TODO: ESP-NOWの双方向通信でACKを受信する実装
        // 現在は常にtrueを返す（実装後に削除予定）
        log::debug!("Waiting for ACK for sequence_id: {}", sequence_id);
        Ok(true)
    }
    
    pub fn get_state(&self) -> &StreamingState {
        &self.state
    }
    
    pub fn get_stats(&self) -> &StreamingStats {
        &self.stats
    }
    
    pub fn reset_stats(&mut self) {
        self.stats = StreamingStats::default();
    }
    
    pub fn is_idle(&self) -> bool {
        matches!(self.state, StreamingState::Idle)
    }
    
    pub fn is_sending(&self) -> bool {
        matches!(self.state, StreamingState::Sending)
    }
    
    pub fn is_complete(&self) -> bool {
        matches!(self.state, StreamingState::Complete)
    }
    
    pub fn has_error(&self) -> bool {
        matches!(self.state, StreamingState::Error(_))
    }
}

/// 大容量画像データをチャンクに分割
pub fn split_image_to_chunks(image_data: &[u8], chunk_size: usize) -> Vec<Vec<u8>> {
    if chunk_size == 0 {
        return vec![image_data.to_vec()];
    }
    
    image_data
        .chunks(chunk_size)
        .map(|chunk| chunk.to_vec())
        .collect()
}

/// チャンクから画像データを再構成
pub fn reconstruct_image_from_chunks(chunks: &[Vec<u8>]) -> Vec<u8> {
    chunks.iter().flatten().copied().collect()
}
