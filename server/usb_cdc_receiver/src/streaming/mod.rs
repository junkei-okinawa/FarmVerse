/// Streaming Architecture for USB CDC Receiver
/// 
/// このモジュールは、ESP-NOWで受信したデータのバッファリングを提供します。
/// 
/// ## 主要機能
/// 
/// - **BufferedData**: 受信データのバッファリング

pub mod buffer;

// 必要な型のみエクスポート
pub use buffer::BufferedData;

#[cfg(test)]
mod tests {
    #[test]
    fn test_streaming_module() {
        // モジュールの基本的なテスト
        assert!(true);
    }
}
