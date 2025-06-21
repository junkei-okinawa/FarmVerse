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

/// 電圧測定用の定数
const MIN_MV: f32 = 128.0; // UnitCam GPIO0 の実測値に合わせて調整
const MAX_MV: f32 = 3130.0; // UnitCam GPIO0 の実測値に合わせて調整
const RANGE_MV: f32 = MAX_MV - MIN_MV;

/// 電圧センサー管理モジュール
pub struct VoltageSensor;

impl VoltageSensor {
    /// ADC2を使用してGPIO0から電圧を測定し、パーセンテージに変換
    /// 
    /// # Returns
    /// - 0-100: 正常な電圧パーセンテージ
    /// - 255: 測定エラー
    pub fn measure_voltage_percentage(
        adc2: ADC2,
        gpio0: Gpio0,
    ) -> anyhow::Result<u8> {
        info!("ADC2を初期化しています (GPIO0)");
        let adc_driver = AdcDriver::new(adc2)?;
        let adc_config = AdcChannelConfig {
            attenuation: DB_12,
            calibration: Calibration::Line,
            ..Default::default()
        };
        let mut adc_channel = AdcChannelDriver::new(&adc_driver, gpio0, &adc_config)?;

        info!("電圧を測定しパーセンテージを計算します...");
        let voltage_percent = match adc_channel.read() {
            Ok(voltage_mv_u16) => {
                let voltage_mv = voltage_mv_u16 as f32;
                info!("電圧測定成功: {:.0} mV", voltage_mv);
                
                let percentage = if RANGE_MV <= 0.0 {
                    0.0
                } else {
                    ((voltage_mv - MIN_MV) / RANGE_MV * 100.0)
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
