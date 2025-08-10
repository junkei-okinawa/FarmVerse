use esp_idf_svc::hal::{
    adc::{
        attenuation::DB_12,
        oneshot::{
            config::{AdcChannelConfig, Calibration},
            AdcChannelDriver, AdcDriver,
        },
        ADC1,
    },
    gpio::Gpio6,
};
use log::{error, info};
use crate::core::config::CONFIG;

/// ADC電圧センサー管理モジュール
pub struct VoltageSensor;

impl VoltageSensor {
    /// ADC2を使用してGPIO PINからADC電圧を測定し、パーセンテージに変換
    /// 
    /// # Returns
    /// - 0-100: 正常な電圧パーセンテージ
    /// - 255: 測定エラー
    pub fn measure_voltage_percentage(
        adc: ADC1,
        gpio_pin: Gpio6,
    ) -> anyhow::Result<u8> {
        info!("ADC1を初期化しています (GPIO6)");
        let adc_driver = AdcDriver::new(adc)?;
        let adc_config = AdcChannelConfig {
            attenuation: DB_12,
            calibration: Calibration::Curve,
            ..Default::default()
        };
        let mut adc_channel = AdcChannelDriver::new(&adc_driver, gpio_pin, &adc_config)?;

        info!("ADC電圧を測定しパーセンテージを計算します...");
        let voltage_percent = match adc_channel.read() {
            Ok(voltage_mv_u16) => {
                let voltage_mv = voltage_mv_u16 as f32;
                info!("ADC電圧測定成功: {:.0} mV", voltage_mv);
                
                let min_mv = CONFIG.adc_voltage_min_mv;
                let max_mv = CONFIG.adc_voltage_max_mv;
                let range_mv = max_mv - min_mv;
                
                let percentage = if range_mv <= 0.0 {
                    0.0
                } else {
                    ((voltage_mv - min_mv) / range_mv * 100.0)
                        .max(0.0)
                        .min(100.0)
                };
                
                let result = percentage.round() as u8;
                info!("計算されたパーセンテージ: {} %", result);
                result
            }
            Err(e) => {
                error!("ADC読み取りエラー: {:?}. 電圧は255%として扱います。", e);
                255
            }
        };

        // リソースを明示的に解放
        drop(adc_channel);
        drop(adc_driver);

        Ok(voltage_percent)
    }
}
