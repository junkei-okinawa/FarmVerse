/// Streaming Architecture for USB CDC Receiver
/// 
/// このモジュールは、ESP-NOWで受信したデータを即座にUSB CDCに転送する
/// ストリーミングアーキテクチャを実装します。
/// 
/// ## 主要機能
/// 
/// - **DeviceStreamManager**: 複数デバイスのストリーム管理
/// - **StreamingBuffer**: 循環バッファでの効率的メモリ利用
/// - **FrameProcessor**: フレーム解析とチェックサム検証
/// - **StreamingController**: 全体制御とエラーハンドリング

pub mod buffer;
pub mod device_manager;
pub mod frame_processor;
pub mod controller;

// 必要な型のみエクスポート
pub use controller::{StreamingController, StreamingConfig};

/// ストリーミング処理で使用する共通エラー型
#[derive(Debug, Clone)]
pub enum StreamingError {
    /// バッファが満杯
    BufferFull,
    /// 無効なフレーム
    InvalidFrame(String),
    /// チェックサムエラー
    ChecksumMismatch { expected: u32, actual: u32 },
    /// シーケンス番号エラー
    SequenceError { expected: u16, actual: u16 },
    /// タイムアウト
    Timeout,
    /// デバイスが見つからない
    DeviceNotFound([u8; 6]),
    /// USB転送エラー
    UsbTransferError(String),
    /// ESP-NOW送信エラー
    EspNowSendError(String),
    /// その他のエラー
    Other(String),
}

impl core::fmt::Display for StreamingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StreamingError::BufferFull => write!(f, "Buffer is full"),
            StreamingError::InvalidFrame(msg) => write!(f, "Invalid frame: {}", msg),
            StreamingError::ChecksumMismatch { expected, actual } => {
                write!(f, "Checksum mismatch: expected {}, got {}", expected, actual)
            }
            StreamingError::SequenceError { expected, actual } => {
                write!(f, "Sequence error: expected {}, got {}", expected, actual)
            }
            StreamingError::Timeout => write!(f, "Operation timed out"),
            StreamingError::DeviceNotFound(mac) => {
                write!(f, "Device not found: {:02X?}", mac)
            }
            StreamingError::UsbTransferError(msg) => write!(f, "USB transfer error: {}", msg),
            StreamingError::EspNowSendError(msg) => write!(f, "ESP-NOW send error: {}", msg),
            StreamingError::Other(msg) => write!(f, "Other error: {}", msg),
        }
    }
}

/// ストリーミング処理の結果型
pub type StreamingResult<T> = Result<T, StreamingError>;

/// ストリーミング処理の統計情報
#[derive(Debug, Clone, Default)]
pub struct StreamingStatistics {
    /// 受信したフレーム数
    pub frames_received: u64,
    /// 処理に成功したフレーム数
    pub frames_processed: u64,
    /// エラーが発生したフレーム数
    pub frames_error: u64,
    /// 転送されたバイト数
    pub bytes_transferred: u64,
    /// バッファフル発生回数
    pub buffer_full_count: u32,
    /// チェックサムエラー回数
    pub checksum_error_count: u32,
    /// シーケンスエラー回数
    pub sequence_error_count: u32,
}

impl StreamingStatistics {
    /// 新しい統計インスタンスを作成
    pub fn new() -> Self {
        Self::default()
    }

    /// フレーム受信をカウント
    pub fn count_frame_received(&mut self) {
        self.frames_received += 1;
    }

    /// フレーム処理成功をカウント
    pub fn count_frame_processed(&mut self, bytes: usize) {
        self.frames_processed += 1;
        self.bytes_transferred += bytes as u64;
    }

    /// エラーをカウント
    pub fn count_error(&mut self, error: &StreamingError) {
        self.frames_error += 1;
        match error {
            StreamingError::BufferFull => self.buffer_full_count += 1,
            StreamingError::ChecksumMismatch { .. } => self.checksum_error_count += 1,
            StreamingError::SequenceError { .. } => self.sequence_error_count += 1,
            _ => {}
        }
    }

    /// 成功率を計算（パーセンテージ）
    pub fn success_rate(&self) -> f32 {
        if self.frames_received == 0 {
            0.0
        } else {
            (self.frames_processed as f32 / self.frames_received as f32) * 100.0
        }
    }

    /// 統計をリセット
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_statistics() {
        let mut stats = StreamingStatistics::new();
        
        // 初期状態
        assert_eq!(stats.frames_received, 0);
        assert_eq!(stats.frames_processed, 0);
        assert_eq!(stats.success_rate(), 0.0);
        
        // フレーム受信
        stats.count_frame_received();
        stats.count_frame_processed(100);
        assert_eq!(stats.frames_received, 1);
        assert_eq!(stats.frames_processed, 1);
        assert_eq!(stats.bytes_transferred, 100);
        assert_eq!(stats.success_rate(), 100.0);
        
        // エラー発生
        stats.count_frame_received();
        stats.count_error(&StreamingError::BufferFull);
        assert_eq!(stats.frames_received, 2);
        assert_eq!(stats.frames_error, 1);
        assert_eq!(stats.buffer_full_count, 1);
        assert_eq!(stats.success_rate(), 50.0);
        
        // リセット
        stats.reset();
        assert_eq!(stats.frames_received, 0);
        assert_eq!(stats.success_rate(), 0.0);
    }

    #[test]
    fn test_streaming_error_display() {
        let error = StreamingError::ChecksumMismatch { expected: 123, actual: 456 };
        assert_eq!(error.to_string(), "Checksum mismatch: expected 123, got 456");
        
        let error = StreamingError::DeviceNotFound([0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]);
        assert!(error.to_string().contains("Device not found"));
    }
}
