/// USB CDC Mock Integration Tests
/// 
/// このテストは、USB CDCインターフェースのMock実装を使用して、
/// ESP-NOWフレーム受信からUSB送信までのデータフローをテストします。
/// 
/// Frame::new()とFrame::to_bytes()を使用して、実際の実装と同じロジックでテストします。

use usb_cdc_receiver::esp_now::frame::Frame;
use usb_cdc_receiver::esp_now::FrameType;
use usb_cdc_receiver::usb::mock::MockUsbCdc;
use usb_cdc_receiver::usb::UsbInterface;

#[test]
fn test_usb_send_esp_now_frame() {
    // Mock USB CDCを作成
    let mut mock_usb = MockUsbCdc::new();

    // ESP-NOWフレームを作成（実際のxiao_esp32s3_senseから送信されるフォーマット）
    let mac_address = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    let sequence_num = 1u32;
    let frame_type = FrameType::Data;
    let payload = vec![0x01, 0x02, 0x03, 0x04, 0x05];

    // フレームをバイト列に変換（Frame::to_bytes()を使用）
    let frame = Frame::new(mac_address, frame_type, sequence_num, payload.clone());
    let frame_bytes = frame.to_bytes();

    // USB経由でフレームを送信
    let result = mock_usb.send_frame(&frame_bytes, "AA:BB:CC:DD:EE:FF");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), frame_bytes.len());

    // 送信されたデータを検証
    let sent_data = mock_usb.get_sent_data();
    assert_eq!(sent_data.len(), 1);
    assert_eq!(sent_data[0], frame_bytes);
}

#[test]
fn test_usb_receive_sleep_command() {
    // Mock USB CDCを作成
    let mut mock_usb = MockUsbCdc::new();

    // スリープコマンドをキューに追加
    mock_usb.queue_command("SLEEP 300".to_string());

    // コマンドを読み取る
    let result = mock_usb.read_command(100);
    assert!(result.is_ok());

    let command = result.unwrap();
    assert!(command.is_some());
    assert_eq!(command.unwrap(), "SLEEP 300");
}

#[test]
fn test_usb_send_large_frame() {
    // Mock USB CDCを作成
    let mut mock_usb = MockUsbCdc::new();

    // 大きなペイロード（画像データをシミュレート）
    let large_payload = vec![0xAB; 10000]; // 10KB
    let mac_address = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
    let frame = Frame::new(mac_address, FrameType::Data, 1, large_payload.clone());
    let frame_bytes = frame.to_bytes();

    // USB経由で送信
    let result = mock_usb.send_frame(&frame_bytes, "11:22:33:44:55:66");
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), frame_bytes.len());

    // データが正しく送信されたか確認
    let sent_data = mock_usb.get_sent_data();
    assert_eq!(sent_data.len(), 1);
    assert_eq!(sent_data[0].len(), frame_bytes.len());
}

#[test]
fn test_usb_error_handling_write_error() {
    // Mock USB CDCを作成
    let mut mock_usb = MockUsbCdc::new();

    // 書き込みエラーをシミュレート
    mock_usb.set_write_error(true);

    let test_data = b"test data";
    let result = mock_usb.write(test_data, 100);

    assert!(result.is_err());
}

#[test]
fn test_usb_error_handling_timeout() {
    // Mock USB CDCを作成
    let mut mock_usb = MockUsbCdc::new();

    // タイムアウトをシミュレート
    mock_usb.set_timeout(true);

    let test_data = b"test data";
    let result = mock_usb.write(test_data, 100);

    assert!(result.is_err());
}

#[test]
fn test_usb_multiple_frames() {
    // Mock USB CDCを作成
    let mut mock_usb = MockUsbCdc::new();

    // 複数のフレームを送信
    let mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    
    for i in 0..5 {
        let payload = vec![i as u8; 100];
        let frame = Frame::new(mac, FrameType::Data, i as u32, payload);
        let frame_bytes = frame.to_bytes();
        let result = mock_usb.send_frame(&frame_bytes, "AA:BB:CC:DD:EE:FF");
        assert!(result.is_ok());
    }

    // 5つのフレームが送信されたことを確認
    let sent_data = mock_usb.get_sent_data();
    assert_eq!(sent_data.len(), 5);
}

#[test]
fn test_usb_read_write_sequence() {
    // Mock USB CDCを作成
    let mut mock_usb = MockUsbCdc::new();

    // 書き込み
    let write_data = b"Request data";
    mock_usb.write(write_data, 100).unwrap();

    // 読み取り用データをキューに追加
    mock_usb.queue_read_data(b"Response data".to_vec());

    // 読み取り
    let mut buffer = [0u8; 128];
    let result = mock_usb.read(&mut buffer, 100);
    assert!(result.is_ok());

    let bytes_read = result.unwrap();
    assert_eq!(&buffer[..bytes_read], b"Response data");

    // 送信データを検証
    let sent = mock_usb.get_sent_data();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0], write_data);
}

#[test]
fn test_frame_creation_helper() {
    // Frame::to_bytes()を使用したテスト
    let mac = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
    let payload = vec![0xAA, 0xBB, 0xCC];
    let frame = Frame::new(mac, FrameType::Data, 1, payload.clone());
    let frame_bytes = frame.to_bytes();

    // フレーム構造を検証
    assert_eq!(&frame_bytes[0..4], &0xFACEAABBu32.to_be_bytes()); // START
    assert_eq!(&frame_bytes[4..10], &mac); // MAC
    assert_eq!(frame_bytes[10], FrameType::Data as u8); // Type
    assert_eq!(&frame_bytes[11..15], &1u32.to_le_bytes()); // Sequence
    assert_eq!(&frame_bytes[15..19], &3u32.to_le_bytes()); // Data Length

    // END MARKER位置を計算
    let end_pos = frame_bytes.len() - 4;
    assert_eq!(&frame_bytes[end_pos..], &0xCDEF5678u32.to_be_bytes());
}

#[test]
fn test_usb_data_flow_integration() {
    // 統合テスト: ESP-NOW受信 → USB送信のフローをシミュレート
    let mut mock_usb = MockUsbCdc::new();

    // 1. ESP-NOWフレームを受信したと仮定
    let mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
    let sensor_data = vec![
        0x12, 0x34, // 温度データ
        0x56, 0x78, // 湿度データ
        0x9A, 0xBC, // 電圧データ
    ];
    let frame = Frame::new(mac, FrameType::Data, 10, sensor_data.clone());
    let frame_bytes = frame.to_bytes();

    // 2. フレームを解析（esp_now/frame.rsの機能を使用）
    let parsed_frame = Frame::from_bytes(&frame_bytes);
    assert!(parsed_frame.is_some(), "Frame parsing should succeed");

    let (parsed, _consumed_bytes) = parsed_frame.unwrap();
    assert_eq!(parsed.mac_address(), &mac);
    assert_eq!(parsed.sequence_number(), 10);
    assert_eq!(parsed.data(), &sensor_data[..]);

    // 3. USB経由でPCに送信
    let result = mock_usb.send_frame(&frame_bytes, "AA:BB:CC:DD:EE:FF");
    assert!(result.is_ok());

    // 4. 送信されたデータを検証
    let sent = mock_usb.get_sent_data();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0], frame_bytes);
}
