/// USBコマンド解析機能

use log::{debug, warn};

// ESP-NOWコマンド解析の定数
/// ESP-NOWコマンドの期待パーツ数
/// フォーマット: CMD_SEND_ESP_NOW:XX:XX:XX:XX:XX:XX:SLEEP_SECONDS
/// = 1(コマンド) + 6(MACアドレス) + 1(スリープ時間) = 8パーツ
const EXPECTED_ESP_NOW_PARTS: usize = 8;

/// 解析されたコマンド
#[derive(Debug, Clone)]
pub enum Command {
    /// ESP-NOW送信コマンド
    /// フォーマット: "CMD_SEND_ESP_NOW:MAC_ADDRESS:SLEEP_SECONDS"
    SendEspNow {
        /// 送信先MACアドレス
        mac_address: String,
        /// スリープ時間（秒）
        sleep_seconds: u32,
    },
    /// 不明なコマンド
    Unknown(String),
}

/// コマンド解析エラー
#[derive(Debug)]
pub enum CommandParseError {
    /// 無効なフォーマット
    InvalidFormat,
    /// 無効なスリープ時間
    InvalidSleepTime,
    /// 無効なMACアドレス
    InvalidMacAddress,
}

/// コマンド文字列を解析します
/// 
/// # 引数
/// * `command_str` - 解析するコマンド文字列
/// 
/// # 戻り値
/// * `Result<Command, CommandParseError>` - 解析されたコマンドまたはエラー
pub fn parse_command(command_str: &str) -> Result<Command, CommandParseError> {
    debug!("Parsing command: '{}'", command_str);
    
    let trimmed = command_str.trim();
    
    if trimmed.starts_with("CMD_SEND_ESP_NOW:") {
        parse_esp_now_command(trimmed)
    } else {
        warn!("Unknown command format: '{}'", trimmed);
        Ok(Command::Unknown(trimmed.to_string()))
    }
}

/// ESP-NOW送信コマンドを解析します
/// 
/// フォーマット: "CMD_SEND_ESP_NOW:MAC_ADDRESS:SLEEP_SECONDS"
/// 例: "CMD_SEND_ESP_NOW:34:ab:95:fb:3f:c4:60"
/// 
/// # 引数
/// * `command_str` - ESP-NOWコマンド文字列
/// 
/// # 戻り値
/// * `Result<Command, CommandParseError>` - 解析されたコマンドまたはエラー
fn parse_esp_now_command(command_str: &str) -> Result<Command, CommandParseError> {
    let parts: Vec<&str> = command_str.split(':').collect();
    
    // フォーマット: CMD_SEND_ESP_NOW:XX:XX:XX:XX:XX:XX:SLEEP_SECONDS
    // 最低8パーツ必要 (CMD_SEND_ESP_NOW + 6つのMACアドレス + SLEEP_SECONDS)
    if parts.len() != EXPECTED_ESP_NOW_PARTS {
        warn!("Invalid ESP-NOW command format. Expected {} parts, got {}: '{}'", 
              EXPECTED_ESP_NOW_PARTS, parts.len(), command_str);
        return Err(CommandParseError::InvalidFormat);
    }
    
    // CMD_SEND_ESP_NOWの確認
    if parts[0] != "CMD_SEND_ESP_NOW" {
        return Err(CommandParseError::InvalidFormat);
    }
    
    // MACアドレスを再構築 (parts[1]～parts[6])
    let mac_address = format!("{}:{}:{}:{}:{}:{}", 
                             parts[1], parts[2], parts[3], 
                             parts[4], parts[5], parts[6]);
    
    // MACアドレスの妥当性をチェック
    if !is_valid_mac_address(&mac_address) {
        warn!("Invalid MAC address format: '{}'", mac_address);
        return Err(CommandParseError::InvalidMacAddress);
    }
    
    // スリープ時間を解析 (parts[7])
    let sleep_seconds = parts[7]
        .parse::<u32>()
        .map_err(|_| {
            warn!("Invalid sleep time: '{}'", parts[7]);
            CommandParseError::InvalidSleepTime
        })?;
    
    // スリープ時間の妥当性をチェック (1秒～24時間)
    if sleep_seconds == 0 || sleep_seconds > 86400 {
        warn!("Sleep time out of range (1-86400): {}", sleep_seconds);
        return Err(CommandParseError::InvalidSleepTime);
    }
    
    debug!("Parsed ESP-NOW command: MAC={}, Sleep={}s", mac_address, sleep_seconds);
    
    Ok(Command::SendEspNow {
        mac_address,
        sleep_seconds,
    })
}

/// MACアドレスの妥当性をチェックします
/// 
/// # 引数
/// * `mac_str` - チェックするMACアドレス文字列
/// 
/// # 戻り値
/// * `bool` - 妥当な場合はtrue
fn is_valid_mac_address(mac_str: &str) -> bool {
    let parts: Vec<&str> = mac_str.split(':').collect();
    
    if parts.len() != 6 {
        return false;
    }
    
    for part in parts {
        if part.len() != 2 {
            return false;
        }
        
        if u8::from_str_radix(part, 16).is_err() {
            return false;
        }
    }
    
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_esp_now_command() {
        let command = "CMD_SEND_ESP_NOW:34:ab:95:fb:3f:c4:60";
        let result = parse_command(command).unwrap();
        
        match result {
            Command::SendEspNow { mac_address, sleep_seconds } => {
                assert_eq!(mac_address, "34:ab:95:fb:3f:c4");
                assert_eq!(sleep_seconds, 60);
            }
            _ => panic!("Expected SendEspNow command"),
        }
    }

    #[test]
    fn test_invalid_mac_address() {
        let command = "CMD_SEND_ESP_NOW:invalid:mac:60";
        let result = parse_command(command);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_sleep_time() {
        let command = "CMD_SEND_ESP_NOW:34:ab:95:fb:3f:c4:0";
        let result = parse_command(command);
        assert!(result.is_err());
    }
}
