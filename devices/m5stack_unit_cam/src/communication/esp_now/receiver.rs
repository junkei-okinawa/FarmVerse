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
        info!("=== スリープコマンド待機開始 ===");
        info!("タイムアウト: {}秒", timeout_seconds);
        
        let timeout_ms = timeout_seconds * 1000;
        let check_interval_ms = 100;
        let mut elapsed_ms = 0;

        // 受信フラグをリセット
        SLEEP_COMMAND_RECEIVED.store(false, Ordering::SeqCst);
        RECEIVED_SLEEP_DURATION.store(0, Ordering::SeqCst);
        
        info!("受信フラグをリセットしました");

        while elapsed_ms < timeout_ms {
            // 受信データをチェック
            if SLEEP_COMMAND_RECEIVED.load(Ordering::SeqCst) {
                let sleep_duration = RECEIVED_SLEEP_DURATION.load(Ordering::SeqCst);
                info!("受信データ検出: sleep_duration={}", sleep_duration);
                if sleep_duration > 0 && sleep_duration <= 86400 { // 最大24時間
                    info!("✓ 有効なスリープコマンドを受信: {}秒", sleep_duration);
                    return Some(sleep_duration);
                } else {
                    warn!("無効なスリープ時間: {}", sleep_duration);
                }
            }

            if elapsed_ms % 500 == 0 { // 0.5秒毎に進捗をログ出力
                info!("待機中... {}/{}秒", elapsed_ms / 1000, timeout_seconds);
            }

            FreeRtos::delay_ms(check_interval_ms);
            elapsed_ms += check_interval_ms;
        }

        warn!("✗ スリープコマンドのタイムアウト（{}秒）", timeout_seconds);
        None
    }
}

/// ESP-NOW受信コールバック
extern "C" fn esp_now_recv_cb(
    recv_info: *const esp_idf_sys::esp_now_recv_info_t,
    data: *const u8,
    data_len: i32,
) {
    info!("=== ESP-NOW受信コールバック ===");
    
    if data_len <= 0 {
        warn!("ESP-NOW受信: データ長が無効 ({})", data_len);
        return;
    }

    unsafe {
        let data_slice = std::slice::from_raw_parts(data, data_len as usize);
        
        // 送信者MACアドレスを取得（安全な方法）
        let sender_mac = if !recv_info.is_null() {
            let recv_info_ref = &*recv_info;
            let src_addr_slice = std::slice::from_raw_parts(recv_info_ref.src_addr, 6);
            format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                src_addr_slice[0], src_addr_slice[1], src_addr_slice[2],
                src_addr_slice[3], src_addr_slice[4], src_addr_slice[5])
        } else {
            "UNKNOWN".to_string()
        };
        
        info!("送信者MAC: {}", sender_mac);
        info!("データサイズ: {}", data_len);
        info!("データ内容: {:02X?}", data_slice);
        
        // バイナリ形式の場合（4バイトのu32）
        if data_len == 4 {
            let sleep_seconds = u32::from_le_bytes([data_slice[0], data_slice[1], data_slice[2], data_slice[3]]);
            info!("バイナリ形式でのスリープ時間: {}秒", sleep_seconds);
            if sleep_seconds > 0 && sleep_seconds <= 86400 {
                info!("✓ 有効なバイナリスリープコマンド受信: {}秒", sleep_seconds);
                RECEIVED_SLEEP_DURATION.store(sleep_seconds, Ordering::SeqCst);
                SLEEP_COMMAND_RECEIVED.store(true, Ordering::SeqCst);
                return;
            } else {
                warn!("無効なバイナリスリープ時間: {}", sleep_seconds);
            }
        }

        // 文字列形式の場合
        if let Ok(command_str) = std::str::from_utf8(data_slice) {
            info!("文字列形式でのコマンド: '{}'", command_str);
            
            // 数値のみの場合（秒数）
            if let Ok(sleep_seconds) = command_str.trim().parse::<u32>() {
                info!("文字列形式でのスリープ時間: {}秒", sleep_seconds);
                if sleep_seconds > 0 && sleep_seconds <= 86400 { // 最大24時間
                    info!("✓ 有効な文字列スリープコマンド受信: {}秒", sleep_seconds);
                    RECEIVED_SLEEP_DURATION.store(sleep_seconds, Ordering::SeqCst);
                    SLEEP_COMMAND_RECEIVED.store(true, Ordering::SeqCst);
                    return;
                } else {
                    warn!("無効な文字列スリープ時間: {}", sleep_seconds);
                }
            }
        }

        warn!("✗ 無効なスリープコマンド形式: {:02X?}", data_slice);
    }
}
