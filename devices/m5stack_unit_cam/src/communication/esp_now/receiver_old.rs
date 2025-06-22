use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::espnow::EspNow;
use esp_idf_sys::*;
use log::{error, info, warn};
use std::sync::{Arc, Mutex};

/// ESP-NOW受信者
pub struct EspNowReceiver {
    esp_now: Arc<Mutex<EspNow<'static>>>,
    last_received_data: Arc<Mutex<Option<Vec<u8>>>>,
}

impl EspNowReceiver {
    /// 新しいESP-NOW受信者を作成
    pub fn new(esp_now: Arc<Mutex<EspNow<'static>>>) -> Result<Self, EspError> {
        let last_received_data = Arc::new(Mutex::new(None));
        let receiver_data = Arc::clone(&last_received_data);

        // 受信コールバックを設定
        {
            let mut esp_now_guard = esp_now.lock().unwrap();
            esp_now_guard.register_recv_cb(move |info: &RecvInfo, data: &[u8]| {
                info!(
                    "ESP-NOW受信: MAC={:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}, データサイズ={}, RSSI={}",
                    info.src_addr[0], info.src_addr[1], info.src_addr[2],
                    info.src_addr[3], info.src_addr[4], info.src_addr[5],
                    data.len(),
                    info.rx_ctrl.map(|ctrl| ctrl.rssi).unwrap_or(0)
                );

                // データをコピーして保存
                if let Ok(mut guard) = receiver_data.lock() {
                    *guard = Some(data.to_vec());
                    info!("ESP-NOW受信データを保存しました: {:?}", data);
                } else {
                    error!("ESP-NOW受信データの保存に失敗しました");
                }
            })?;
        }

        Ok(Self {
            esp_now,
            last_received_data,
        })
    }

    /// スリープコマンドを待機（タイムアウト付き）
    pub fn wait_for_sleep_command(&self, timeout_seconds: u32) -> Option<u32> {
        info!("スリープコマンドを{}秒間待機中...", timeout_seconds);
        
        let timeout_ms = timeout_seconds * 1000;
        let check_interval_ms = 100;
        let mut elapsed_ms = 0;

        while elapsed_ms < timeout_ms {
            // 受信データをチェック
            if let Ok(guard) = self.last_received_data.lock() {
                if let Some(ref data) = *guard {
                    if let Some(sleep_duration) = self.parse_sleep_command(data) {
                        info!("スリープコマンドを受信しました: {}秒", sleep_duration);
                        return Some(sleep_duration);
                    }
                }
            }

            FreeRtos::delay_ms(check_interval_ms);
            elapsed_ms += check_interval_ms;
        }

        warn!("スリープコマンドのタイムアウト（{}秒）", timeout_seconds);
        None
    }

    /// 受信データからスリープコマンドを解析
    fn parse_sleep_command(&self, data: &[u8]) -> Option<u32> {
        // データが数値（スリープ時間）として解析できるかチェック
        if data.len() == 0 {
            return None;
        }

        // バイナリデータを文字列として解釈を試行
        if let Ok(command_str) = std::str::from_utf8(data) {
            info!("受信コマンド文字列: '{}'", command_str);
            
            // 数値のみの場合（秒数）
            if let Ok(sleep_seconds) = command_str.trim().parse::<u32>() {
                if sleep_seconds > 0 && sleep_seconds <= 86400 { // 最大24時間
                    return Some(sleep_seconds);
                }
            }
        }

        // バイナリ形式の場合（4バイトのu32）
        if data.len() == 4 {
            let sleep_seconds = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
            if sleep_seconds > 0 && sleep_seconds <= 86400 {
                info!("バイナリスリープコマンド: {}秒", sleep_seconds);
                return Some(sleep_seconds);
            }
        }

        warn!("無効なスリープコマンド形式: {:?}", data);
        None
    }

    /// 受信データをクリア
    pub fn clear_received_data(&self) {
        if let Ok(mut guard) = self.last_received_data.lock() {
            *guard = None;
        }
    }
}
