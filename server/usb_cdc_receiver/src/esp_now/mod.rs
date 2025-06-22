pub mod frame;
pub mod receiver;
pub mod sender;

/// ESP-NOWフレームタイプを定義する列挙型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    /// ハッシュ値を含むフレーム（画像の整合性確認用）
    Hash = 1,
    /// 画像データを含むフレーム
    Data = 2,
    /// 転送終了を示すフレーム
    Eof = 3,
}

impl FrameType {
    /// バイト値からフレームタイプを取得
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(FrameType::Hash),
            2 => Some(FrameType::Data),
            3 => Some(FrameType::Eof),
            _ => None,
        }
    }

    /// フレームタイプをバイト値に変換
    pub fn to_byte(self) -> u8 {
        self as u8
    }

    /// フレームタイプのわかりやすい文字列表現を取得
    pub fn as_str(&self) -> &'static str {
        match self {
            FrameType::Hash => "HASH",
            FrameType::Data => "DATA",
            FrameType::Eof => "EOF",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_type_conversion() {
        assert_eq!(FrameType::Hash.to_byte(), 1);
        assert_eq!(FrameType::Data.to_byte(), 2);
        assert_eq!(FrameType::Eof.to_byte(), 3);

        assert_eq!(FrameType::from_byte(1), Some(FrameType::Hash));
        assert_eq!(FrameType::from_byte(2), Some(FrameType::Data));
        assert_eq!(FrameType::from_byte(3), Some(FrameType::Eof));
        assert_eq!(FrameType::from_byte(4), None);
    }

    #[test]
    fn test_frame_type_as_str() {
        assert_eq!(FrameType::Hash.as_str(), "HASH");
        assert_eq!(FrameType::Data.as_str(), "DATA");
        assert_eq!(FrameType::Eof.as_str(), "EOF");
    }
}
