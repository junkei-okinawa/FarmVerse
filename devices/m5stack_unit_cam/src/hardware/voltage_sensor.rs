use esp_idf_svc::hal::{
    adc::{
        attenuation::DB_12,
        oneshot::{
            config::{AdcChannelConfig, Calibration},
            AdcChannelDriver, AdcDriver,
        },
        ADC2,
    },
    gpio::Gpio0,
};
use log::{error, info};
use crate::core::config::CONFIG;
use crate::core::voltage_to_percentage;

/// ADC電圧センサー管理モジュール
pub struct VoltageSensor;

impl VoltageSensor {
    /// ADC2を使用してGPIO0からADC電圧を測定し、パーセンテージに変換
    /// 
    /// # Returns
    /// - 0-100: 正常な電圧パーセンテージ
    /// - 255: 測定エラー
    pub fn measure_voltage_percentage(
        mut adc2: ADC2,
        mut gpio0: Gpio0,
    ) -> anyhow::Result<(u8, ADC2, Gpio0)> {
        info!("ADC2を初期化しています (GPIO0)");
        let adc_driver = AdcDriver::new(&mut adc2)?;
        let adc_config = AdcChannelConfig {
            attenuation: DB_12,
            calibration: Calibration::Line,
            ..Default::default()
        };
        let mut adc_channel = AdcChannelDriver::new(&adc_driver, &mut gpio0, &adc_config)?;

        info!("ADC電圧を測定しパーセンテージを計算します...");
        let mut voltage_percent = match adc_channel.read() {
            Ok(voltage_mv_u16) => {
                let voltage_mv = voltage_mv_u16 as f32;
                info!("ADC電圧測定成功: {:.0} mV", voltage_mv);

                let result = voltage_to_percentage(
                    voltage_mv,
                    CONFIG.adc_voltage_min_mv as f32,
                    CONFIG.adc_voltage_max_mv as f32,
                );
                info!("計算されたパーセンテージ: {} %", result);
                result
            }
            Err(e) => {
                error!("ADC読み取りエラー: {:?}. 電圧は255%として扱います。", e);
                255
            }
        };

        if CONFIG.force_voltage_percent_50 {
            info!("force_voltage_percent_50=true のため、電圧を 50% に強制します");
            voltage_percent = 50;
        }

        // リソースを明示的に解放
        drop(adc_channel);
        drop(adc_driver);

        Ok((voltage_percent, adc2, gpio0))
    }
}
