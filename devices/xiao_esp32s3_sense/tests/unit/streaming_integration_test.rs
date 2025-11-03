//! ESP-NOW Streaming Integration Tests
//! 
//! Phase 4A: エンドツーエンドのストリーミングプロトコルテスト
//! - フレーム分割・再構成
//! - プロトコル互換性検証
//! - エラーハンドリング

#[cfg(test)]
mod tests {
    use crate::utils::streaming_protocol::{MessageType, StreamingHeader, StreamingMessage};
    
    const TEST_CHUNK_SIZE: usize = 200;
    
    /// テストデータ生成ヘルパー
    fn generate_test_image(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i % 256) as u8).collect()
    }
    
    /// チャンク分割ヘルパー
    fn split_into_chunks(data: &[u8], chunk_size: usize) -> Vec<Vec<u8>> {
        data.chunks(chunk_size)
            .map(|chunk| chunk.to_vec())
            .collect()
    }
    
    #[test]
    fn test_end_to_end_small_image() {
        // 500バイトの小さな画像
        let image_data = generate_test_image(500);
        let frame_id = 1;
        let mut sequence_id = 0u16;
        
        // チャンク分割
        let chunks = split_into_chunks(&image_data, TEST_CHUNK_SIZE);
        let total_chunks = chunks.len() as u16;
        
        // 1. Start Frame
        let start_msg = StreamingMessage::start_frame(frame_id, sequence_id);
        let start_bytes = start_msg.serialize();
        let decoded_start = StreamingMessage::deserialize(&start_bytes).unwrap();
        assert_eq!(decoded_start.header().message_type(), MessageType::StartFrame);
        assert_eq!(decoded_start.header().frame_id(), frame_id);
        sequence_id += 1;
        
        // 2. Data Chunks
        let mut received_data = Vec::new();
        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            let data_msg = StreamingMessage::data_chunk(
                frame_id,
                sequence_id,
                chunk_idx as u16,
                total_chunks,
                chunk.clone(),
            );
            
            // シリアライズ・デシリアライズ
            let bytes = data_msg.serialize();
            let decoded = StreamingMessage::deserialize(&bytes).unwrap();
            
            // 検証
            assert_eq!(decoded.header().message_type(), MessageType::DataChunk);
            assert_eq!(decoded.header().frame_id(), frame_id);
            assert_eq!(decoded.header().chunk_index(), chunk_idx as u16);
            assert_eq!(decoded.header().total_chunks(), total_chunks);
            assert!(decoded.header().verify_checksum(decoded.data()));
            
            received_data.extend_from_slice(decoded.data());
            sequence_id += 1;
        }
        
        // 3. End Frame
        let end_msg = StreamingMessage::end_frame(frame_id, sequence_id);
        let end_bytes = end_msg.serialize();
        let decoded_end = StreamingMessage::deserialize(&end_bytes).unwrap();
        assert_eq!(decoded_end.header().message_type(), MessageType::EndFrame);
        assert_eq!(decoded_end.header().frame_id(), frame_id);
        
        // 4. データ整合性確認
        assert_eq!(received_data, image_data);
    }
    
    #[test]
    fn test_end_to_end_large_image() {
        // 10KBの大きな画像
        let image_data = generate_test_image(10_000);
        let frame_id = 42;
        let mut sequence_id = 100u16;
        
        let chunks = split_into_chunks(&image_data, TEST_CHUNK_SIZE);
        let total_chunks = chunks.len() as u16;
        
        // Start Frame
        let start_msg = StreamingMessage::start_frame(frame_id, sequence_id);
        let start_bytes = start_msg.serialize();
        StreamingMessage::deserialize(&start_bytes).unwrap();
        sequence_id += 1;
        
        // すべてのチャンクを送信・受信
        let mut received_data = Vec::new();
        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            let data_msg = StreamingMessage::data_chunk(
                frame_id,
                sequence_id,
                chunk_idx as u16,
                total_chunks,
                chunk.clone(),
            );
            
            let bytes = data_msg.serialize();
            let decoded = StreamingMessage::deserialize(&bytes).unwrap();
            
            assert!(decoded.header().verify_checksum(decoded.data()));
            received_data.extend_from_slice(decoded.data());
            sequence_id += 1;
        }
        
        // End Frame
        let end_msg = StreamingMessage::end_frame(frame_id, sequence_id);
        let end_bytes = end_msg.serialize();
        StreamingMessage::deserialize(&end_bytes).unwrap();
        
        // データ整合性確認
        assert_eq!(received_data.len(), image_data.len());
        assert_eq!(received_data, image_data);
    }
    
    #[test]
    fn test_ack_nack_messages() {
        let sequence_id = 42;
        
        // ACKメッセージ
        let ack_msg = StreamingMessage::ack(sequence_id);
        let ack_bytes = ack_msg.serialize();
        let decoded_ack = StreamingMessage::deserialize(&ack_bytes).unwrap();
        assert_eq!(decoded_ack.header().message_type(), MessageType::Ack);
        assert_eq!(decoded_ack.header().sequence_id(), sequence_id);
        
        // NACKメッセージ
        let nack_msg = StreamingMessage::nack(sequence_id);
        let nack_bytes = nack_msg.serialize();
        let decoded_nack = StreamingMessage::deserialize(&nack_bytes).unwrap();
        assert_eq!(decoded_nack.header().message_type(), MessageType::Nack);
        assert_eq!(decoded_nack.header().sequence_id(), sequence_id);
    }
    
    #[test]
    fn test_checksum_validation() {
        let frame_id = 1;
        let sequence_id = 0;
        let data = vec![0xAA, 0xBB, 0xCC, 0xDD];
        
        let msg = StreamingMessage::data_chunk(frame_id, sequence_id, 0, 1, data.clone());
        let mut bytes = msg.serialize();
        
        // チェックサムを破壊
        let checksum_offset = 13; // ヘッダー内のチェックサム位置
        bytes[checksum_offset] ^= 0xFF;
        
        // デシリアライズ後のチェックサム検証
        let decoded = StreamingMessage::deserialize(&bytes).unwrap();
        assert!(!decoded.header().verify_checksum(decoded.data()));
    }
    
    #[test]
    fn test_sequence_id_overflow() {
        // sequence_idのオーバーフロー処理
        let frame_id = 1;
        let max_sequence = u16::MAX;
        
        let msg = StreamingMessage::start_frame(frame_id, max_sequence);
        let bytes = msg.serialize();
        let decoded = StreamingMessage::deserialize(&bytes).unwrap();
        
        assert_eq!(decoded.header().sequence_id(), max_sequence);
        
        // 次のシーケンスIDは0に戻る想定
        let next_sequence = max_sequence.wrapping_add(1);
        assert_eq!(next_sequence, 0);
    }
    
    #[test]
    fn test_chunk_order_preservation() {
        // チャンク順序の保持テスト
        let image_data = generate_test_image(1000);
        let chunks = split_into_chunks(&image_data, TEST_CHUNK_SIZE);
        let total_chunks = chunks.len() as u16;
        let frame_id = 5;
        
        let mut chunk_messages = Vec::new();
        
        // すべてのチャンクをメッセージ化
        for (idx, chunk) in chunks.iter().enumerate() {
            let msg = StreamingMessage::data_chunk(
                frame_id,
                idx as u16,
                idx as u16,
                total_chunks,
                chunk.clone(),
            );
            chunk_messages.push(msg);
        }
        
        // シリアライズ・デシリアライズして順序確認
        let mut reconstructed = Vec::new();
        for (expected_idx, msg) in chunk_messages.iter().enumerate() {
            let bytes = msg.serialize();
            let decoded = StreamingMessage::deserialize(&bytes).unwrap();
            
            assert_eq!(decoded.header().chunk_index(), expected_idx as u16);
            reconstructed.extend_from_slice(decoded.data());
        }
        
        assert_eq!(reconstructed, image_data);
    }
    
    #[test]
    fn test_empty_data_chunk() {
        // 空データチャンクの処理
        let frame_id = 1;
        let sequence_id = 0;
        
        let msg = StreamingMessage::data_chunk(frame_id, sequence_id, 0, 1, vec![]);
        let bytes = msg.serialize();
        let decoded = StreamingMessage::deserialize(&bytes).unwrap();
        
        assert_eq!(decoded.header().data_length(), 0);
        assert_eq!(decoded.data().len(), 0);
        assert!(decoded.header().verify_checksum(decoded.data()));
    }
    
    #[test]
    fn test_protocol_version_compatibility() {
        // プロトコルバージョン互換性テスト
        // 現在のフォーマット: [Type:1][SeqId:2][FrameId:4][ChunkIdx:2][TotalChunks:2][DataLen:2][Checksum:4][Data:N]
        
        let msg = StreamingMessage::data_chunk(1, 2, 3, 4, vec![0xAA, 0xBB]);
        let bytes = msg.serialize();
        
        // ヘッダーサイズ検証
        assert!(bytes.len() >= 17); // 17バイトヘッダー + データ
        
        // フォーマット検証
        assert_eq!(bytes[0], MessageType::DataChunk as u8);
        assert_eq!(u16::from_le_bytes([bytes[1], bytes[2]]), 2); // sequence_id
        assert_eq!(u32::from_le_bytes([bytes[3], bytes[4], bytes[5], bytes[6]]), 1); // frame_id
        assert_eq!(u16::from_le_bytes([bytes[7], bytes[8]]), 3); // chunk_index
        assert_eq!(u16::from_le_bytes([bytes[9], bytes[10]]), 4); // total_chunks
        assert_eq!(u16::from_le_bytes([bytes[11], bytes[12]]), 2); // data_length
    }
    
    #[test]
    fn test_max_chunk_size() {
        // ESP-NOWの最大ペイロードサイズ(250バイト)を考慮
        // ヘッダー17バイト + データ = 最大233バイト/チャンク
        let max_data_size = 233;
        let data = generate_test_image(max_data_size);
        
        let msg = StreamingMessage::data_chunk(1, 0, 0, 1, data.clone());
        let bytes = msg.serialize();
        
        // ESP-NOW制限以下であることを確認
        assert!(bytes.len() <= 250);
        
        let decoded = StreamingMessage::deserialize(&bytes).unwrap();
        assert_eq!(decoded.data(), &data[..]);
    }
}
