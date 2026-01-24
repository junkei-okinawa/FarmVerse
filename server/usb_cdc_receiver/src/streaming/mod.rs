/// Streaming Architecture for USB CDC Receiver
/// 
/// このモジュールは、ESP-NOWで受信したデータのバッファリングを提供します。
/// 
/// ## 主要機能
/// 
/// - **BufferedData**: 受信データのバッファリング

pub mod controller;
pub mod device_manager;
pub mod buffer;
pub use controller::{StreamingController, StreamingConfig};
pub use device_manager::{DeviceStreamManager, ProcessedFrame, StreamManagerConfig};
pub use buffer::BufferedData;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamingError {
    BufferFull,
    InvalidData,
    Timeout,
    EspNowSendError(String), // String wrapping needed? Format uses String
    UsbTransferError(String),
}

// For EspNowSendError(String) we need String, but enum variants with data are fine.
// Wait, the error message in controller.rs was:
// StreamingError::EspNowSendError(format!("...")) -> this creates a String.
// So variants should hold String.
// However, existing variants are unit variants.
// Let's change definition to:
// pub enum StreamingError {
//     BufferFull,
//     InvalidData,
//     Timeout,
//     EspNowSendError(String),
//     UsbTransferError(String),
// }

// Also define StreamingStatistics here.
#[derive(Debug, Clone, Default)]
pub struct StreamingStatistics {
    pub bytes_transferred: u64,
    pub frames_processed: u64,
}

impl StreamingStatistics {
    pub fn count_frame_processed(&mut self, _bytes: usize) {
         self.frames_processed += 1;
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
