/// ESP-NOW Streaming Protocol Implementation
/// 
/// Issue #12: 大容量画像のストリーミング送信対応
/// - チャンク分割送信機能
/// - シーケンス管理とリトライ機構
/// - チェックサム検証

#[allow(dead_code)] // Issue #12 実装中のため一時的に警告を抑制

use crate::hardware::camera::StreamingCameraConfig;
use crate::communication::esp_now::sender::{EspNowSender, EspNowError};
use crate::utils::streaming_protocol::{
    MessageType, StreamingHeader, StreamingMessage, DeserializeError
};

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

impl From<DeserializeError> for StreamingError {
    fn from(error: DeserializeError) -> Self {
        StreamingError::InvalidFrame(error.to_string())
    }
}

/// StreamingMessage用のヘルパー関数
/// ハードウェア非依存の実装はutils::streaming_protocolで提供
impl StreamingMessage {
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

#[cfg(test)]
mod tests {
    use super::*;

    // 基本的なシリアライゼーション/デシリアライゼーションのテストは
    // src/utils/streaming_protocol.rs で実施済み
    //
    // ここでは streaming.rs 固有の機能（ヘルパー関数、StreamingSender等）をテスト

    #[test]
    fn test_helper_start_frame() {
        let msg = StreamingMessage::start_frame(123, 456);
        assert_eq!(msg.header.message_type, MessageType::StartFrame);
        assert_eq!(msg.header.frame_id, 123);
        assert_eq!(msg.header.sequence_id, 456);
        assert_eq!(msg.data.len(), 0);
    }

    #[test]
    fn test_helper_end_frame() {
        let msg = StreamingMessage::end_frame(789, 101);
        assert_eq!(msg.header.message_type, MessageType::EndFrame);
        assert_eq!(msg.header.frame_id, 789);
        assert_eq!(msg.header.sequence_id, 101);
        assert_eq!(msg.data.len(), 0);
    }

    #[test]
    fn test_helper_data_chunk() {
        let data = vec![1, 2, 3, 4, 5];
        let msg = StreamingMessage::data_chunk(10, 20, 3, 10, data.clone());
        
        assert_eq!(msg.header.message_type, MessageType::DataChunk);
        assert_eq!(msg.header.frame_id, 10);
        assert_eq!(msg.header.sequence_id, 20);
        assert_eq!(msg.header.chunk_index, 3);
        assert_eq!(msg.header.total_chunks, 10);
        assert_eq!(msg.header.data_length, 5);
        assert_eq!(msg.data, data);
        assert!(msg.header.verify_checksum(&msg.data));
    }

    #[test]
    fn test_helper_ack() {
        let msg = StreamingMessage::ack(555);
        assert_eq!(msg.header.message_type, MessageType::Ack);
        assert_eq!(msg.header.sequence_id, 555);
        assert_eq!(msg.data.len(), 0);
    }

    #[test]
    fn test_helper_nack() {
        let msg = StreamingMessage::nack(666);
        assert_eq!(msg.header.message_type, MessageType::Nack);
        assert_eq!(msg.header.sequence_id, 666);
        assert_eq!(msg.data.len(), 0);
    }

    #[test]
    fn test_deserialize_error_conversion() {
        // DeserializeErrorからStreamingErrorへの変換テスト
        let short_data = vec![1, 2, 3]; // Too short
        let result = StreamingMessage::deserialize(&short_data);
        
        assert!(result.is_err());
        match result {
            Err(StreamingError::InvalidFrame(_)) => {}, // Expected
            _ => panic!("Expected StreamingError::InvalidFrame"),
        }
    }

    #[test]
    fn test_split_image_to_chunks() {
        let image_data = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let chunks = split_image_to_chunks(&image_data, 3);
        
        assert_eq!(chunks.len(), 4);
        assert_eq!(chunks[0], vec![1, 2, 3]);
        assert_eq!(chunks[1], vec![4, 5, 6]);
        assert_eq!(chunks[2], vec![7, 8, 9]);
        assert_eq!(chunks[3], vec![10]);
    }

    #[test]
    fn test_reconstruct_image_from_chunks() {
        let chunks = vec![
            vec![1, 2, 3],
            vec![4, 5, 6],
            vec![7, 8, 9],
            vec![10],
        ];
        
        let reconstructed = reconstruct_image_from_chunks(&chunks);
        assert_eq!(reconstructed, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    }

    #[test]
    fn test_round_trip_chunk_operations() {
        let original_data = vec![0xAA; 1000];  // 1000 bytes
        let chunks = split_image_to_chunks(&original_data, 233);
        let reconstructed = reconstruct_image_from_chunks(&chunks);
        
        assert_eq!(reconstructed, original_data);
    }
}
