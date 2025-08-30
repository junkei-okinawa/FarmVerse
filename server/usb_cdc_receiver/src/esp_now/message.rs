/// ESP-NOW メッセージ定義
/// 
/// 双方向通信のためのメッセージタイプとプロトコル定義

use log::{debug, warn};

/// メッセージタイプ
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageType {
    /// データフレーム（カメラ画像など）
    DataFrame = 0x01,
    /// ACK確認応答
    Ack = 0x02,
    /// スリープコマンド
    SleepCommand = 0x03,
    /// ハートビート
    Heartbeat = 0x04,
}

impl MessageType {
    /// u8からMessageTypeに変換
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(MessageType::DataFrame),
            0x02 => Some(MessageType::Ack),
            0x03 => Some(MessageType::SleepCommand),
            0x04 => Some(MessageType::Heartbeat),
            _ => None,
        }
    }
    
    /// MessageTypeをu8に変換
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

/// ACKメッセージの構造
#[derive(Debug, Clone)]
pub struct AckMessage {
    /// ACK対象のシーケンス番号
    pub sequence_number: u16,
    /// ACK対象のメッセージタイプ
    pub acked_message_type: MessageType,
    /// 受信ステータス
    pub status: AckStatus,
}

/// ACK応答ステータス
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AckStatus {
    /// 正常受信
    Success = 0x00,
    /// チェックサムエラー
    ChecksumError = 0x01,
    /// バッファ溢れ
    BufferOverflow = 0x02,
    /// 不正なフォーマット
    InvalidFormat = 0x03,
}

impl AckStatus {
    /// u8からAckStatusに変換
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x00 => Some(AckStatus::Success),
            0x01 => Some(AckStatus::ChecksumError),
            0x02 => Some(AckStatus::BufferOverflow),
            0x03 => Some(AckStatus::InvalidFormat),
            _ => None,
        }
    }
    
    /// AckStatusをu8に変換
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

impl AckMessage {
    /// 新しいACKメッセージを作成
    pub fn new(sequence_number: u16, acked_message_type: MessageType, status: AckStatus) -> Self {
        Self {
            sequence_number,
            acked_message_type,
            status,
        }
    }
    
    /// ACKメッセージを成功ACKとして作成
    pub fn success(sequence_number: u16, acked_message_type: MessageType) -> Self {
        Self::new(sequence_number, acked_message_type, AckStatus::Success)
    }
    
    /// ACKメッセージをバイナリ形式にシリアライズ
    /// 
    /// フォーマット:
    /// ```
    /// [MSG_TYPE(1)] [SEQ_NUM(2)] [ACKED_TYPE(1)] [STATUS(1)]
    /// ```
    pub fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(5);
        data.push(MessageType::Ack.to_u8());
        data.extend_from_slice(&self.sequence_number.to_le_bytes());
        data.push(self.acked_message_type.to_u8());
        data.push(self.status.to_u8());
        data
    }
    
    /// バイナリデータからACKメッセージをデシリアライズ
    pub fn deserialize(data: &[u8]) -> Option<Self> {
        if data.len() < 5 {
            warn!("ACK message too short: {} bytes", data.len());
            return None;
        }
        
        // メッセージタイプの確認
        if MessageType::from_u8(data[0])? != MessageType::Ack {
            warn!("Invalid ACK message type: {}", data[0]);
            return None;
        }
        
        let sequence_number = u16::from_le_bytes([data[1], data[2]]);
        let acked_message_type = MessageType::from_u8(data[3])?;
        let status = AckStatus::from_u8(data[4])?;
        
        debug!("Deserialized ACK: seq={}, type={:?}, status={:?}", 
               sequence_number, acked_message_type, status);
        
        Some(Self::new(sequence_number, acked_message_type, status))
    }
}

/// スリープコマンドメッセージ
#[derive(Debug, Clone)]
pub struct SleepCommandMessage {
    /// スリープ時間（秒）
    pub sleep_seconds: u32,
}

impl SleepCommandMessage {
    /// 新しいスリープコマンドを作成
    pub fn new(sleep_seconds: u32) -> Self {
        Self { sleep_seconds }
    }
    
    /// スリープコマンドをバイナリ形式にシリアライズ
    /// 
    /// フォーマット:
    /// ```
    /// [MSG_TYPE(1)] [SLEEP_SECONDS(4)]
    /// ```
    pub fn serialize(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(5);
        data.push(MessageType::SleepCommand.to_u8());
        data.extend_from_slice(&self.sleep_seconds.to_le_bytes());
        data
    }
    
    /// バイナリデータからスリープコマンドをデシリアライズ
    pub fn deserialize(data: &[u8]) -> Option<Self> {
        if data.len() < 5 {
            warn!("Sleep command too short: {} bytes", data.len());
            return None;
        }
        
        // メッセージタイプの確認
        if MessageType::from_u8(data[0])? != MessageType::SleepCommand {
            warn!("Invalid sleep command message type: {}", data[0]);
            return None;
        }
        
        let sleep_seconds = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
        
        debug!("Deserialized sleep command: {} seconds", sleep_seconds);
        
        Some(Self::new(sleep_seconds))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ack_message_serialization() {
        let ack = AckMessage::success(12345, MessageType::DataFrame);
        let data = ack.serialize();
        
        assert_eq!(data.len(), 5);
        assert_eq!(data[0], MessageType::Ack.to_u8());
        
        let deserialized = AckMessage::deserialize(&data).unwrap();
        assert_eq!(deserialized.sequence_number, 12345);
        assert_eq!(deserialized.acked_message_type, MessageType::DataFrame);
        assert_eq!(deserialized.status, AckStatus::Success);
    }
    
    #[test]
    fn test_sleep_command_serialization() {
        let sleep_cmd = SleepCommandMessage::new(3600);
        let data = sleep_cmd.serialize();
        
        assert_eq!(data.len(), 5);
        assert_eq!(data[0], MessageType::SleepCommand.to_u8());
        
        let deserialized = SleepCommandMessage::deserialize(&data).unwrap();
        assert_eq!(deserialized.sleep_seconds, 3600);
    }
}
