pub mod data_queue;

/// 受信データを表す構造体
/// 
/// MACアドレスとフレームデータを保持します。
#[derive(Debug, Clone)]
pub struct ReceivedData {
    /// 送信元のMACアドレス
    pub mac: [u8; 6],
    /// 受信したフレームデータ
    pub data: Vec<u8>,
}

/// キューの操作結果を表す型
pub type QueueResult<T> = Result<T, QueueError>;

/// キューのエラーを表す列挙型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueError {
    /// キューがいっぱいの場合のエラー
    Full,
    /// キューが空の場合のエラー
    Empty,
    /// キュー操作時のロックエラー
    LockError,
    /// その他のエラー
    Other(&'static str),
}

impl std::fmt::Display for QueueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueueError::Full => write!(f, "Queue is full"),
            QueueError::Empty => write!(f, "Queue is empty"),
            QueueError::LockError => write!(f, "Failed to lock queue"),
            QueueError::Other(msg) => write!(f, "Queue error: {}", msg),
        }
    }
}

impl std::error::Error for QueueError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_error_display() {
        assert_eq!(format!("{}", QueueError::Full), "Queue is full");
        assert_eq!(format!("{}", QueueError::Empty), "Queue is empty");
        assert_eq!(format!("{}", QueueError::LockError), "Failed to lock queue");
        assert_eq!(format!("{}", QueueError::Other("test")), "Queue error: test");
    }
}
