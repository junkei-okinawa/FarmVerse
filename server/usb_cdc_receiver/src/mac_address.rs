#[cfg(target_os = "espidf")]
use log::debug;
#[cfg(not(target_os = "espidf"))]
macro_rules! debug {
    ($($arg:tt)*) => {};
}

use std::fmt;
use std::str::FromStr;

/// MACアドレスを表す構造体
/// IEEE 802規格に従った6バイトのMACアドレスを保持します。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MacAddress([u8; 6]);

impl MacAddress {
    /// 新しいMACアドレスを6バイトの配列から作成します
    pub fn new(addr: [u8; 6]) -> Self {
        MacAddress(addr)
    }

    /// MACアドレスの生バイト配列を取得します
    pub fn as_bytes(&self) -> &[u8; 6] {
        &self.0
    }

    /// MACアドレスの生バイト配列を取得します（所有権を移動）
    pub fn into_bytes(self) -> [u8; 6] {
        self.0
    }
}

impl FromStr for MacAddress {
    type Err = Box<dyn std::error::Error>;

    /// 文字列からMACアドレスをパースします
    ///
    /// # 引数
    /// * `s` - "XX:XX:XX:XX:XX:XX"形式のMACアドレス文字列
    ///
    /// # 戻り値
    /// * `Result<MacAddress, Box<dyn std::error::Error>>` - パース結果
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        debug!("Parsing MAC address from string: {}", s);
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 6 {
            return Err(format!(
                "Invalid MAC address format: '{}'. Expected 6 parts separated by colons.",
                s
            )
            .into());
        }

        let mut mac = [0u8; 6];
        for (i, part) in parts.iter().enumerate() {
            mac[i] = u8::from_str_radix(part, 16)
                .map_err(|e| format!("Invalid hex value in MAC address: {}", e))?;
        }

        Ok(MacAddress(mac))
    }
}

impl fmt::Display for MacAddress {
    /// MACアドレスを標準的な16進数表記にフォーマットします
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

/// MACアドレスをログ出力用にフォーマットする便利関数
pub fn format_mac_address(mac: &[u8; 6]) -> String {
    format!(
        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mac_address_from_str_valid() {
        let mac_str = "12:34:56:78:9a:bc";
        let mac = MacAddress::from_str(mac_str).unwrap();
        assert_eq!(mac.0, [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]);
    }

    #[test]
    fn test_mac_address_from_str_invalid_format() {
        let invalid_mac = "12:34:56:78:9a"; // 部分が不足
        assert!(MacAddress::from_str(invalid_mac).is_err());
    }

    #[test]
    fn test_mac_address_from_str_invalid_hex() {
        let invalid_mac = "12:34:56:78:9a:zz"; // 無効な16進数
        assert!(MacAddress::from_str(invalid_mac).is_err());
    }

    #[test]
    fn test_mac_address_display() {
        let mac = MacAddress([0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]);
        assert_eq!(format!("{}", mac), "12:34:56:78:9a:bc");
    }

    #[test]
    fn test_format_mac_address() {
        let mac = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
        assert_eq!(format_mac_address(&mac), "12:34:56:78:9a:bc");
    }
}
