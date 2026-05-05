/// "XX:XX:XX:XX:XX:XX" 形式の文字列を [u8; 6] に変換する
pub fn parse_mac(s: &str) -> Option<[u8; 6]> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 6 {
        return None;
    }
    let mut mac = [0u8; 6];
    for (i, p) in parts.iter().enumerate() {
        if p.len() != 2 {
            return None;
        }
        mac[i] = u8::from_str_radix(p, 16).ok()?;
    }
    Some(mac)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mac_typical() {
        assert_eq!(
            parse_mac("11:22:33:44:55:66"),
            Some([0x11, 0x22, 0x33, 0x44, 0x55, 0x66])
        );
    }

    #[test]
    fn test_parse_mac_all_zeros() {
        assert_eq!(parse_mac("00:00:00:00:00:00"), Some([0u8; 6]));
    }

    #[test]
    fn test_parse_mac_all_ff() {
        assert_eq!(parse_mac("FF:FF:FF:FF:FF:FF"), Some([0xff; 6]));
    }

    #[test]
    fn test_parse_mac_lowercase_hex() {
        assert_eq!(
            parse_mac("ab:cd:ef:00:11:22"),
            Some([0xab, 0xcd, 0xef, 0x00, 0x11, 0x22])
        );
    }

    #[test]
    fn test_parse_mac_too_few_segments() {
        assert_eq!(parse_mac("11:22:33:44:55"), None);
    }

    #[test]
    fn test_parse_mac_too_many_segments() {
        assert_eq!(parse_mac("11:22:33:44:55:66:77"), None);
    }

    #[test]
    fn test_parse_mac_invalid_hex() {
        assert_eq!(parse_mac("GG:22:33:44:55:66"), None);
    }

    #[test]
    fn test_parse_mac_empty_string() {
        assert_eq!(parse_mac(""), None);
    }

    #[test]
    fn test_parse_mac_wrong_delimiter() {
        assert_eq!(parse_mac("11-22-33-44-55-66"), None);
    }

    #[test]
    fn test_parse_mac_single_digit_segments() {
        // 各セグメントは必ず 2 桁の hex でなければならない
        assert_eq!(parse_mac("1:2:3:4:5:6"), None);
    }

    #[test]
    fn test_parse_mac_three_digit_segment() {
        assert_eq!(parse_mac("111:22:33:44:55:66"), None);
    }
}
