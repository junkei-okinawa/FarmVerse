#[cfg(test)]
mod tests {
    use usb_cdc_receiver::streaming::device_manager::{DeviceStreamManager, StreamManagerConfig};
    use usb_cdc_receiver::esp_now::FrameType;
    use usb_cdc_receiver::esp_now::frame::calculate_checksum;

    // ヘルパー：フレームを作成する
    fn create_frame(mac: [u8; 6], sequence: u16, payload: &[u8]) -> Vec<u8> {
        // START_MARKER + MAC + FRAME_TYPE + SEQ + LEN + PAYLOAD + CHECKSUM + END_MARKER
        let mut frame = Vec::new();
        
        // frame.rs:
        // pub const START_MARKER: u32 = 0xFACE_AABB;
        // let start_marker_bytes = START_MARKER.to_be_bytes();
        // framed_data.extend_from_slice(&start_marker_bytes);
        
        // Let's verify markers from frame.rs
        // START_MARKER = 0xFACE_AABB (BE) -> [0xFA, 0xCE, 0xAA, 0xBB]
        // END_MARKER = 0xCDEF_5678 (BE) -> [0xCD, 0xEF, 0x56, 0x78]
        
        frame.extend_from_slice(&[0xFA, 0xCE, 0xAA, 0xBB]); // START_MARKER
        frame.extend_from_slice(&mac);
        frame.push(FrameType::Data.to_byte());
        frame.extend_from_slice(&(sequence as u32).to_le_bytes()); // SEQ is u32 in Frame, not u16!
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // LEN is u32 in Frame
        frame.extend_from_slice(payload);
        
        // Checksum is calculated on payload only!
        let checksum = calculate_checksum(payload);
        frame.extend_from_slice(&checksum.to_le_bytes());
        
        frame.extend_from_slice(&[0xCD, 0xEF, 0x56, 0x78]); // END_MARKER
        frame
    }

    #[test]
    fn test_process_valid_frame() {
        let config = StreamManagerConfig::default();
        let mut manager = DeviceStreamManager::new(config);
        
        let mac = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        let payload = b"Hello World";
        let sequence = 123;
        
        let frame_bytes = create_frame(mac, sequence, payload);
        
        let result = manager.process_data(mac, &frame_bytes);
        assert!(result.is_ok());
        
        let processed_frames = result.unwrap();
        assert_eq!(processed_frames.len(), 1);
        
        let frame = &processed_frames[0];
        assert_eq!(frame.sequence, sequence as u32);
        assert_eq!(frame.mac, mac);
        assert_eq!(frame.full_frame, frame_bytes); // full_frame にはバイト列全体が入るはず
        
        // 統計確認
        let stats = manager.global_statistics();
        assert_eq!(stats.frames_received, 1);
        assert_eq!(stats.frames_processed, 1);
        assert_eq!(stats.frames_error, 0);
    }

    #[test]
    fn test_process_invalid_checksum() {
        let config = StreamManagerConfig::default();
        let mut manager = DeviceStreamManager::new(config);
        
        let mac = [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let payload = b"Bad Frame";
        let sequence = 999;
        
        // 正しいフレームを作成
        let mut frame_bytes = create_frame(mac, sequence, payload);
        
        // データを改変してチェックサム不整合を起こす
        // ペイロード部分のバイトを変更
        // HEADER(4+6+1+4+4=19bytes) + PAYLOAD
        let payload_idx = 19;
        frame_bytes[payload_idx] = frame_bytes[payload_idx].wrapping_add(1); 
        
        let result = manager.process_data(mac, &frame_bytes);
        assert!(result.is_ok()); // エラーでも Ok(empty) を返す仕様
        
        let processed_frames = result.unwrap();
        assert!(processed_frames.is_empty());
        
        // 統計確認
        let stats = manager.global_statistics();
        assert_eq!(stats.frames_received, 1);
        assert_eq!(stats.frames_processed, 0);
        assert_eq!(stats.frames_error, 1);
        assert_eq!(stats.checksum_error_count, 1);
    }
    
    #[test]
    fn test_process_garbage_data() {
        let config = StreamManagerConfig::default();
        let mut manager = DeviceStreamManager::new(config);
        
        let mac = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let garbage = b"This is not a frame";
        
        let result = manager.process_data(mac, garbage);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
        
        let stats = manager.global_statistics();
        assert_eq!(stats.frames_received, 1);
        assert_eq!(stats.frames_error, 1);
    }
}