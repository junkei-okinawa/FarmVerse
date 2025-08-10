/// ESP-NOW Streaming Protocol Implementation
/// 
/// Issue #12: 大容量画像のストリーミング送信対応
/// - チャンク分割送信機能
/// - シーケンス管理とリトライ機構
/// - チェックサム検証

#[allow(dead_code)] // Issue #12 実装中のため一時的に警告を抑制

use crate::hardware::camera::StreamingCameraConfig;

/// ストリーミングプロトコルのメッセージタイプ
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MessageType {
    StartFrame,
    DataChunk,
    EndFrame,
    Ack,
    Nack,
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

/// ストリーミング送信機
#[derive(Debug)]
pub struct StreamingSender {
    config: StreamingCameraConfig,
    frame_id: u32,
    sequence_id: u16,
    state: StreamingState,
    stats: StreamingStats,
}

impl StreamingSender {
    pub fn new(config: StreamingCameraConfig) -> Result<Self, StreamingError> {
        if config.chunk_size == 0 || config.chunk_size > 4096 {
            return Err(StreamingError::ChunkSizeInvalid);
        }
        
        Ok(Self {
            config,
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
    
    fn send_message(&self, _message: &StreamingMessage) -> Result<(), StreamingError> {
        // Simulate ESP-NOW message sending
        // In real implementation, this would use ESP-NOW APIs
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
    
    fn wait_for_ack(&self, _sequence_id: u16) -> Result<bool, StreamingError> {
        // Simulate ACK waiting
        // In real implementation, this would wait for ACK from receiver
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
