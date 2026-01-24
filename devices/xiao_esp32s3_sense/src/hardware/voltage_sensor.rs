use esp_idf_svc::hal::{
    adc::{
        attenuation::DB_11,
        oneshot::{
            config::{AdcChannelConfig, Calibration},
            AdcChannelDriver, AdcDriver,
        },
        ADC1,
    },
};
use log::{error, info};
use crate::config::CONFIG;

/// ADC電圧センサー管理モジュール
pub struct VoltageSensor;

impl VoltageSensor {
    /// 電圧(mV)をパーセンテージに変換する純粋関数
    /// 
    /// # Arguments
    /// - `voltage_mv`: ADC測定電圧（ミリボルト）
    /// - `min_mv`: 最小電圧（0%相当）
    /// - `max_mv`: 最大電圧（100%相当）
    /// 
    /// # Returns
    /// - 0-100: 正常な電圧パーセンテージ
    /// 
    /// # Examples
    /// ```
    /// let percent = VoltageSensor::calculate_voltage_percentage(1629.0, 128.0, 3130.0);
    /// assert_eq!(percent, 50);
    /// ```
    pub fn calculate_voltage_percentage(voltage_mv: f32, min_mv: f32, max_mv: f32) -> u8 {
        let range_mv = max_mv - min_mv;
        
        let percentage = if range_mv <= 0.0 {
            0.0
        } else {
            ((voltage_mv - min_mv) / range_mv * 100.0)
                .max(0.0)
                .min(100.0)
        };
        
        percentage.round() as u8
    }

    /// ADC1を使用してGPIO PINからADC電圧を測定し、パーセンテージに変換
    /// WiFi競合を避けるため、WiFi初期化前に実行する必要があります
    /// 
    /// # Returns
    /// - (電圧パーセンテージ, ADC1): 測定結果とADC1の所有権
    ///   - 電圧パーセンテージ: 通常は 0–100 の値を取り、`255` は測定に失敗したことを示します
    pub fn measure_voltage_percentage<T: esp_idf_svc::hal::gpio::ADCPin>(
        mut adc: ADC1,
        gpio_pin: T,
    ) -> anyhow::Result<(u8, ADC1)> {
        info!("ADC1を初期化しています (WiFi競合回避)");
        let adc_driver = AdcDriver::new(&mut adc)?;
        let adc_config = AdcChannelConfig {
            attenuation: DB_11,
            calibration: Calibration::Curve,
            ..Default::default()
        };
        let mut adc_channel = AdcChannelDriver::new(&adc_driver, gpio_pin, &adc_config)?;

        info!("ADC電圧を10回測定し、平均値を計算します...");
        let mut sum_mv = 0u32;
        let mut samples = 0u8;

        for _ in 0..10 {
            match adc_channel.read() {
                Ok(mv) => {
                    sum_mv += mv as u32;
                    samples += 1;
                }
                Err(e) => {
                    error!("ADCサンプル読み取りエラー: {:?}", e);
                }
            }
            // 短いウェイトを入れてノイズを分散
            esp_idf_svc::hal::delay::FreeRtos::delay_ms(10);
        }

        let voltage_percent = if samples > 0 {
            let avg_mv = (sum_mv / samples as u32) as f32;
            info!("ADC電圧測定結果: 平均値={:.0} mV, サンプル数={}", avg_mv, samples);
            
            let min_mv = CONFIG.adc_voltage_min_mv as f32;
            let max_mv = CONFIG.adc_voltage_max_mv as f32;
            
            let result = Self::calculate_voltage_percentage(avg_mv, min_mv, max_mv);
            info!("計算されたパーセンテージ: {} % (設定範囲: {} - {} mV)", result, min_mv, max_mv);
            result
        } else {
            error!("有効なADCサンプルが取得できませんでした。電圧は255%として扱います。");
            255
        };

        // ADCチャンネルを解放してADCドライバーからADC1を取り戻す
        drop(adc_channel);
        drop(adc_driver);

        Ok((voltage_percent, adc)) // ADC1の所有権を返す
    }
}
