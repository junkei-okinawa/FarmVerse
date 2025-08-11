use esp_idf_svc::sys::esp_now_send;
use log::{error, info, warn};

/// ESP-NOW送信エラー
#[derive(Debug)]
pub enum EspNowSendError {
    /// ピア追加失敗
    AddPeerFailed(i32),
    /// 送信失敗
    SendFailed(i32),
    /// 無効なMACアドレス
    InvalidMacAddress,
}

/// ESP-NOW送信機能
pub struct EspNowSender {
    // 設定フィールドは特に不要 - ESP-NOWピアは main.rs で登録済み
}

impl EspNowSender {
    /// 新しいESP-NOW送信インスタンスを作成
    pub fn new() -> Self {
        Self {}
    }

    /// MACアドレス文字列を[u8; 6]配列に変換
    /// 
    /// # 引数
    /// * `mac_str` - "XX:XX:XX:XX:XX:XX" 形式のMACアドレス文字列
    /// 
    /// # 戻り値
    /// * `Result<[u8; 6], EspNowSendError>` - 変換されたMACアドレス配列
    pub fn parse_mac_address(mac_str: &str) -> Result<[u8; 6], EspNowSendError> {
        let parts: Vec<&str> = mac_str.split(':').collect();
        if parts.len() != 6 {
            return Err(EspNowSendError::InvalidMacAddress);
        }

        let mut mac = [0u8; 6];
        for (i, part) in parts.iter().enumerate() {
            mac[i] = u8::from_str_radix(part, 16)
                .map_err(|_| EspNowSendError::InvalidMacAddress)?;
        }

        Ok(mac)
    }

    /// ESP-NOWでデータを送信
    /// 
    /// # 引数
    /// * `mac_address` - 送信先のMACアドレス
    /// * `data` - 送信するデータ
    /// 
    /// # 戻り値
    /// * `Result<(), EspNowSendError>` - 成功時はOk(())、失敗時はエラー
    pub fn send_data(&self, mac_address: [u8; 6], data: &[u8]) -> Result<(), EspNowSendError> {
        use esp_idf_svc::hal::delay::FreeRtos;
        
        // ピアは register_esp_now_peers() で登録済みなので、直接送信
        info!("ESP-NOW low-level send: MAC={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}, {} bytes",
              mac_address[0], mac_address[1], mac_address[2],
              mac_address[3], mac_address[4], mac_address[5], data.len());
        
        // ESP-NOWの送信前に遅延を追加（チャンネル競合防止）
        FreeRtos::delay_ms(300);
        
        let result = unsafe {
            esp_now_send(
                mac_address.as_ptr(),
                data.as_ptr(),
                data.len(),
            )
        };

        if result == 0 {
            info!("✓ ESP-NOW raw send successful: error code {}", result);
            Ok(())
        } else {
            error!("✗ ESP-NOW raw send failed: error code {}", result);
            Err(EspNowSendError::SendFailed(result))
        }
    }

    /// スリープコマンドを送信（リトライ機構付き）
    /// 
    /// # 引数
    /// * `mac_str` - 送信先のMACアドレス文字列 ("XX:XX:XX:XX:XX:XX")
    /// * `sleep_seconds` - スリープ時間（秒）
    /// 
    /// # 戻り値
    /// * `Result<(), EspNowSendError>` - 成功時はOk(())、失敗時はエラー
    pub fn send_sleep_command(&self, mac_str: &str, sleep_seconds: u32) -> Result<(), EspNowSendError> {
        use esp_idf_svc::hal::delay::FreeRtos;
        
        info!("=== ESP-NOW Sleep Command Sending ===");
        info!("Target MAC: {}", mac_str);
        info!("Sleep Duration: {} seconds", sleep_seconds);
        
        let mac_address = Self::parse_mac_address(mac_str)?;
        info!("Parsed MAC address: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
              mac_address[0], mac_address[1], mac_address[2],
              mac_address[3], mac_address[4], mac_address[5]);
        
        // バイナリ形式でスリープ時間を送信（4バイトのu32）
        let sleep_data = sleep_seconds.to_le_bytes();
        info!("Sleep data bytes: {:02X} {:02X} {:02X} {:02X}",
              sleep_data[0], sleep_data[1], sleep_data[2], sleep_data[3]);
        
        // リトライ機構付きで送信
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY_MS: u32 = 200;
        
        for attempt in 1..=MAX_RETRIES {
            info!("Attempting ESP-NOW send (attempt {}/{})", attempt, MAX_RETRIES);
            
            let result = self.send_data(mac_address, &sleep_data);
            
            match &result {
                Ok(()) => {
                    info!("✓ Sleep command sent successfully via ESP-NOW (attempt {})", attempt);
                    return Ok(());
                }
                Err(e) => {
                    warn!("✗ ESP-NOW send attempt {} failed: {:?}", attempt, e);
                    if attempt < MAX_RETRIES {
                        info!("Waiting {}ms before retry...", RETRY_DELAY_MS);
                        FreeRtos::delay_ms(RETRY_DELAY_MS);
                    }
                }
            }
        }
        
        error!("✗ All {} ESP-NOW send attempts failed for {}", MAX_RETRIES, mac_str);
        Err(EspNowSendError::SendFailed(-1)) // All retries failed
    }
}
