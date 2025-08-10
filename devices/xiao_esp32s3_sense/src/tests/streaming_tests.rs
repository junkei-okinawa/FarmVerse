/// ESP-NOW ストリーミング送信機能テスト
/// 
/// テスト項目:
/// 1. ストリーミングプロトコル
/// 2. チャンク分割・再構成
/// 3. エラーハンドリング・リトライ機構
#[cfg(test)]
mod streaming_protocol_tests {
    use crate::communication::esp_now::streaming::*;
    use crate::hardware::camera::StreamingCameraConfig;

    #[test]
    fn test_streaming_message_creation() {
        let frame = StreamingMessage::start_frame(1, 100);
        
        assert_eq!(frame.header.message_type, MessageType::StartFrame);
        assert_eq!(frame.header.frame_id, 1);
        assert_eq!(frame.header.sequence_id, 100);
        assert_eq!(frame.header.data_length, 0);
        assert!(frame.data.is_empty());
    }

    #[test]
    fn test_data_chunk_creation() {
        let test_data = vec![1, 2, 3, 4, 5];
        let chunk = StreamingMessage::data_chunk(42, 200, 1, 5, test_data.clone());
        
        assert_eq!(chunk.header.message_type, MessageType::DataChunk);
        assert_eq!(chunk.header.frame_id, 42);
        assert_eq!(chunk.header.sequence_id, 200);
        assert_eq!(chunk.header.chunk_index, 1);
        assert_eq!(chunk.header.total_chunks, 5);
        assert_eq!(chunk.header.data_length, 5);
        assert_eq!(chunk.data, test_data);
        assert!(chunk.header.checksum > 0);
    }

    #[test]
    fn test_checksum_verification() {
        let test_data = vec![10, 20, 30, 40, 50];
        let mut header = StreamingHeader::new(
            MessageType::DataChunk,
            1000,
            500,
            2,
            10,
            test_data.len() as u16,
        );
        
        // チェックサム計算
        header.calculate_checksum(&test_data);
        
        // チェックサム検証
        assert!(header.verify_checksum(&test_data));
        
        // 間違ったデータでの検証失敗確認
        let wrong_data = vec![99, 98, 97];
        assert!(!header.verify_checksum(&wrong_data));
    }

    #[test]
    fn test_streaming_sender_creation() {
        let config = StreamingCameraConfig::default();
        let sender = StreamingSender::new(config);
        
        assert!(sender.is_ok());
        let sender = sender.unwrap();
        assert!(sender.is_idle());
        assert!(!sender.is_sending());
        assert!(!sender.is_complete());
        assert!(!sender.has_error());
    }

    #[test]
    fn test_streaming_sender_invalid_config() {
        let mut config = StreamingCameraConfig::default();
        config.chunk_size = 0; // Invalid
        
        let result = StreamingSender::new(config);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), StreamingError::ChunkSizeInvalid);
    }

    #[test]
    fn test_frame_sending() {
        let config = StreamingCameraConfig::default();
        let mut sender = StreamingSender::new(config).unwrap();
        
        let test_image = vec![0xFF, 0xD8, 1, 2, 3, 4, 5, 0xFF, 0xD9];
        let result = sender.send_frame(&test_image);
        
        assert!(result.is_ok());
        assert!(sender.is_complete());
        
        let stats = sender.get_stats();
        assert_eq!(stats.frames_sent, 1);
        assert!(stats.chunks_sent > 0);
        assert!(stats.bytes_sent > 0);
    }

    #[test]
    fn test_empty_frame_error() {
        let config = StreamingCameraConfig::default();
        let mut sender = StreamingSender::new(config).unwrap();
        
        let empty_data = vec![];
        let result = sender.send_frame(&empty_data);
        
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), StreamingError::CameraError("Empty image data"));
    }
}

/// ESP-NOW ストリーミング性能・信頼性テスト
/// 
/// テスト項目:
/// 1. 複数フレーム連続送信
/// 2. ストリーミング統計情報
/// 3. リトライ機構動作確認
#[cfg(test)]
mod streaming_reliability_tests {
    use crate::communication::esp_now::streaming::*;
    use crate::hardware::camera::StreamingCameraConfig;

    #[test]
    fn test_multiple_frame_sending() {
        let config = StreamingCameraConfig::default();
        let mut sender = StreamingSender::new(config).unwrap();
        
        // 複数フレーム送信
        for i in 1..=3 {
            let test_data = vec![i; 100]; // 100バイトのテストデータ
            let result = sender.send_frame(&test_data);
            assert!(result.is_ok());
        }
        
        let stats = sender.get_stats();
        assert_eq!(stats.frames_sent, 3);
        assert!(stats.chunks_sent >= 3); // 最低3チャンク以上
        assert_eq!(stats.bytes_sent, 300); // 100 * 3フレーム
    }

    #[test]
    fn test_streaming_statistics() {
        let config = StreamingCameraConfig::default();
        let mut sender = StreamingSender::new(config).unwrap();
        
        // 統計リセット
        sender.reset_stats();
        let stats = sender.get_stats();
        assert_eq!(stats.frames_sent, 0);
        assert_eq!(stats.chunks_sent, 0);
        assert_eq!(stats.bytes_sent, 0);
        assert_eq!(stats.retries, 0);
        assert_eq!(stats.errors, 0);
        
        // フレーム送信後の統計確認
        let test_data = vec![1; 50];
        sender.send_frame(&test_data).unwrap();
        
        let stats = sender.get_stats();
        assert!(stats.frames_sent > 0);
        assert!(stats.bytes_sent > 0);
    }

    #[test]
    fn test_chunk_splitting() {
        let image_data = vec![42; 2000]; // 2KB test data
        let chunks = split_image_to_chunks(&image_data, 512); // 512-byte chunks
        
        assert_eq!(chunks.len(), 4); // 2000/512 = 4 chunks
        assert_eq!(chunks[0].len(), 512);
        assert_eq!(chunks[1].len(), 512);
        assert_eq!(chunks[2].len(), 512);
        assert_eq!(chunks[3].len(), 2000 - 512 * 3); // Remainder
        
        // データ再構成テスト
        let reconstructed = reconstruct_image_from_chunks(&chunks);
        assert_eq!(reconstructed, image_data);
    }

    #[test]
    fn test_edge_case_chunk_splitting() {
        // 空データ
        let empty_chunks = split_image_to_chunks(&[], 1024);
        assert_eq!(empty_chunks.len(), 1);
        assert!(empty_chunks[0].is_empty());
        
        // チャンクサイズ0
        let data = vec![1, 2, 3, 4, 5];
        let zero_chunks = split_image_to_chunks(&data, 0);
        assert_eq!(zero_chunks.len(), 1);
        assert_eq!(zero_chunks[0], data);
        
        // データサイズ = チャンクサイズ
        let exact_chunks = split_image_to_chunks(&data, 5);
        assert_eq!(exact_chunks.len(), 1);
        assert_eq!(exact_chunks[0], data);
    }
}
