// ESP-NOW Frame Parser Unit Tests
// これらのテストはホストマシンで実行されます

use usb_cdc_receiver::esp_now::FrameType;
use usb_cdc_receiver::esp_now::frame::{Frame, calculate_checksum, detect_frame_type};

#[test]
fn test_checksum_calculation() {
    // 空データ
    assert_eq!(calculate_checksum(&[]), 0);
    
    // 単純なケース
    assert_eq!(calculate_checksum(&[1, 0, 0, 0]), 1);
    assert_eq!(calculate_checksum(&[1, 2, 3, 4]), 0x04030201);
    
    // XORの性質: 同じデータを2回XORすると0になる
    assert_eq!(
        calculate_checksum(&[1, 2, 3, 4, 1, 2, 3, 4]),
        0
    );
    
    // より複雑なケース
    let data = b"Hello, World!";
    let checksum1 = calculate_checksum(data);
    let checksum2 = calculate_checksum(data);
    assert_eq!(checksum1, checksum2, "Checksum should be deterministic");
}

#[test]
fn test_frame_serialization_deserialization() {
    let mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    let test_data = b"Test payload data";
    let seq = 12345;
    
    // フレーム作成
    let frame = Frame::new(
        mac,
        FrameType::Data,
        seq,
        test_data.to_vec(),
    );
    
    // シリアライズ
    let bytes = frame.to_bytes();
    
    // デシリアライズ
    let (parsed_frame, size) = Frame::from_bytes(&bytes)
        .expect("Frame parsing should succeed");
    
    // 検証
    assert_eq!(size, bytes.len(), "Parsed size should match frame size");
    assert_eq!(parsed_frame.mac_address(), &mac, "MAC address should match");
    assert_eq!(parsed_frame.frame_type(), FrameType::Data, "Frame type should match");
    assert_eq!(parsed_frame.sequence_number(), seq, "Sequence number should match");
    assert_eq!(parsed_frame.data(), test_data, "Payload data should match");
}

#[test]
fn test_frame_types() {
    let mac = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
    
    // Hash Frame
    let hash_frame = Frame::new(mac, FrameType::Hash, 1, vec![1, 2, 3]);
    let bytes = hash_frame.to_bytes();
    let (parsed, _) = Frame::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.frame_type(), FrameType::Hash);
    
    // Data Frame
    let data_frame = Frame::new(mac, FrameType::Data, 2, vec![4, 5, 6]);
    let bytes = data_frame.to_bytes();
    let (parsed, _) = Frame::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.frame_type(), FrameType::Data);
    
    // EOF Frame
    let eof_frame = Frame::new(mac, FrameType::Eof, 3, vec![]);
    let bytes = eof_frame.to_bytes();
    let (parsed, _) = Frame::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.frame_type(), FrameType::Eof);
}

#[test]
fn test_invalid_frame_too_short() {
    let short_data = vec![0xFF; 10]; // 最小フレームサイズ以下
    assert!(Frame::from_bytes(&short_data).is_none(), "Short data should fail parsing");
}

#[test]
fn test_invalid_start_marker() {
    let mac = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
    let frame = Frame::new(mac, FrameType::Data, 1, vec![1, 2, 3]);
    let mut bytes = frame.to_bytes();
    
    // 開始マーカーを破壊
    bytes[0] = 0x00;
    
    assert!(Frame::from_bytes(&bytes).is_none(), "Invalid start marker should fail");
}

#[test]
fn test_invalid_checksum() {
    let mac = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
    let frame = Frame::new(mac, FrameType::Data, 1, vec![1, 2, 3]);
    let mut bytes = frame.to_bytes();
    
    // チェックサムの位置を特定して破壊 (終了マーカーの4バイト前)
    let checksum_pos = bytes.len() - 8;
    bytes[checksum_pos] ^= 0xFF;
    
    assert!(Frame::from_bytes(&bytes).is_none(), "Invalid checksum should fail");
}

#[test]
fn test_invalid_end_marker() {
    let mac = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
    let frame = Frame::new(mac, FrameType::Data, 1, vec![1, 2, 3]);
    let mut bytes = frame.to_bytes();
    
    // 終了マーカーを破壊
    let last_pos = bytes.len() - 1;
    bytes[last_pos] = 0x00;
    
    assert!(Frame::from_bytes(&bytes).is_none(), "Invalid end marker should fail");
}

#[test]
fn test_large_payload() {
    let mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    let large_data = vec![0x42; 1000]; // 1000バイトのペイロード
    let seq = 999;
    
    let frame = Frame::new(mac, FrameType::Data, seq, large_data.clone());
    let bytes = frame.to_bytes();
    let (parsed, size) = Frame::from_bytes(&bytes).unwrap();
    
    assert_eq!(size, bytes.len());
    assert_eq!(parsed.data(), &large_data[..]);
}

#[test]
fn test_empty_payload() {
    let mac = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
    let empty_data = vec![];
    let seq = 0;
    
    let frame = Frame::new(mac, FrameType::Eof, seq, empty_data);
    let bytes = frame.to_bytes();
    let (parsed, _) = Frame::from_bytes(&bytes).unwrap();
    
    assert_eq!(parsed.data().len(), 0);
    assert_eq!(parsed.frame_type(), FrameType::Eof);
}

#[test]
fn test_detect_frame_type_eof() {
    assert_eq!(detect_frame_type(b"EOF!"), FrameType::Eof);
}

#[test]
fn test_detect_frame_type_hash() {
    assert_eq!(detect_frame_type(b"HASH:abc123"), FrameType::Hash);
    assert_eq!(detect_frame_type(b"HASH:1"), FrameType::Hash); // 最小ケース: 長さ6
}

#[test]
fn test_detect_frame_type_data() {
    assert_eq!(detect_frame_type(b"normal data"), FrameType::Data);
    assert_eq!(detect_frame_type(b"anything else"), FrameType::Data);
    assert_eq!(detect_frame_type(b""), FrameType::Data);
}

#[test]
fn test_sequence_number_overflow() {
    let mac = [0xFF; 6];
    let data = vec![0xAA; 10];
    
    // 最大値のシーケンス番号
    let frame = Frame::new(mac, FrameType::Data, u32::MAX, data.clone());
    let bytes = frame.to_bytes();
    let (parsed, _) = Frame::from_bytes(&bytes).unwrap();
    
    assert_eq!(parsed.sequence_number(), u32::MAX);
}
