/*!
 * # M5Stack Unit Cam Image Sender Library
 *
 * ESP32カメラ画像を撮影して ESP-NOW プロトコルで送信するためのライブラリ
 *
 * ## モジュール構成
 * - `core`: アプリケーションの核となる機能（設定、データサービス、制御）
 * - `hardware`: ハードウェア制御（カメラ、LED、電圧センサー、ピン設定）
 * - `communication`: 通信機能（ESP-NOW、ネットワーク管理）
 * - `power`: 電源管理（ディープスリープ）
 */

// 公開モジュール
#[cfg(not(test))]
pub mod communication;
#[cfg(not(test))]
pub mod config;
pub mod core;  // テスト時も公開
#[cfg(not(test))]
pub mod hardware;
pub mod mac_address;
#[cfg(not(test))]
pub mod power;
pub mod utils;

// 内部で使用する型をまとめてエクスポート
#[cfg(not(test))]
pub use communication::esp_now::{EspNowError, EspNowSender, EspNowReceiver};
#[cfg(not(test))]
pub use config::{AppConfig, ConfigError, MemoryConfig};
pub use core::{DataService, MeasuredData};  // テスト時も公開
#[cfg(not(test))]
pub use hardware::camera::CameraController;
#[cfg(not(test))]
pub use hardware::led::status_led::{LedError, StatusLed};
#[cfg(not(test))]
pub use hardware::{CameraPins, VoltageSensor};
pub use mac_address::MacAddress;
pub use utils::calculate_voltage_percentage;

/// ライブラリのバージョン情報
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// 統合テスト
#[cfg(all(test, not(target_os = "espidf")))]
mod integration_tests {
    use super::*;
    use crate::core::MeasuredData;
    use crate::utils::streaming_protocol::StreamingMessage;

    #[test]
    fn test_measured_data_to_streaming_pipeline() {
        // Step 1: Create sensor data
        let measured_data = MeasuredData::new(75, None)
            .with_temperature(Some(25.3));

        assert_eq!(measured_data.voltage_percent, 75);
        assert_eq!(measured_data.temperature_celsius, Some(25.3));

        // Step 2: Verify summary
        let summary = measured_data.get_summary();
        assert!(summary.contains("電圧:75%"));
        assert!(summary.contains("温度:25.3°C"));
    }

    #[test]
    fn test_streaming_message_creation() {
        // Test streaming message creation with sample data
        let test_data = b"Test sensor data";
        let frame_id = 12345;

        // Create streaming messages
        let messages = StreamingMessage::create_data_stream(
            frame_id,
            test_data,
            200, // chunk size
        );

        // Verify messages were created
        assert!(!messages.is_empty());
        
        // Check first message is START
        if let StreamingMessage::Start { frame_id: fid, .. } = &messages[0] {
            assert_eq!(*fid, frame_id);
        } else {
            panic!("First message should be Start");
        }

        // Check last message is End
        if let StreamingMessage::End { frame_id: fid, .. } = messages.last().unwrap() {
            assert_eq!(*fid, frame_id);
        } else {
            panic!("Last message should be End");
        }
    }

    #[test]
    fn test_complete_data_flow_small_data() {
        // Simulate complete flow with small data (single chunk)
        let measured_data = MeasuredData::new(80, None)
            .with_temperature(Some(26.5))
            .with_tds(Some(450.2));

        // Verify data structure
        assert_eq!(measured_data.voltage_percent, 80);
        assert_eq!(measured_data.temperature_celsius, Some(26.5));
        assert_eq!(measured_data.tds_ppm, Some(450.2));

        // Create small payload
        let payload = format!(
            "{{\"voltage\":{},\"temp\":{:.1}}}",
            measured_data.voltage_percent,
            measured_data.temperature_celsius.unwrap_or(0.0)
        );

        // Convert to streaming messages
        let messages = StreamingMessage::create_data_stream(
            999,
            payload.as_bytes(),
            100,
        );

        // Verify structure
        assert!(messages.len() >= 2); // At least Start and End
        
        // Count message types
        let start_count = messages.iter().filter(|m| matches!(m, StreamingMessage::Start { .. })).count();
        let end_count = messages.iter().filter(|m| matches!(m, StreamingMessage::End { .. })).count();
        
        assert_eq!(start_count, 1);
        assert_eq!(end_count, 1);
    }

    #[test]
    fn test_complete_data_flow_large_data() {
        // Simulate large data scenario (multiple chunks)
        let large_data = vec![0xAB; 1000]; // 1KB of data
        let measured_data = MeasuredData::new(90, Some(large_data.clone()));

        assert_eq!(measured_data.image_data.as_ref().unwrap().len(), 1000);

        // Create streaming messages with small chunk size to force multiple chunks
        let frame_id = 54321;
        let messages = StreamingMessage::create_data_stream(
            frame_id,
            &large_data,
            200, // Small chunk size
        );

        // Verify multiple chunks were created
        let data_chunk_count = messages.iter()
            .filter(|m| matches!(m, StreamingMessage::DataChunk { .. }))
            .count();

        assert!(data_chunk_count > 1, "Should have multiple data chunks");

        // Verify sequence
        let mut sequence_ids = Vec::new();
        for msg in &messages {
            match msg {
                StreamingMessage::Start { sequence_id, .. } => sequence_ids.push(*sequence_id),
                StreamingMessage::DataChunk { sequence_id, .. } => sequence_ids.push(*sequence_id),
                StreamingMessage::End { sequence_id, .. } => sequence_ids.push(*sequence_id),
                _ => {}
            }
        }

        // All messages should have the same sequence ID within a stream
        if !sequence_ids.is_empty() {
            let first_seq = sequence_ids[0];
            assert!(sequence_ids.iter().all(|&id| id == first_seq), 
                "All messages in stream should have same sequence ID");
        }
    }

    #[test]
    fn test_measured_data_with_warnings() {
        // Test data flow with sensor warnings
        let mut measured_data = MeasuredData::new(45, None);
        measured_data.add_warning("Low voltage detected".to_string());
        measured_data.add_warning("Temperature sensor error".to_string());

        assert_eq!(measured_data.sensor_warnings.len(), 2);
        
        let summary = measured_data.get_summary();
        assert!(summary.contains("警告:2件"));
    }

    #[test]
    fn test_edge_case_zero_length_data() {
        // Test with zero-length data
        let empty_data = Vec::new();
        let messages = StreamingMessage::create_data_stream(
            111,
            &empty_data,
            100,
        );

        // Should still have Start and End
        assert!(messages.len() >= 2);
        
        let has_start = messages.iter().any(|m| matches!(m, StreamingMessage::Start { .. }));
        let has_end = messages.iter().any(|m| matches!(m, StreamingMessage::End { .. }));
        
        assert!(has_start);
        assert!(has_end);
    }

    #[test]
    fn test_edge_case_exact_chunk_size() {
        // Test with data exactly equal to chunk size
        let data = vec![0xFF; 200];
        let messages = StreamingMessage::create_data_stream(
            222,
            &data,
            200, // Exact size
        );

        let data_chunks = messages.iter()
            .filter(|m| matches!(m, StreamingMessage::DataChunk { .. }))
            .count();

        assert_eq!(data_chunks, 1, "Should have exactly one data chunk");
    }
}
