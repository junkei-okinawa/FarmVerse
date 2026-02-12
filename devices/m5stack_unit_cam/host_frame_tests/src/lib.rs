#![allow(dead_code)]

extern crate thiserror;

#[path = "../../src/core/capture_policy.rs"]
mod capture_policy;
#[path = "../../src/communication/esp_now/frame_codec.rs"]
mod frame_codec;
#[path = "../../src/communication/esp_now/frame.rs"]
mod frame;
#[path = "../../src/communication/esp_now/retry_policy.rs"]
mod retry_policy;
#[path = "../../src/core/config_validation.rs"]
mod config_validation;
#[path = "../../src/core/data_prep.rs"]
mod data_prep;
#[path = "../../src/core/domain_logic.rs"]
mod domain_logic;
#[path = "../../src/mac_address.rs"]
mod mac_address;

#[cfg(test)]
mod tests {
    use super::config_validation::{
        parse_camera_warmup_frames, parse_receiver_mac, parse_target_minute_last_digit,
        parse_target_second_tens_digit, validate_wifi_ssid, ValidationError,
    };
    use super::capture_policy::{should_capture_image, INVALID_VOLTAGE_PERCENT, LOW_VOLTAGE_THRESHOLD_PERCENT};
    use super::data_prep::{prepare_image_payload, simple_image_hash, DUMMY_HASH};
    use super::domain_logic::{resolve_sleep_duration_seconds, voltage_to_percentage};
    use super::frame::ImageFrame;
    use super::frame_codec::{
        build_hash_payload, build_sensor_data_frame, calculate_xor_checksum,
        payload_size_candidates, safe_initial_payload_size, END_MARKER, ESP_NOW_MAX_SIZE,
        FRAME_OVERHEAD, START_MARKER,
    };
    use super::mac_address::MacAddress;
    use super::retry_policy::{no_mem_retry_delay_ms, retry_delay_ms};

    #[test]
    fn checksum_uses_little_endian_4byte_chunks() {
        // 0x04030201 ^ 0x08070605 = 0x0C040404
        let data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let checksum = calculate_xor_checksum(&data);
        assert_eq!(checksum, 0x0C040404);
    }

    #[test]
    fn frame_structure_matches_sensor_data_protocol() {
        let mac = [0x10, 0x11, 0x12, 0x13, 0x14, 0x15];
        let sequence = 0x01020304;
        let payload = [0xAA, 0xBB, 0xCC];
        let frame = build_sensor_data_frame(2, mac, sequence, &payload);

        let expected_len = 4 + 6 + 1 + 4 + 4 + payload.len() + 4 + 4;
        assert_eq!(frame.len(), expected_len);

        assert_eq!(&frame[0..4], &START_MARKER);
        assert_eq!(&frame[4..10], &mac);
        assert_eq!(frame[10], 2);
        assert_eq!(&frame[11..15], &sequence.to_le_bytes());
        assert_eq!(&frame[15..19], &(payload.len() as u32).to_le_bytes());
        assert_eq!(&frame[19..22], &payload);

        let checksum_offset = 19 + payload.len();
        let checksum = calculate_xor_checksum(&payload);
        assert_eq!(
            &frame[checksum_offset..checksum_offset + 4],
            &checksum.to_le_bytes()
        );
        assert_eq!(&frame[checksum_offset + 4..checksum_offset + 8], &END_MARKER);
    }

    #[test]
    fn payload_size_is_capped_to_esp_now_limit() {
        let capped = safe_initial_payload_size(999);
        assert_eq!(capped, ESP_NOW_MAX_SIZE - FRAME_OVERHEAD);
    }

    #[test]
    fn payload_size_keeps_small_value() {
        let kept = safe_initial_payload_size(120);
        assert_eq!(kept, 120);
    }

    #[test]
    fn payload_size_candidates_are_ordered_fallbacks() {
        let candidates = payload_size_candidates(999);
        assert_eq!(candidates, [223, 150, 100, 50, 30]);
    }

    #[test]
    fn hash_payload_uses_dummy_values_when_missing_optional_fields() {
        let payload = build_hash_payload("abc", 42, None, None, "2026/02/11 12:00:00.000");
        assert_eq!(
            payload,
            "HASH:abc,VOLT:42,TEMP:-999.0,TDS_VOLT:-999.0,2026/02/11 12:00:00.000"
        );
    }

    #[test]
    fn hash_payload_uses_provided_optional_fields() {
        let payload =
            build_hash_payload("abc", 42, Some(25.2), Some(1.7), "2026/02/11 12:00:00.000");
        assert_eq!(
            payload,
            "HASH:abc,VOLT:42,TEMP:25.2,TDS_VOLT:1.7,2026/02/11 12:00:00.000"
        );
    }

    #[test]
    fn mac_address_parse_and_display_roundtrip() {
        let mac = MacAddress::from_str("aa:bb:cc:dd:ee:ff").unwrap();
        assert_eq!(mac.to_string(), "aa:bb:cc:dd:ee:ff");
    }

    #[test]
    fn mac_address_invalid_format_returns_error() {
        let result = MacAddress::from_str("aa:bb:cc");
        assert!(result.is_err());
    }

    #[test]
    fn voltage_to_percentage_returns_zero_when_range_invalid() {
        assert_eq!(voltage_to_percentage(3000.0, 2000.0, 2000.0), 0);
    }

    #[test]
    fn voltage_to_percentage_clamps_low() {
        assert_eq!(voltage_to_percentage(100.0, 500.0, 2500.0), 0);
    }

    #[test]
    fn voltage_to_percentage_clamps_high() {
        assert_eq!(voltage_to_percentage(3000.0, 500.0, 2500.0), 100);
    }

    #[test]
    fn voltage_to_percentage_rounds_middle_value() {
        assert_eq!(voltage_to_percentage(1500.0, 500.0, 2500.0), 50);
    }

    #[test]
    fn resolve_sleep_duration_prefers_received_positive_value() {
        assert_eq!(resolve_sleep_duration_seconds(Some(123), 999), 123);
    }

    #[test]
    fn resolve_sleep_duration_uses_default_on_none() {
        assert_eq!(resolve_sleep_duration_seconds(None, 999), 999);
    }

    #[test]
    fn resolve_sleep_duration_uses_default_on_zero() {
        assert_eq!(resolve_sleep_duration_seconds(Some(0), 999), 999);
    }

    #[test]
    fn image_frame_calculate_hash_empty_is_error() {
        let result = ImageFrame::calculate_hash(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn image_frame_calculate_hash_known_value() {
        let hash = ImageFrame::calculate_hash(b"test data").unwrap();
        assert_eq!(
            hash,
            "916f0027a575074ce72a331777c3478d6513f786a591bd892da1a577bf2335f9"
        );
    }

    #[test]
    fn parse_receiver_mac_rejects_placeholder() {
        let err = parse_receiver_mac("11:22:33:44:55:66").unwrap_err();
        assert_eq!(err, ValidationError::MissingReceiverMac);
    }

    #[test]
    fn parse_receiver_mac_accepts_valid_value() {
        let mac = parse_receiver_mac("00:11:22:33:44:55").unwrap();
        assert_eq!(mac.to_string(), "00:11:22:33:44:55");
    }

    #[test]
    fn parse_camera_warmup_frames_returns_none_for_255() {
        assert_eq!(parse_camera_warmup_frames(255).unwrap(), None);
    }

    #[test]
    fn parse_camera_warmup_frames_rejects_large_value() {
        let err = parse_camera_warmup_frames(11).unwrap_err();
        assert_eq!(err, ValidationError::InvalidCameraWarmupFrames(11));
    }

    #[test]
    fn parse_target_digits_support_none_sentinel() {
        assert_eq!(parse_target_minute_last_digit(255).unwrap(), None);
        assert_eq!(parse_target_second_tens_digit(255).unwrap(), None);
    }

    #[test]
    fn parse_target_digits_reject_out_of_range() {
        assert_eq!(
            parse_target_minute_last_digit(10).unwrap_err(),
            ValidationError::InvalidTargetMinuteLastDigit(10)
        );
        assert_eq!(
            parse_target_second_tens_digit(6).unwrap_err(),
            ValidationError::InvalidTargetSecondLastDigit(6)
        );
    }

    #[test]
    fn validate_wifi_ssid_rejects_empty() {
        let err = validate_wifi_ssid("").unwrap_err();
        assert_eq!(err, ValidationError::MissingWifiSsid);
    }

    #[test]
    fn retry_delay_uses_linear_backoff() {
        assert_eq!(retry_delay_ms(1), 300);
        assert_eq!(retry_delay_ms(2), 600);
        assert_eq!(retry_delay_ms(3), 900);
    }

    #[test]
    fn no_mem_retry_delay_uses_longer_backoff() {
        assert_eq!(no_mem_retry_delay_ms(1), 1200);
        assert_eq!(no_mem_retry_delay_ms(2), 1600);
        assert_eq!(no_mem_retry_delay_ms(3), 2000);
    }

    #[test]
    fn simple_image_hash_matches_length_and_sum() {
        let hash = simple_image_hash(&[1, 2, 3]);
        assert_eq!(hash, "0000000300000006");
    }

    #[test]
    fn prepare_image_payload_uses_dummy_for_none() {
        let (data, hash) = prepare_image_payload(None);
        assert!(data.is_empty());
        assert_eq!(hash, DUMMY_HASH);
    }

    #[test]
    fn prepare_image_payload_uses_dummy_for_empty_data() {
        let (data, hash) = prepare_image_payload(Some(vec![]));
        assert!(data.is_empty());
        assert_eq!(hash, DUMMY_HASH);
    }

    #[test]
    fn prepare_image_payload_returns_data_and_hash_for_valid_data() {
        let (data, hash) = prepare_image_payload(Some(vec![1, 2, 3]));
        assert_eq!(data, vec![1, 2, 3]);
        assert_eq!(hash, "0000000300000006");
    }

    #[test]
    fn should_capture_image_rejects_low_voltage_threshold_and_below() {
        assert!(!should_capture_image(LOW_VOLTAGE_THRESHOLD_PERCENT));
        assert!(!should_capture_image(LOW_VOLTAGE_THRESHOLD_PERCENT - 1));
    }

    #[test]
    fn should_capture_image_accepts_normal_range() {
        assert!(should_capture_image(9));
        assert!(should_capture_image(100));
    }

    #[test]
    fn should_capture_image_rejects_invalid_voltage_marker() {
        assert!(!should_capture_image(INVALID_VOLTAGE_PERCENT));
    }
}
