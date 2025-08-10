use std::fmt;

/// MACアドレスを表す構造体
#[derive(Debug, Clone)]
pub struct MacAddress(pub(crate) [u8; 6]);

impl MacAddress {
    /// 文字列形式のMACアドレスから構造体を生成します
    ///
    /// # 引数
    ///
    /// * `s` - "xx:xx:xx:xx:xx:xx"形式のMACアドレス文字列
    ///
    /// # エラー
    ///
    /// 文字列のフォーマットが不正な場合や16進数として解析できない場合にエラーを返します
    pub fn from_str(s: &str) -> Result<Self, Box<dyn std::error::Error>> {
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
                .map_err(|e| format!("Failed to parse '{}' as hex byte: {}", part, e))?;
        }

        Ok(MacAddress(mac))
    }

    /// バイト配列からMACアドレスを作成
    pub fn new(addr: [u8; 6]) -> Self {
        MacAddress(addr)
    }
}

impl fmt::Display for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mac_address_from_str() {
        let mac = MacAddress::from_str("11:22:33:44:55:66").unwrap();
        assert_eq!(mac.0, [0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
    }

    #[test]
    fn test_mac_address_display() {
        let mac = MacAddress([0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        assert_eq!(format!("{}", mac), "11:22:33:44:55:66");
    }
}
