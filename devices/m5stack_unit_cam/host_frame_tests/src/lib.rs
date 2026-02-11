#![allow(dead_code)]

#[path = "../../src/communication/esp_now/frame_codec.rs"]
mod frame_codec;
#[path = "../../src/mac_address.rs"]
mod mac_address;

#[cfg(test)]
mod tests {
    use super::frame_codec::{
        build_hash_payload, build_sensor_data_frame, calculate_xor_checksum,
        safe_initial_payload_size, END_MARKER, ESP_NOW_MAX_SIZE, FRAME_OVERHEAD, START_MARKER,
    };
    use super::mac_address::MacAddress;

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
}
