use super::{UsbError, UsbResult};
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::usb_serial::{UsbDMinGpio, UsbDPlusGpio, UsbSerialConfig, UsbSerialDriver};
use esp_idf_svc::sys;
use log::{debug, error, warn};

/// USB CDCドライバーを管理する構造体
pub struct UsbCdc<'d> {
    driver: UsbSerialDriver<'d>,
}

impl<'d> UsbCdc<'d> {
    /// 新しいUSB CDCインスタンスを作成します
    ///
    /// # 引数
    ///
    /// * `usb_serial` - USBシリアルペリフェラルオブジェクト
    /// * `pin_d_minus` - USBのD-ピン (ESP32-C3では通常GPIO18)
    /// * `pin_d_plus` - USBのD+ピン (ESP32-C3では通常GPIO19)
    ///
    /// # 戻り値
    ///
    /// * `UsbResult<Self>` - 成功した場合は`UsbCdc`インスタンス、
    ///   失敗した場合は`UsbError`
    pub fn new<U, DP, DN>(usb_serial: U, pin_d_minus: DN, pin_d_plus: DP) -> UsbResult<Self>
    where
        U: esp_idf_svc::hal::peripheral::Peripheral<P = esp_idf_svc::hal::usb_serial::USB_SERIAL>
            + 'd,
        DN: esp_idf_svc::hal::peripheral::Peripheral<P = UsbDMinGpio>,
        DP: esp_idf_svc::hal::peripheral::Peripheral<P = UsbDPlusGpio>,
    {
        // USB CDCの設定を作成（バッファサイズを増加させる）
        let mut config = UsbSerialConfig::new();
        config.tx_buffer_size = 4096; // 送信バッファを4096バイトに拡大
        config.rx_buffer_size = 4096; // 受信バッファを4096バイトに拡大

        // USB CDCドライバーを初期化
        let driver = UsbSerialDriver::new(usb_serial, pin_d_minus, pin_d_plus, &config)
            .map_err(|e| UsbError::InitError(format!("USB CDC initialization failed: {}", e)))?;

        debug!("USB CDC Initialized with buffer sizes: TX/RX: 4096 bytes");
        Ok(UsbCdc { driver })
    }

    /// データをUSB経由で送信します
    ///
    /// # 引数
    ///
    /// * `data` - 送信するデータ
    ///
    /// # 戻り値
    ///
    /// * `UsbResult<usize>` - 送信されたバイト数、または`UsbError`
    pub fn write(&mut self, data: &[u8], timeout_ms: u32) -> UsbResult<usize> {
        self.driver.write(data, timeout_ms).map_err(|e| e.into())
    }

    /// USB経由でデータを読み取ります
    ///
    /// # 引数
    ///
    /// * `buffer` - 読み取ったデータを格納するバッファ
    /// * `timeout_ms` - タイムアウト時間（ミリ秒）
    ///
    /// # 戻り値
    ///
    /// * `UsbResult<usize>` - 読み取ったバイト数、または`UsbError`
    pub fn read(&mut self, buffer: &mut [u8], timeout_ms: u32) -> UsbResult<usize> {
        self.driver.read(buffer, timeout_ms).map_err(|e| e.into())
    }

    /// USBからコマンドを読み取り、解析します
    ///
    /// # 引数
    ///
    /// * `timeout_ms` - タイムアウト時間（ミリ秒）
    ///
    /// # 戻り値
    ///
    /// * `UsbResult<Option<String>>` - コマンド文字列、またはタイムアウト時はNone
    pub fn read_command(&mut self, timeout_ms: u32) -> UsbResult<Option<String>> {
        let mut buffer = [0u8; 256]; // コマンド用のバッファ

        match self.read(&mut buffer, timeout_ms) {
            Ok(bytes_read) if bytes_read > 0 => {
                let command_str = String::from_utf8_lossy(&buffer[..bytes_read])
                    .trim()
                    .to_string();

                if !command_str.is_empty() {
                    debug!("USB command received: '{}'", command_str);
                    Ok(Some(command_str))
                } else {
                    Ok(None)
                }
            }
            Ok(_) => Ok(None), // 0バイト読み取り
            Err(UsbError::Timeout) => Ok(None), // タイムアウトは正常
            Err(e) => Err(e), // その他のエラー
        }
    }

    /// フレームデータをUSB CDC経由で送信します
    ///
    /// データを小さなチャンクに分割し、タイムアウトと再試行処理を実装します
    ///
    /// # 引数
    ///
    /// * `data` - 送信するフレーム化されたデータ
    /// * `mac_str` - ログ表示用のMACアドレス文字列
    ///
    /// # 戻り値
    ///
    /// * `UsbResult<usize>` - 送信に成功した場合は送信バイト数、
    ///   失敗した場合は`UsbError`
    pub fn send_frame(&mut self, data: &[u8], mac_str: &str) -> UsbResult<usize> {
        // 送信設定パラメータ
        const MAX_CHUNK_SIZE: usize = 64; // USBバッファサイズに合わせて調整
        const WRITE_TIMEOUT_MS: u32 = 30000; // 30秒のタイムアウト
        const MAX_RETRIES: u32 = 5; // 最大リトライ回数

        let mut bytes_sent = 0;
        let mut timeout = core::mem::MaybeUninit::<sys::TimeOut_t>::uninit();
        let mut write_timeout_ticks =
            (WRITE_TIMEOUT_MS as u64 * sys::configTICK_RATE_HZ as u64 / 1000) as u32;
        unsafe {
            sys::vTaskSetTimeOutState(timeout.as_mut_ptr());
        }
        let mut timeout = unsafe { timeout.assume_init() };
        let mut timeout_logged = false;
        let mut retry_count = 0;

        while bytes_sent < data.len() {
            // タイムアウトチェック
            if unsafe { sys::xTaskCheckForTimeOut(&mut timeout, &mut write_timeout_ticks) } != 0 {
                return Err(UsbError::Timeout);
            }

            // 小さなバッファで書き込み
            let remaining = data.len() - bytes_sent;
            let write_size = if remaining > MAX_CHUNK_SIZE {
                MAX_CHUNK_SIZE
            } else {
                remaining
            };
            let chunk_to_write = &data[bytes_sent..(bytes_sent + write_size)];

            // タイムアウト10msで書き込み試行
            match self.write(chunk_to_write, 10) {
                Ok(written) => {
                    if written > 0 {
                        bytes_sent += written;
                        retry_count = 0; // リトライカウンタリセット
                        timeout_logged = false;

                        // データ書き込みに成功した場合のログ（詳細レベル）
                        debug!(
                            "USB Write: {} bytes (Total: {}/{} - {:.1}%)",
                            written,
                            bytes_sent,
                            data.len(),
                            (bytes_sent as f32 / data.len() as f32) * 100.0
                        );
                    } else {
                        // 書き込みは成功したが0バイト
                        retry_count += 1;
                        if retry_count >= MAX_RETRIES {
                            warn!(
                                "USB CDC: Max retries ({}) reached with 0 bytes written",
                                MAX_RETRIES
                            );
                            FreeRtos::delay_ms(50); // より長く待機
                            retry_count = 0; // リトライカウンタリセット
                        }
                        FreeRtos::delay_ms(5);
                    }
                }
                Err(UsbError::Timeout) => {
                    // タイムアウト（バッファフル）の場合
                    retry_count += 1;
                    if !timeout_logged {
                        debug!("USB Write Timeout (Buffer Full?) for {}", mac_str);
                        timeout_logged = true;
                    }

                    if retry_count >= MAX_RETRIES {
                        warn!(
                            "USB CDC: Max retries ({}) reached due to timeouts",
                            MAX_RETRIES
                        );
                        FreeRtos::delay_ms(50); // より長く待機
                        retry_count = 0;
                    } else {
                        FreeRtos::delay_ms(10);
                    }
                }
                Err(e) => {
                    error!(
                        "USB CDC: Error writing chunk to USB CDC for {}: {}",
                        mac_str, e
                    );
                    return Err(e);
                }
            }
        } // 送信ループの終了

        // 送信成功後に少し待機（ホスト側の処理時間を考慮）
        FreeRtos::delay_ms(5);

        Ok(bytes_sent)
    }
}

#[cfg(test)]
mod tests {
    // USB CDCはハードウェア依存のため、単体テストは行わず
    // 統合テスト環境またはモックを使用して別途テストすることが望ましい
}
