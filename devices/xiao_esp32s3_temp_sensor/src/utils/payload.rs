/// ESP-NOW 送信用の HASH テキストペイロードを生成する
///
/// usb_cdc_receiver の detect_frame_type が先頭テキストでフレーム種別を識別するため、
/// バイナリフレームに包まず生テキストを直接送信する。
///
/// フィールドの意味:
/// - VOLT:100     電圧センサー非搭載のプレースホルダ
/// - TDS_VOLT:-999.0  TDS センサー非搭載のセンチネル値（サーバー側で None として扱われる）
pub fn format_hash_payload(temp: f32) -> String {
    const DUMMY_HASH: &str =
        "0000000000000000000000000000000000000000000000000000000000000000";
    format!(
        "HASH:{},VOLT:100,TEMP:{:.1},TDS_VOLT:-999.0,2000/01/01 00:00:00.000",
        DUMMY_HASH, temp
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_starts_with_hash_prefix() {
        assert!(format_hash_payload(25.0).starts_with("HASH:"));
    }

    #[test]
    fn test_payload_dummy_hash_64_zeros() {
        let p = format_hash_payload(25.0);
        let hash_part = p.strip_prefix("HASH:").unwrap().split(',').next().unwrap();
        assert_eq!(hash_part.len(), 64);
        assert!(hash_part.chars().all(|c| c == '0'));
    }

    #[test]
    fn test_payload_volt_placeholder() {
        assert!(format_hash_payload(25.0).contains("VOLT:100,"));
    }

    #[test]
    fn test_payload_tds_sentinel() {
        assert!(format_hash_payload(25.0).contains("TDS_VOLT:-999.0,"));
    }

    #[test]
    fn test_payload_temp_positive() {
        let p = format_hash_payload(25.0);
        assert!(p.contains("TEMP:25.0,"), "payload: {}", p);
    }

    #[test]
    fn test_payload_temp_negative() {
        let p = format_hash_payload(-5.0);
        assert!(p.contains("TEMP:-5.0,"), "payload: {}", p);
    }

    #[test]
    fn test_payload_temp_rounds_to_one_decimal() {
        // Rust の {:.1} は四捨五入
        let p = format_hash_payload(25.125);
        assert!(p.contains("TEMP:25.1,") || p.contains("TEMP:25.2,"), "payload: {}", p);
    }

    #[test]
    fn test_payload_ends_with_timestamp_placeholder() {
        assert!(format_hash_payload(25.0).ends_with("2000/01/01 00:00:00.000"));
    }

    #[test]
    fn test_eof_marker_literal() {
        // usb_cdc_receiver の detect_frame_type が "EOF!" で EOF を検出する
        assert_eq!(b"EOF!", b"EOF!");
    }
}
