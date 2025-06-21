use crate::mac_address::MacAddress;
use esp_idf_svc::hal::delay::{FreeRtos, TickType};
use esp_idf_svc::hal::mutex::{Condvar, Mutex, RawMutex};
use esp_idf_svc::sys::{
    esp_now_add_peer, esp_now_deinit, esp_now_init, esp_now_peer_info_t, esp_now_register_recv_cb,
    esp_now_register_send_cb, esp_now_recv_info_t, esp_now_send, esp_err_t,
    esp_now_send_status_t, esp_now_send_status_t_ESP_NOW_SEND_SUCCESS,
    wifi_interface_t_WIFI_IF_STA, ESP_IF_WIFI_STA, ESP_OK,
};
use esp_idf_svc::sys::EspError;
use log::{error, info, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use core::slice;
use std::time::Duration;

static LAST_RECEIVED_SLEEP_DURATION_SECONDS: Mutex<Option<u32>> = Mutex::new(None);
// static RECEIVE_FLAG: AtomicBool = AtomicBool::new(false); // Removed
static SLEEP_CMD_CONDVAR: Condvar = Condvar::new();

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

    #[error("ESP-NOW receive error: {0}")]
    RecvError(String),

    #[error("ESP-NOW receive timeout")]
    RecvTimeout,
}

/// 送信状態を共有するためのグローバルチャネル
static SEND_COMPLETE: AtomicBool = AtomicBool::new(true);
static SEND_FAILED: AtomicBool = AtomicBool::new(false);

/// ESP-NOW送信機
#[derive(Debug)]
pub struct EspNowSender {
    #[allow(dead_code)]
    initialized: bool,
}

impl EspNowSender {
    /// ESP-NOW受信コールバック
    unsafe extern "C" fn esp_now_recv_cb(recv_info: *const esp_now_recv_info_t, data: *const u8, len: i32) {
        if data.is_null() || len < 4 { // Expecting at least 4 bytes for duration
            warn!("Received invalid ESP-NOW data: data is null or length is too short (len = {})", len);
            return;
        }

        // The redundant 'if len < 4' block that was here is now removed.

        let duration_slice = slice::from_raw_parts(data.add(len as usize - 4), 4);
        let duration = match duration_slice.try_into() {
            Ok(bytes) => u32::from_le_bytes(bytes),
            Err(_) => {
                warn!("Failed to parse sleep duration from received data slice. Data len: {}, MAC: {:02x?}", len, MacAddress((*recv_info).src_addr));
                return; // Return early on parse error
            }
        };

        if duration > 0 { // Assuming 0 is not a valid sleep duration to be acted upon here
            let mut locked_duration = LAST_RECEIVED_SLEEP_DURATION_SECONDS.lock();
            *locked_duration = Some(duration);
            SLEEP_CMD_CONDVAR.notify_one(); // Notify the waiting thread
            info!("ESP-NOW: Received sleep duration: {} seconds from MAC: {:02x?}", duration, MacAddress((*recv_info).src_addr));
        } else if len >= 10 { // Attempt to log MAC if we likely have it
            info!("ESP-NOW: Received data (len {}) from MAC: {:02x?}, but no valid sleep duration parsed.", len, MacAddress((*recv_info).src_addr));
        } else {
            info!("ESP-NOW: Received data (len {}), but no valid sleep duration parsed.", len);
        }
    }

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

    /// 新しいESP-NOW送信機を初期化します
    ///
    /// # エラー
    ///
    /// ESP-NOWの初期化に失敗した場合にエラーを返します
    pub fn new() -> Result<Self, EspNowError> {
        let result_init = unsafe { esp_now_init() };
        if result_init != ESP_OK {
            error!("Failed to initialize ESP-NOW: {}", result_init);
            return Err(EspNowError::InitFailed(result_init));
        }

        let result_send_cb = unsafe { esp_now_register_send_cb(Some(Self::esp_now_send_cb)) };
        if result_send_cb != ESP_OK {
            error!("Failed to register ESP-NOW send callback: {}", result_send_cb);
            // Consider de-initializing ESP-NOW here if send CB registration fails
            unsafe { esp_now_deinit(); }
            return Err(EspNowError::InitFailed(result_send_cb)); // Or a more specific error
        }

        let result_recv_cb = unsafe { esp_now_register_recv_cb(Some(Self::esp_now_recv_cb)) };
        if result_recv_cb != ESP_OK {
            error!("Failed to register ESP-NOW receive callback: {}", result_recv_cb);
            // Consider de-initializing ESP-NOW here
            unsafe { esp_now_deinit(); }
            return Err(EspNowError::InitFailed(result_recv_cb)); // Or a new error type for recv_cb registration
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
        let mut peer_info = esp_now_peer_info_t::default();
        peer_info.channel = 0;
        peer_info.ifidx = wifi_interface_t_WIFI_IF_STA;
        peer_info.encrypt = false;
        peer_info.peer_addr = peer_mac.0;

        let result = unsafe { esp_now_add_peer(&peer_info) };
        if result != 0 {
            return Err(EspNowError::AddPeerFailed(result));
        }

        Ok(())
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
        // 前回の送信が完了するまで待機
        let mut timeout_counter = 0;
        while !SEND_COMPLETE.load(Ordering::SeqCst) {
            FreeRtos::delay_ms(1);
            timeout_counter += 1;
            if timeout_counter > timeout_ms {
                return Err(EspNowError::SendTimeout);
            }
        }

        // 送信状態をリセット
        SEND_COMPLETE.store(false, Ordering::SeqCst);
        SEND_FAILED.store(false, Ordering::SeqCst);

        // データを送信
        let result = unsafe { esp_now_send(peer_mac.0.as_ptr(), data.as_ptr(), data.len()) };
        if result != ESP_OK { // Changed from result != 0
            SEND_COMPLETE.store(true, Ordering::SeqCst);
            return Err(EspNowError::SendFailed(result));
        }

        // 送信完了を待機
        timeout_counter = 0;
        while !SEND_COMPLETE.load(Ordering::SeqCst) {
            FreeRtos::delay_ms(1);
            timeout_counter += 1;
            if timeout_counter > timeout_ms {
                return Err(EspNowError::SendTimeout);
            }
        }

        // 送信結果を確認
        if SEND_FAILED.load(Ordering::SeqCst) {
            return Err(EspNowError::SendFailedCallback);
        }

        Ok(())
    }

    /// ESP-NOW経由でスリープコマンドを受信します
    ///
    /// # 引数
    ///
    /// * `timeout_ms` - 受信タイムアウト（ミリ秒）
    ///
    /// # 戻り値
    ///
    /// 受信したスリープ時間（秒）。タイムアウトした場合は`EspNowError::RecvTimeout`。
    /// データを受信したが解析できなかった場合は`None`を返す代わりにエラーを返すことも検討。
    /// 現在の実装では、解析成功時のみ`Ok(Some(duration))`を返す。
    pub fn receive_sleep_command(&self, timeout_ms: u32) -> Result<Option<u32>, EspNowError> {
        let mut duration_guard = LAST_RECEIVED_SLEEP_DURATION_SECONDS.lock();
        *duration_guard = None; // Clear previous duration

        let timeout_duration = std::time::Duration::from_millis(timeout_ms as u64);

        // Wait until the duration_guard is Some or timeout occurs
        // The condition for wait_timeout_while should be true as long as we need to wait.
        // So, we wait while *duration_guard is None.
        let result = SLEEP_CMD_CONDVAR.wait_timeout_while(
            &mut duration_guard,
            timeout_duration,
            |shared_data_opt| shared_data_opt.is_none(),
        );

        if result.timed_out() {
            warn!("Timeout waiting for sleep command via Condvar");
            Err(EspNowError::RecvTimeout)
        } else {
            // MutexGuard `duration_guard` is still locked here.
            // The value is what was set by the callback.
            if duration_guard.is_some() {
                // info!("Condvar received sleep command: {:?} seconds", *duration_guard); // Logged in cb
                Ok(*duration_guard) // This dereferences the MutexGuard then copies Option<u32>
            } else {
                // This case (woken up but data is None) should ideally not be hit
                // if the predicate `|shared_data_opt| shared_data_opt.is_none()`
                // and callback logic are correct.
                warn!("Condvar woken up but no sleep duration found.");
                // Potentially return Ok(None) or a specific error
                Ok(None) // Or perhaps an error indicating an unexpected state
            }
        }
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
            self.send(peer_mac, chunk, 1000)?;

            // チャンク間にディレイを挿入
            if delay_between_chunks_ms > 0 {
                FreeRtos::delay_ms(delay_between_chunks_ms);
            }
        }

        // EOFマーカー送信
        FreeRtos::delay_ms(15); // EOFマーカー送信前に少し待機
        self.send(peer_mac, b"EOF!", 1000)?;

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
