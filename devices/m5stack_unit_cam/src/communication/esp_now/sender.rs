use crate::mac_address::MacAddress;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_sys::{
    esp_now_add_peer, esp_now_init, esp_now_peer_info_t, esp_now_recv_info_t, esp_now_register_recv_cb, esp_now_register_send_cb, esp_now_send,
    esp_now_send_status_t, esp_now_send_status_t_ESP_NOW_SEND_SUCCESS,
    wifi_interface_t_WIFI_IF_STA,
};
use log::{error, info, warn};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// ESP-NOW送信結果
#[derive(Debug, Clone)]
#[allow(dead_code)] // This enum may be used in the future for more detailed send status
pub enum SendResult {
    /// 送信タイムアウト
    Timeout,
    /// ESP-IDFエラー
    EspError(esp_idf_sys::EspError),
}

/// ESP-NOW送信エラー
#[derive(Debug, thiserror::Error)]
pub enum EspNowError {
    #[error("ESP-NOW初期化エラー: {0}")]
    InitFailed(i32),

    #[error("ESP-NOWピア追加エラー: {0}")]
    AddPeerFailed(i32),

    #[error("ESP-NOW送信エラー: {0}")]
    SendFailed(i32),

    #[error("送信タイムアウトエラー")]
    SendTimeout,

    #[error("送信失敗（コールバックで報告）")]
    SendFailedCallback,
}

/// 送信状態を共有するためのグローバルチャネル
static SEND_COMPLETE: AtomicBool = AtomicBool::new(true);
static SEND_FAILED: AtomicBool = AtomicBool::new(false);

/// 受信したスリープコマンドのデータ
static RECEIVED_SLEEP_DURATION: AtomicU32 = AtomicU32::new(0);
static SLEEP_COMMAND_RECEIVED: AtomicBool = AtomicBool::new(false);

/// ESP-NOW送信コールバック
extern "C" fn esp_now_send_cb(_mac_addr: *const u8, status: esp_now_send_status_t) {
    if status == esp_now_send_status_t_ESP_NOW_SEND_SUCCESS {
        // 送信成功時の冗長ログは省略
    } else {
        error!("ESP-NOW: Send failed");
        SEND_FAILED.store(true, Ordering::SeqCst);
    }
    SEND_COMPLETE.store(true, Ordering::SeqCst);
}

/// ESP-NOW受信コールバック
extern "C" fn esp_now_recv_cb(
    _recv_info: *const esp_now_recv_info_t,
    data: *const u8,
    data_len: i32,
) {
    if data_len >= 4 {
        // 4バイトのスリープ時間（秒）を受信
        let sleep_bytes = unsafe { std::slice::from_raw_parts(data, 4) };
        let sleep_duration = u32::from_le_bytes([
            sleep_bytes[0],
            sleep_bytes[1],
            sleep_bytes[2],
            sleep_bytes[3],
        ]);
        
        RECEIVED_SLEEP_DURATION.store(sleep_duration, Ordering::SeqCst);
        SLEEP_COMMAND_RECEIVED.store(true, Ordering::SeqCst);
    }
}

/// ESP-NOW送信機
#[derive(Debug)]
pub struct EspNowSender {
    #[allow(dead_code)]
    initialized: bool,
}

impl EspNowSender {
    /// 新しいESP-NOW送信機を初期化します
    ///
    /// # エラー
    ///
    /// ESP-NOWの初期化に失敗した場合にエラーを返します
    pub fn new() -> Result<Self, EspNowError> {
        let result = unsafe { esp_now_init() };
        if result != 0 {
            return Err(EspNowError::InitFailed(result));
        }

        unsafe {
            esp_now_register_send_cb(Some(esp_now_send_cb));
            esp_now_register_recv_cb(Some(esp_now_recv_cb));
        }

        Ok(Self { initialized: true })
    }

    /// ピアを追加します
    ///
    /// # 引数
    ///
    /// * `peer_mac` - ピアのMACアドレス
    ///
    /// # エラー
    ///
    /// ピア追加に失敗した場合にエラーを返します
    pub fn add_peer(&self, peer_mac: &MacAddress) -> Result<(), EspNowError> {
        use log::info;
        
        info!("ESP-NOWピア追加: MAC={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", 
              peer_mac.0[0], peer_mac.0[1], peer_mac.0[2], 
              peer_mac.0[3], peer_mac.0[4], peer_mac.0[5]);

        let mut peer_info = esp_now_peer_info_t::default();
        peer_info.channel = 0;
        peer_info.ifidx = wifi_interface_t_WIFI_IF_STA;
        peer_info.encrypt = false;
        peer_info.peer_addr = peer_mac.0;

        let result = unsafe { esp_now_add_peer(&peer_info) };
        if result != 0 {
            error!("ESP-NOWピア追加失敗: esp_now_add_peer returned {}", result);
            match result {
                -1 => error!("ESP-NOWピア追加エラー: ESP_ERR_INVALID_ARG (引数が無効)"),
                -2 => error!("ESP-NOWピア追加エラー: ESP_ERR_INVALID_STATE (ESP-NOWが初期化されていない)"),
                -3 => error!("ESP-NOWピア追加エラー: ESP_ERR_NO_MEM (メモリ不足)"),
                -6 => error!("ESP-NOWピア追加エラー: ESP_ERR_ESPNOW_EXIST (ピアが既に存在)"),
                -7 => error!("ESP-NOWピア追加エラー: ESP_ERR_ESPNOW_FULL (ピアリストが満杯)"),
                _ => error!("ESP-NOWピア追加エラー: 未知のエラーコード {}", result),
            }
            return Err(EspNowError::AddPeerFailed(result));
        }

        info!("ESP-NOWピア追加成功");
        Ok(())
    }

    /// スリープコマンドを受信するまで待機する
    ///
    /// # 引数
    ///
    /// * `timeout_ms` - タイムアウト時間（ミリ秒）
    ///
    /// # 戻り値
    ///
    /// Some(duration) - 受信したスリープ時間（秒）
    /// None - タイムアウトまたは受信できなかった場合
    pub fn receive_sleep_command(&self, timeout_ms: u32) -> Option<u32> {
        // 受信状態をリセット
        SLEEP_COMMAND_RECEIVED.store(false, Ordering::SeqCst);
        RECEIVED_SLEEP_DURATION.store(0, Ordering::SeqCst);

        let mut elapsed_ms = 0;
        while elapsed_ms < timeout_ms {
            if SLEEP_COMMAND_RECEIVED.load(Ordering::SeqCst) {
                return Some(RECEIVED_SLEEP_DURATION.load(Ordering::SeqCst));
            }
            
            FreeRtos::delay_ms(10);
            elapsed_ms += 10;
        }

        None
    }

    /// メッセージを送信します
    ///
    /// # 引数
    ///
    /// * `peer_mac` - 送信先のMACアドレス
    /// * `data` - 送信するデータ
    /// * `timeout_ms` - 送信タイムアウト（ミリ秒）
    ///
    /// # エラー
    ///
    /// - 送信キューイングに失敗した場合
    /// - タイムアウトした場合
    /// - コールバックがエラーを報告した場合
    pub fn send(
        &self,
        peer_mac: &MacAddress,
        data: &[u8],
        timeout_ms: u32,
    ) -> Result<(), EspNowError> {
        use log::info;
        
        info!("ESP-NOW送信開始: データサイズ={}, 送信先={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", 
              data.len(),
              peer_mac.0[0], peer_mac.0[1], peer_mac.0[2], 
              peer_mac.0[3], peer_mac.0[4], peer_mac.0[5]);

        // 前回の送信が完了するまで待機
        let mut timeout_counter = 0;
        while !SEND_COMPLETE.load(Ordering::SeqCst) {
            FreeRtos::delay_ms(1);
            timeout_counter += 1;
            if timeout_counter > timeout_ms {
                error!("ESP-NOW送信: 前回送信の完了待機タイムアウト");
                return Err(EspNowError::SendTimeout);
            }
        }

        // 送信状態をリセット
        SEND_COMPLETE.store(false, Ordering::SeqCst);
        SEND_FAILED.store(false, Ordering::SeqCst);

        // データを送信
        let result = unsafe { esp_now_send(peer_mac.0.as_ptr(), data.as_ptr(), data.len()) };
        if result != 0 {
            error!("ESP-NOW送信失敗: esp_now_send returned {} (0x{:X})", result, result as u32);
            // エラーコードの詳細を表示
            match result {
                -1 => error!("ESP-NOW送信エラー: ESP_ERR_INVALID_ARG (引数が無効)"),
                -2 => error!("ESP-NOW送信エラー: ESP_ERR_INVALID_STATE (ESP-NOWが初期化されていない)"),
                -3 => error!("ESP-NOW送信エラー: ESP_ERR_NO_MEM (メモリ不足)"),
                -4 => error!("ESP-NOW送信エラー: ESP_ERR_NOT_FOUND (ピアが見つからない)"),
                -5 => error!("ESP-NOW送信エラー: ESP_ERR_INVALID_SIZE (データサイズが無効)"),
                12390 => error!("ESP-NOW送信エラー: ESP_ERR_ESPNOW_FULL (ESP-NOWキューが満杯) - WiFi接続が必要"),
                _ => {
                    error!("ESP-NOW送信エラー: 未知のエラーコード {} (0x{:X})", result, result as u32);
                    // ESP-IDFエラーコードをデコード
                    let err_base = (result as u32 >> 12) & 0xFF;
                    let err_code = result as u32 & 0xFFF;
                    error!("エラーベース: 0x{:X}, エラーコード: 0x{:X}", err_base, err_code);
                }
            }
            SEND_COMPLETE.store(true, Ordering::SeqCst);
            return Err(EspNowError::SendFailed(result));
        }

        info!("ESP-NOW送信コマンド実行成功、送信完了コールバック待機中...");

        // 送信完了を待機
        timeout_counter = 0;
        while !SEND_COMPLETE.load(Ordering::SeqCst) {
            FreeRtos::delay_ms(1);
            timeout_counter += 1;
            if timeout_counter > timeout_ms {
                error!("ESP-NOW送信: 送信完了コールバックタイムアウト ({}ms)", timeout_ms);
                return Err(EspNowError::SendTimeout);
            }
        }

        // 送信結果を確認
        if SEND_FAILED.load(Ordering::SeqCst) {
            error!("ESP-NOW送信: コールバックで送信失敗が報告された");
            return Err(EspNowError::SendFailedCallback);
        }

        info!("ESP-NOW送信成功");
        Ok(())
    }

    /// リトライ機能付きのデータ送信
    pub fn send_with_retry(
        &self,
        peer_mac: &MacAddress,
        data: &[u8],
        timeout_ms: u32,
        max_retries: u8,
    ) -> Result<(), EspNowError> {
        let mut last_error = EspNowError::SendTimeout;
        
        for attempt in 1..=max_retries {
            info!("ESP-NOW送信試行 {}/{}", attempt, max_retries);
            
            match self.send(peer_mac, data, timeout_ms) {
                Ok(()) => {
                    info!("ESP-NOW送信成功 (試行 {})", attempt);
                    return Ok(());
                }
                Err(e) => {
                    error!("ESP-NOW送信失敗 (試行 {}): {:?}", attempt, e);
                    last_error = e;
                    
                    // エラーが12390（キュー満杯）の場合、2回目の試行前にキューリセットを試行
                    if let EspNowError::SendFailed(12390) = last_error {
                        if attempt == 1 {
                            warn!("ESP-NOWキューが満杯です。キューリセットを試行します...");
                            if let Err(reset_err) = self.reset_espnow_queue() {
                                error!("キューリセット失敗: {:?}", reset_err);
                            } else {
                                // ピアを再追加
                                if let Err(peer_err) = self.add_peer(peer_mac) {
                                    error!("ピア再追加失敗: {:?}", peer_err);
                                }
                            }
                        }
                    }
                    
                    if attempt < max_retries {
                        // リトライ前に少し待機
                        FreeRtos::delay_ms(200 * attempt as u32);
                    }
                }
            }
        }
        
        error!("ESP-NOW送信: 全ての試行が失敗しました");
        Err(last_error)
    }

    /// 画像データをチャンクに分割して送信する
    ///
    /// # 引数
    ///
    /// * `peer_mac` - 送信先のMACアドレス
    /// * `data` - 送信する画像データ
    /// * `chunk_size` - チャンクサイズ（バイト数）
    /// * `delay_between_chunks_ms` - チャンク間のディレイ（ミリ秒）
    ///
    /// # エラー
    ///
    /// - 送信に失敗した場合にエラーを返します
    pub fn send_image_chunks(
        &self,
        peer_mac: &MacAddress,
        data: Vec<u8>,
        chunk_size: usize,
        delay_between_chunks_ms: u32,
    ) -> Result<(), EspNowError> {
        for chunk in data.chunks(chunk_size) {
            // リトライ機能付きで送信
            self.send_with_retry(peer_mac, chunk, 1000, 3)?;

            // チャンク間にディレイを挿入
            if delay_between_chunks_ms > 0 {
                FreeRtos::delay_ms(delay_between_chunks_ms);
            }
        }

        // EOFマーカー送信
        FreeRtos::delay_ms(15); // EOFマーカー送信前に少し待機
        self.send_with_retry(peer_mac, b"EOF!", 1000, 3)?;

        Ok(())
    }

    /// WiFi状態を確認してESP-NOW送信の準備ができているかチェック
    pub fn check_wifi_status(&self) -> Result<(), EspNowError> {
        use esp_idf_sys::{esp_wifi_get_mode, wifi_mode_t};
        
        let mut mode: wifi_mode_t = 0;
        let result = unsafe { esp_wifi_get_mode(&mut mode) };
        if result != 0 {
            error!("WiFiモード取得に失敗: {}", result);
            return Err(EspNowError::InitFailed(result));
        }
        
        info!("現在のWiFiモード: {}", mode);
        if mode == 0 {  // WIFI_MODE_NULL
            error!("WiFiが無効化されています - ESP-NOW送信には WiFi STA または AP モードが必要");
            return Err(EspNowError::InitFailed(-1));
        }
        
        Ok(())
    }

    /// ESP-NOWピアの状態を確認
    pub fn check_peer_status(&self, peer_mac: &MacAddress) -> Result<(), EspNowError> {
        use esp_idf_sys::{esp_now_get_peer, esp_now_peer_info_t};
        
        let mut peer_info = esp_now_peer_info_t::default();
        let result = unsafe { 
            esp_now_get_peer(peer_mac.0.as_ptr(), &mut peer_info) 
        };
        
        if result != 0 {
            error!("ピア情報取得失敗: {} (0x{:X})", result, result as u32);
            match result {
                -4 => error!("ピアが見つかりません - MACアドレス {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X} は登録されていない可能性", 
                            peer_mac.0[0], peer_mac.0[1], peer_mac.0[2], 
                            peer_mac.0[3], peer_mac.0[4], peer_mac.0[5]),
                _ => error!("不明なピア確認エラー"),
            }
            return Err(EspNowError::AddPeerFailed(result));
        }
        
        info!("ピア情報確認成功: チャンネル={}, 暗号化={}", 
              peer_info.channel, peer_info.encrypt);
        Ok(())
    }

    /// ESP-NOWの統計情報を表示
    pub fn print_espnow_stats(&self) {
        info!("=== ESP-NOW診断情報 ===");
        
        // WiFiチャンネル情報を取得
        unsafe {
            use esp_idf_sys::{esp_wifi_get_channel, wifi_second_chan_t};
            let mut primary = 0u8;
            let mut second: wifi_second_chan_t = 0;
            let result = esp_wifi_get_channel(&mut primary, &mut second);
            if result == 0 {
                info!("WiFiチャンネル: プライマリ={}, セカンダリ={}", primary, second);
            } else {
                error!("WiFiチャンネル取得失敗: {}", result);
            }
        }
        
        info!("デバイス送信準備完了 - 受信機が同じチャンネルで待機している必要があります");
        info!("======================");
    }

    /// ESP-NOWキューをクリアしてリセット（診断用）
    pub fn reset_espnow_queue(&self) -> Result<(), EspNowError> {
        use esp_idf_sys::{esp_now_deinit, esp_now_init};
        
        info!("ESP-NOWキューリセットを実行中...");
        
        // ESP-NOWを一度無効化
        let result = unsafe { esp_now_deinit() };
        if result != 0 {
            error!("ESP-NOW無効化失敗: {} (0x{:X})", result, result as u32);
        }
        
        // 短時間待機
        FreeRtos::delay_ms(100);
        
        // ESP-NOWを再初期化
        let result = unsafe { esp_now_init() };
        if result != 0 {
            error!("ESP-NOW再初期化失敗: {} (0x{:X})", result, result as u32);
            return Err(EspNowError::InitFailed(result));
        }
        
        // コールバックを再設定
        unsafe {
            esp_now_register_send_cb(Some(esp_now_send_cb));
            esp_now_register_recv_cb(Some(esp_now_recv_cb));
        }
        
        info!("ESP-NOWキューリセット完了");
        Ok(())
    }
}

impl Drop for EspNowSender {
    fn drop(&mut self) {
        // 必要に応じてクリーンアップ処理を追加
    }
}

#[cfg(test)]
mod tests {
    // テストは環境が整ったタイミングで追加
}
