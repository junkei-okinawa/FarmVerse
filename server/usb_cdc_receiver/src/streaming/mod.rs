/// Streaming Architecture for USB CDC Receiver
/// 
/// このモジュールは、ESP-NOWで受信したデータのバッファリングを提供します。
/// 
/// ## 主要機能
/// 
/// - **BufferedData**: 受信データのバッファリング

#[cfg(feature = "esp")]
pub mod controller;
pub mod device_manager;
#[cfg(feature = "esp")]
pub mod buffer;

#[cfg(feature = "esp")]
pub use controller::{StreamingController, StreamingConfig};
pub use device_manager::{DeviceStreamManager, ProcessedFrame, StreamManagerConfig};
#[cfg(feature = "esp")]
pub use buffer::BufferedData;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamingError {
    BufferFull,
    InvalidData,
    Timeout,
    EspNowSendError(String), // carries the underlying ESP-NOW error message
    UsbTransferError(String), // carries the underlying USB transfer error message
}

// Also define StreamingStatistics here.
#[derive(Debug, Clone, Default)]
pub struct StreamingStatistics {
    pub bytes_transferred: u64,
    pub frames_received: u64,
    pub frames_processed: u64,
}

impl StreamingStatistics {
    pub fn count_frame_received(&mut self) {
        self.frames_received += 1;
    }

    pub fn count_frame_processed(&mut self, bytes: usize) {
         self.frames_processed += 1;
         self.bytes_transferred += bytes as u64;
    }

    pub fn add_frames_processed(&mut self, count: u64) {
        self.frames_processed += count;
    }
}

pub type StreamingResult<T> = Result<T, StreamingError>;

impl std::fmt::Display for StreamingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamingError::BufferFull => write!(f, "Buffer is full"),
            StreamingError::InvalidData => write!(f, "Invalid data received"),
            StreamingError::Timeout => write!(f, "Operation timed out"),
            StreamingError::EspNowSendError(msg) => write!(f, "ESP-NOW send error: {}", msg),
            StreamingError::UsbTransferError(msg) => write!(f, "USB transfer error: {}", msg),
        }
    }
}

impl std::error::Error for StreamingError {}

// 必要な型のみエクスポート
// pub use buffer::BufferedData; // Removed duplicate


#[cfg(test)]
mod tests {
    #[test]
    fn test_streaming_module() {
        // モジュールの基本的なテスト
        assert!(true);
    }
}
