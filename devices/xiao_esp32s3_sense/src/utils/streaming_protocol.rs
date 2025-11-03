/// ESP-NOW ストリーミングプロトコル（ハードウェア非依存部分）
/// テスト可能な純粋関数を提供

/// デシリアライゼーションエラー型(ハードウェア非依存)
/// 
/// ストリーミングメッセージのデシリアライズ時に発生するエラー。
/// ハードウェア非依存のため、`no_std`環境でも使用可能。
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum DeserializeError {
    /// データ長がヘッダーサイズ(17バイト)未満
    DataTooShort,
    /// 無効なメッセージタイプ値を検出(値を含む)
    InvalidMessageType(u8),
}

impl core::fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DeserializeError::DataTooShort => write!(f, "Data too short for header"),
            DeserializeError::InvalidMessageType(value) => write!(f, "Invalid message type: {}", value),
        }
    }
}

/// メッセージタイプ
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
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
#[derive(Debug, PartialEq, Eq, Clone)]
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
            checksum: 0,
        }
    }
    
    /// チェックサムを計算して設定
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
    
    /// チェックサムを検証
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

    /// フィールドアクセサ
    pub fn message_type(&self) -> MessageType {
        self.message_type
    }

    pub fn sequence_id(&self) -> u16 {
        self.sequence_id
    }

    pub fn frame_id(&self) -> u32 {
        self.frame_id
    }

    pub fn chunk_index(&self) -> u16 {
        self.chunk_index
    }

    pub fn total_chunks(&self) -> u16 {
        self.total_chunks
    }

    pub fn data_length(&self) -> u16 {
        self.data_length
    }
}

/// ストリーミングメッセージ
#[derive(Debug, PartialEq, Eq, Clone)]
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
        
        // ヘッダーをシリアライズ (15 bytes)
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
    pub fn deserialize(data: &[u8]) -> Result<Self, DeserializeError> {
        if data.len() < 17 {
            return Err(DeserializeError::DataTooShort);
        }
        
        let mut offset = 0;
        
        // ヘッダーをデシリアライズ
        let message_type = MessageType::from_u8(data[offset])
            .ok_or(DeserializeError::InvalidMessageType(data[offset]))?;
        offset += 1;
        
        let sequence_id = u16::from_le_bytes([data[offset], data[offset + 1]]);
        offset += 2;
        
        let frame_id = u32::from_le_bytes([
            data[offset], data[offset + 1], data[offset + 2], data[offset + 3]
        ]);
        offset += 4;
        
        let chunk_index = u16::from_le_bytes([data[offset], data[offset + 1]]);
        offset += 2;
        
        let total_chunks = u16::from_le_bytes([data[offset], data[offset + 1]]);
        offset += 2;
        
        let data_length = u16::from_le_bytes([data[offset], data[offset + 1]]);
        offset += 2;
        
        let checksum = u32::from_le_bytes([
            data[offset], data[offset + 1], data[offset + 2], data[offset + 3]
        ]);
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

    /// ヘッダーへのアクセス
    pub fn header(&self) -> &StreamingHeader {
        &self.header
    }

    /// データへのアクセス
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Start Frameメッセージを作成
    pub fn start_frame(frame_id: u32, sequence_id: u16) -> Self {
        let mut header = StreamingHeader::new(
            MessageType::StartFrame,
            sequence_id,
            frame_id,
            0,
            0,
            0,
        );
        header.calculate_checksum(&[]);
        StreamingMessage::new(header, vec![])
    }

    /// Data Chunkメッセージを作成
    pub fn data_chunk(
        frame_id: u32,
        sequence_id: u16,
        chunk_index: u16,
        total_chunks: u16,
        data: Vec<u8>,
    ) -> Self {
        let data_length = data.len() as u16;
        let mut header = StreamingHeader::new(
            MessageType::DataChunk,
            sequence_id,
            frame_id,
            chunk_index,
            total_chunks,
            data_length,
        );
        header.calculate_checksum(&data);
        StreamingMessage::new(header, data)
    }

    /// End Frameメッセージを作成
    pub fn end_frame(frame_id: u32, sequence_id: u16) -> Self {
        let mut header = StreamingHeader::new(
            MessageType::EndFrame,
            sequence_id,
            frame_id,
            0,
            0,
            0,
        );
        header.calculate_checksum(&[]);
        StreamingMessage::new(header, vec![])
    }

    /// ACKメッセージを作成
    pub fn ack(sequence_id: u16) -> Self {
        let mut header = StreamingHeader::new(
            MessageType::Ack,
            sequence_id,
            0,
            0,
            0,
            0,
        );
        header.calculate_checksum(&[]);
        StreamingMessage::new(header, vec![])
    }

    /// NACKメッセージを作成
    pub fn nack(sequence_id: u16) -> Self {
        let mut header = StreamingHeader::new(
            MessageType::Nack,
            sequence_id,
            0,
            0,
            0,
            0,
        );
        header.calculate_checksum(&[]);
        StreamingMessage::new(header, vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // MessageType テスト

    #[test]
    fn test_message_type_from_u8_valid() {
        assert_eq!(MessageType::from_u8(1), Some(MessageType::StartFrame));
        assert_eq!(MessageType::from_u8(2), Some(MessageType::DataChunk));
        assert_eq!(MessageType::from_u8(3), Some(MessageType::EndFrame));
        assert_eq!(MessageType::from_u8(4), Some(MessageType::Ack));
        assert_eq!(MessageType::from_u8(5), Some(MessageType::Nack));
    }

    #[test]
    fn test_message_type_from_u8_invalid() {
        assert_eq!(MessageType::from_u8(0), None);
        assert_eq!(MessageType::from_u8(6), None);
        assert_eq!(MessageType::from_u8(255), None);
    }

    #[test]
    fn test_message_type_to_u8() {
        assert_eq!(MessageType::StartFrame as u8, 1);
        assert_eq!(MessageType::DataChunk as u8, 2);
        assert_eq!(MessageType::EndFrame as u8, 3);
        assert_eq!(MessageType::Ack as u8, 4);
        assert_eq!(MessageType::Nack as u8, 5);
    }

    // StreamingHeader テスト

    #[test]
    fn test_header_new() {
        let header = StreamingHeader::new(
            MessageType::DataChunk,
            100,
            1,
            5,
            10,
            128,
        );
        
        assert_eq!(header.message_type, MessageType::DataChunk);
        assert_eq!(header.sequence_id, 100);
        assert_eq!(header.frame_id, 1);
        assert_eq!(header.chunk_index, 5);
        assert_eq!(header.total_chunks, 10);
        assert_eq!(header.data_length, 128);
        assert_eq!(header.checksum, 0);
    }

    #[test]
    fn test_checksum_calculation() {
        let mut header = StreamingHeader::new(
            MessageType::DataChunk,
            1,
            1,
            0,
            1,
            5,
        );
        let data = vec![1, 2, 3, 4, 5];
        
        header.calculate_checksum(&data);
        
        // チェックサムが計算されていることを確認
        assert_ne!(header.checksum, 0);
        
        // 同じデータで検証が成功することを確認
        assert!(header.verify_checksum(&data));
    }

    #[test]
    fn test_checksum_verification_success() {
        let mut header = StreamingHeader::new(
            MessageType::DataChunk,
            100,
            5,
            2,
            10,
            4,
        );
        let data = vec![0xAA, 0xBB, 0xCC, 0xDD];
        
        header.calculate_checksum(&data);
        assert!(header.verify_checksum(&data));
    }

    #[test]
    fn test_checksum_verification_failure() {
        let mut header = StreamingHeader::new(
            MessageType::DataChunk,
            100,
            5,
            2,
            10,
            4,
        );
        let data = vec![0xAA, 0xBB, 0xCC, 0xDD];
        header.calculate_checksum(&data);
        
        // 異なるデータで検証失敗
        let wrong_data = vec![0xAA, 0xBB, 0xCC, 0xDE];
        assert!(!header.verify_checksum(&wrong_data));
    }

    #[test]
    fn test_checksum_empty_data() {
        let mut header = StreamingHeader::new(
            MessageType::StartFrame,
            1,
            1,
            0,
            0,
            0,
        );
        let data: Vec<u8> = vec![];
        
        header.calculate_checksum(&data);
        assert!(header.verify_checksum(&data));
    }

    #[test]
    fn test_checksum_overflow() {
        let mut header = StreamingHeader::new(
            MessageType::DataChunk,
            65535,
            4294967295,
            65535,
            65535,
            1000,
        );
        let data = vec![0xFF; 1000];
        
        header.calculate_checksum(&data);
        assert!(header.verify_checksum(&data));
    }

    // StreamingMessage シリアライゼーションテスト

    #[test]
    fn test_message_serialize_deserialize_roundtrip() {
        let mut header = StreamingHeader::new(
            MessageType::DataChunk,
            42,
            7,
            3,
            8,
            4,
        );
        let data = vec![0x01, 0x02, 0x03, 0x04];
        header.calculate_checksum(&data);
        
        let message = StreamingMessage::new(header.clone(), data.clone());
        let serialized = message.serialize();
        let deserialized = StreamingMessage::deserialize(&serialized).unwrap();
        
        assert_eq!(deserialized.header, header);
        assert_eq!(deserialized.data, data);
    }

    #[test]
    fn test_message_serialize_format() {
        let header = StreamingHeader::new(
            MessageType::StartFrame,
            1,
            1,
            0,
            1,
            0,
        );
        let data: Vec<u8> = vec![];
        
        let message = StreamingMessage::new(header, data);
        let serialized = message.serialize();
        
        // ヘッダーサイズは17バイト (1+2+4+2+2+2+4)
        assert_eq!(serialized.len(), 17);
        
        // メッセージタイプ確認
        assert_eq!(serialized[0], MessageType::StartFrame as u8);
    }

    #[test]
    fn test_message_deserialize_too_short() {
        let short_data = vec![1, 2, 3];
        let result = StreamingMessage::deserialize(&short_data);
        
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), DeserializeError::DataTooShort);
    }

    #[test]
    fn test_message_deserialize_invalid_message_type() {
        let mut invalid_data = vec![0; 17]; // 17 bytes header
        invalid_data[0] = 99; // Invalid message type
        
        let result = StreamingMessage::deserialize(&invalid_data);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), DeserializeError::InvalidMessageType(99));
    }

    #[test]
    fn test_message_with_payload() {
        let mut header = StreamingHeader::new(
            MessageType::DataChunk,
            10,
            2,
            1,
            5,
            256,
        );
        let data = vec![0xAA; 256];
        header.calculate_checksum(&data);
        
        let message = StreamingMessage::new(header, data.clone());
        let serialized = message.serialize();
        
        // ヘッダー(17) + データ(256)
        assert_eq!(serialized.len(), 17 + 256);
        
        let deserialized = StreamingMessage::deserialize(&serialized).unwrap();
        assert_eq!(deserialized.data, data);
    }

    #[test]
    fn test_multiple_message_types() {
        let types = vec![
            MessageType::StartFrame,
            MessageType::DataChunk,
            MessageType::EndFrame,
            MessageType::Ack,
            MessageType::Nack,
        ];
        
        for msg_type in types {
            let header = StreamingHeader::new(msg_type, 1, 1, 0, 1, 0);
            let message = StreamingMessage::new(header, vec![]);
            let serialized = message.serialize();
            let deserialized = StreamingMessage::deserialize(&serialized).unwrap();
            
            assert_eq!(deserialized.header.message_type, msg_type);
        }
    }

    #[test]
    fn test_endianness_consistency() {
        let header = StreamingHeader::new(
            MessageType::DataChunk,
            0x1234,  // 16-bit
            0x12345678,  // 32-bit
            0xABCD,
            0x5678,
            100,
        );
        let message = StreamingMessage::new(header, vec![]);
        let serialized = message.serialize();
        
        // Little-endian確認
        assert_eq!(serialized[1], 0x34);  // sequence_id low byte
        assert_eq!(serialized[2], 0x12);  // sequence_id high byte
        
        assert_eq!(serialized[3], 0x78);  // frame_id byte 0
        assert_eq!(serialized[4], 0x56);  // frame_id byte 1
        assert_eq!(serialized[5], 0x34);  // frame_id byte 2
        assert_eq!(serialized[6], 0x12);  // frame_id byte 3
    }

    #[test]
    fn test_large_frame_id() {
        let header = StreamingHeader::new(
            MessageType::EndFrame,
            1,
            0xFFFFFFFF,  // Max u32
            0,
            1,
            0,
        );
        let message = StreamingMessage::new(header.clone(), vec![]);
        let serialized = message.serialize();
        let deserialized = StreamingMessage::deserialize(&serialized).unwrap();
        
        assert_eq!(deserialized.header.frame_id, 0xFFFFFFFF);
    }

    #[test]
    fn test_checksum_with_different_headers() {
        let data = vec![1, 2, 3, 4];
        
        let mut header1 = StreamingHeader::new(
            MessageType::DataChunk, 1, 1, 0, 1, 4
        );
        header1.calculate_checksum(&data);
        
        let mut header2 = StreamingHeader::new(
            MessageType::DataChunk, 2, 1, 0, 1, 4
        );
        header2.calculate_checksum(&data);
        
        // 異なるsequence_idなので異なるチェックサム
        assert_ne!(header1.checksum, header2.checksum);
    }

    // ==================== Phase 4A: Integration Tests ====================
    
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
