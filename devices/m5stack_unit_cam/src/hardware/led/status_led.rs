use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{Output, PinDriver};

/// LEDの制御に関するエラー
#[derive(Debug, thiserror::Error)]
pub enum LedError {
    #[error("LEDの初期化に失敗しました: {0}")]
    InitFailed(String),

    #[error("LEDの点灯制御に失敗しました: {0}")]
    ControlFailed(String),
}

/// ステータスLED制御
pub struct StatusLed {
    led: PinDriver<'static, esp_idf_svc::hal::gpio::Gpio4, Output>,
}

impl StatusLed {
    /// 新しいステータスLEDコントローラーを作成します
    ///
    /// # 引数
    ///
    /// * `pin` - GPIO4ピン
    ///
    /// # エラー
    ///
    /// LEDの初期化に失敗した場合にエラーを返します
    pub fn new(pin: esp_idf_svc::hal::gpio::Gpio4) -> Result<Self, LedError> {
        let led = PinDriver::output(pin).map_err(|e| LedError::InitFailed(format!("{:?}", e)))?;

        Ok(Self { led })
    }

    /// LEDを点灯させます
    ///
    /// # エラー
    ///
    /// LED制御に失敗した場合にエラーを返します
    pub fn turn_on(&mut self) -> Result<(), LedError> {
        self.led
            .set_high()
            .map_err(|e| LedError::ControlFailed(format!("{:?}", e)))
    }

    /// LEDを消灯させます
    ///
    /// # エラー
    ///
    /// LED制御に失敗した場合にエラーを返します
    pub fn turn_off(&mut self) -> Result<(), LedError> {
        self.led
            .set_low()
            .map_err(|e| LedError::ControlFailed(format!("{:?}", e)))
    }

    /// LED点滅パターンを実行します（エラー表示）
    ///
    /// # エラー
    ///
    /// LED制御に失敗した場合にエラーを返します
    pub fn blink_error(&mut self) -> Result<(), LedError> {
        for _ in 0..3 {
            self.turn_on()?;
            FreeRtos::delay_ms(300);
            self.turn_off()?;
            FreeRtos::delay_ms(300);
        }
        Ok(())
    }

    /// 成功時のLED点滅（短い点滅）
    ///
    /// # エラー
    ///
    /// LED制御に失敗した場合にエラーを返します
    pub fn blink_success(&mut self) -> Result<(), LedError> {
        for _ in 0..2 {
            self.turn_on()?;
            FreeRtos::delay_ms(100);
            self.turn_off()?;
            FreeRtos::delay_ms(100);
        }
        Ok(())
    }

    /// 送信中パターンを実行します
    ///
    /// # エラー
    ///
    /// LED制御に失敗した場合にエラーを返します
    pub fn indicate_sending(&mut self) -> Result<(), LedError> {
        self.turn_on()?;
        FreeRtos::delay_ms(100);
        self.turn_off()
    }
}
