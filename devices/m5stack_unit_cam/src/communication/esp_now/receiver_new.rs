use esp_idf_svc::hal::delay::FreeRtos;
use log::{error, info, warn};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// 受信したスリープコマンドのデータ
static RECEIVED_SLEEP_DURATION: AtomicU32 = AtomicU32::new(0);
static SLEEP_COMMAND_RECEIVED: AtomicBool = AtomicBool::new(false);

/// ESP-NOW受信者（シンプル実装）
pub struct EspNowReceiver {
    /// プレースホルダー - 実際のESP-NOW受信はコールバックで処理
    _placeholder: (),
}

impl EspNowReceiver {
    /// 新しいESP-NOW受信者を作成
    pub fn new(_esp_now: Arc<Mutex<esp_idf_svc::espnow::EspNow<'static>>>) -> Result<Self, esp_idf_sys::EspError> {
        // ESP-NOW受信コールバックを設定
        unsafe {
            esp_idf_sys::esp_now_register_recv_cb(Some(esp_now_recv_cb));
        }

        Ok(Self {
            _placeholder: (),
        })
    }

    /// スリープコマンドを待機（タイムアウト付き）
    pub fn wait_for_sleep_command(&self, timeout_seconds: u32) -> Option<u32> {
        info!("スリープコマンドを{}秒間待機中...", timeout_seconds);
        
        let timeout_ms = timeout_seconds * 1000;
        let check_interval_ms = 100;
        let mut elapsed_ms = 0;

        // 受信フラグをリセット
        SLEEP_COMMAND_RECEIVED.store(false, Ordering::SeqCst);

        while elapsed_ms < timeout_ms {
            // 受信データをチェック
            if SLEEP_COMMAND_RECEIVED.load(Ordering::SeqCst) {
                let sleep_duration = RECEIVED_SLEEP_DURATION.load(Ordering::SeqCst);
                if sleep_duration > 0 && sleep_duration <= 86400 { // 最大24時間
                    info!("スリープコマンドを受信しました: {}秒", sleep_duration);
                    return Some(sleep_duration);
                }
            }

            FreeRtos::delay_ms(check_interval_ms);
            elapsed_ms += check_interval_ms;
        }

        warn!("スリープコマンドのタイムアウト（{}秒）", timeout_seconds);
        None
    }
}

/// ESP-NOW受信コールバック
extern "C" fn esp_now_recv_cb(
    _recv_info: *const esp_idf_sys::esp_now_recv_info_t,
    data: *const u8,
    data_len: i32,
) {
    if data_len <= 0 {
        return;
    }

    unsafe {
        let data_slice = std::slice::from_raw_parts(data, data_len as usize);
        
        info!("ESP-NOW受信: データサイズ={}", data_len);
        
        // バイナリ形式の場合（4バイトのu32）
        if data_len == 4 {
            let sleep_seconds = u32::from_le_bytes([data_slice[0], data_slice[1], data_slice[2], data_slice[3]]);
            if sleep_seconds > 0 && sleep_seconds <= 86400 {
                info!("バイナリスリープコマンド受信: {}秒", sleep_seconds);
                RECEIVED_SLEEP_DURATION.store(sleep_seconds, Ordering::SeqCst);
                SLEEP_COMMAND_RECEIVED.store(true, Ordering::SeqCst);
                return;
            }
        }

        // 文字列形式の場合
        if let Ok(command_str) = std::str::from_utf8(data_slice) {
            info!("受信コマンド文字列: '{}'", command_str);
            
            // 数値のみの場合（秒数）
            if let Ok(sleep_seconds) = command_str.trim().parse::<u32>() {
                if sleep_seconds > 0 && sleep_seconds <= 86400 { // 最大24時間
                    info!("文字列スリープコマンド受信: {}秒", sleep_seconds);
                    RECEIVED_SLEEP_DURATION.store(sleep_seconds, Ordering::SeqCst);
                    SLEEP_COMMAND_RECEIVED.store(true, Ordering::SeqCst);
                    return;
                }
            }
        }

        warn!("無効なスリープコマンド形式: {:?}", data_slice);
    }
}
