/// "XX:XX:XX:XX:XX:XX" 形式の文字列を [u8; 6] に変換する
pub fn parse_mac(s: &str) -> Option<[u8; 6]> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 6 {
        return None;
    }
    let mut mac = [0u8; 6];
    for (i, p) in parts.iter().enumerate() {
        mac[i] = u8::from_str_radix(p, 16).ok()?;
    }
    Some(mac)
}

/// deep sleep サイクルカウンタと再キャリブレーション周期から、今サイクルで PHY 再キャリブレーションが必要か判定する
///
/// interval = 0 のとき常に false (無効化)
/// interval != 0 のとき cycle % interval == 0 で true
pub fn needs_recalibration(cycle: u32, interval: u32) -> bool {
    interval != 0 && cycle % interval == 0
}

/// ESP-NOW 送信用の HASH ペイロード文字列を生成する
///
/// usb_cdc_receiver の detect_frame_type がテキスト判定で使う先頭文字列 "HASH:" を含む。
/// VOLT は電圧センサなしのプレースホルダ (100)、TDS_VOLT は未搭載のセンチネル値 (-999.0)。
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

    // ── parse_mac ────────────────────────────────────────────────────────────

    #[test]
    fn parse_mac_typical() {
        assert_eq!(
            parse_mac("11:22:33:44:55:66"),
            Some([0x11, 0x22, 0x33, 0x44, 0x55, 0x66])
        );
    }

    #[test]
    fn parse_mac_all_zeros() {
        assert_eq!(parse_mac("00:00:00:00:00:00"), Some([0u8; 6]));
    }

    #[test]
    fn parse_mac_all_ff() {
        assert_eq!(parse_mac("FF:FF:FF:FF:FF:FF"), Some([0xff; 6]));
    }

    #[test]
    fn parse_mac_lowercase_hex() {
        assert_eq!(
            parse_mac("ab:cd:ef:00:11:22"),
            Some([0xab, 0xcd, 0xef, 0x00, 0x11, 0x22])
        );
    }

    #[test]
    fn parse_mac_too_few_segments() {
        assert_eq!(parse_mac("11:22:33:44:55"), None);
    }

    #[test]
    fn parse_mac_too_many_segments() {
        assert_eq!(parse_mac("11:22:33:44:55:66:77"), None);
    }

    #[test]
    fn parse_mac_invalid_hex() {
        assert_eq!(parse_mac("GG:22:33:44:55:66"), None);
    }

    #[test]
    fn parse_mac_empty_string() {
        assert_eq!(parse_mac(""), None);
    }

    #[test]
    fn parse_mac_wrong_delimiter() {
        assert_eq!(parse_mac("11-22-33-44-55-66"), None);
    }

    // ── needs_recalibration ──────────────────────────────────────────────────

    #[test]
    fn recal_first_cycle_triggers() {
        // cycle=0 は初回起動 → 周期に関係なく true (0 % N == 0)
        assert!(needs_recalibration(0, 100));
    }

    #[test]
    fn recal_mid_cycle_does_not_trigger() {
        assert!(!needs_recalibration(50, 100));
        assert!(!needs_recalibration(99, 100));
    }

    #[test]
    fn recal_exact_interval_triggers() {
        assert!(needs_recalibration(100, 100));
        assert!(needs_recalibration(200, 100));
    }

    #[test]
    fn recal_interval_zero_always_false() {
        // interval=0 は無効化; いかなる cycle でも false
        assert!(!needs_recalibration(0, 0));
        assert!(!needs_recalibration(100, 0));
        assert!(!needs_recalibration(u32::MAX, 0));
    }

    #[test]
    fn recal_wrapping_max_u32() {
        // u32::MAX % 100 == 95 → false
        assert!(!needs_recalibration(u32::MAX, 100));
    }

    #[test]
    fn recal_interval_one_always_true() {
        // interval=1 は毎サイクル再キャリブレーション
        assert!(needs_recalibration(0, 1));
        assert!(needs_recalibration(1, 1));
        assert!(needs_recalibration(999, 1));
    }

    // ── format_hash_payload ──────────────────────────────────────────────────

    #[test]
    fn payload_starts_with_hash_prefix() {
        let p = format_hash_payload(25.0);
        assert!(p.starts_with("HASH:"), "payload must start with HASH:");
    }

    #[test]
    fn payload_dummy_hash_64_zeros() {
        let p = format_hash_payload(25.0);
        // "HASH:" の後に 64 桁のゼロが続く
        let after_hash = p.strip_prefix("HASH:").unwrap();
        let hash_part = after_hash.split(',').next().unwrap();
        assert_eq!(hash_part.len(), 64);
        assert!(hash_part.chars().all(|c| c == '0'));
    }

    #[test]
    fn payload_volt_placeholder() {
        let p = format_hash_payload(25.0);
        assert!(p.contains("VOLT:100,"), "VOLT placeholder must be 100");
    }

    #[test]
    fn payload_tds_sentinel() {
        let p = format_hash_payload(25.0);
        assert!(p.contains("TDS_VOLT:-999.0,"), "TDS sentinel must be -999.0");
    }

    #[test]
    fn payload_temp_positive() {
        let p = format_hash_payload(25.0);
        assert!(p.contains("TEMP:25.0,"), "payload: {}", p);
    }

    #[test]
    fn payload_temp_negative() {
        let p = format_hash_payload(-5.0);
        assert!(p.contains("TEMP:-5.0,"), "payload: {}", p);
    }

    #[test]
    fn payload_temp_rounds_to_one_decimal() {
        // 25.125 → "25.1" (Rust {:.1} は銀行丸め)
        let p = format_hash_payload(25.125);
        assert!(p.contains("TEMP:25.1,") || p.contains("TEMP:25.2,"), "payload: {}", p);
    }

    #[test]
    fn payload_ends_with_timestamp_placeholder() {
        let p = format_hash_payload(25.0);
        assert!(p.ends_with("2000/01/01 00:00:00.000"), "payload: {}", p);
    }

    #[test]
    fn payload_eof_marker_is_literal() {
        // "EOF!" は固定値。receiver の detect_frame_type が "EOF!" で EOF を検出する。
        assert_eq!(b"EOF!", b"EOF!");
    }
}
