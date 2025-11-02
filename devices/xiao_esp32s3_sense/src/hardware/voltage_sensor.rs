use esp_idf_svc::hal::{
    adc::{
        attenuation::DB_11,
        oneshot::{
            config::{AdcChannelConfig, Calibration},
            AdcChannelDriver, AdcDriver,
        },
        ADC1,
    },
    gpio::Gpio6,
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
    /// - 0-100: 正常な電圧パーセンテージ
    /// - 255: 測定エラー
    pub fn measure_voltage_percentage(
        mut adc: ADC1,
        gpio_pin: Gpio6,
    ) -> anyhow::Result<(u8, ADC1)> {
        info!("ADC1を初期化しています (GPIO6, WiFi競合回避)");
        let adc_driver = AdcDriver::new(&mut adc)?;
        let adc_config = AdcChannelConfig {
            attenuation: DB_11,
            calibration: Calibration::Curve,
            ..Default::default()
        };
        let mut adc_channel = AdcChannelDriver::new(&adc_driver, gpio_pin, &adc_config)?;

        info!("ADC電圧を測定しパーセンテージを計算します...");
        let voltage_percent = match adc_channel.read() {
            Ok(voltage_mv_u16) => {
                let voltage_mv = voltage_mv_u16 as f32;
                info!("ADC電圧測定成功: {:.0} mV", voltage_mv);
                
                let min_mv = CONFIG.adc_voltage_min_mv as f32;
                let max_mv = CONFIG.adc_voltage_max_mv as f32;
                
                let result = Self::calculate_voltage_percentage(voltage_mv, min_mv, max_mv);
                info!("計算されたパーセンテージ: {} %", result);
                result
            }
            Err(e) => {
                error!("ADC読み取りエラー: {:?}. 電圧は255%として扱います。", e);
                255
            }
        };

        // ADCチャンネルを解放してADCドライバーからADC1を取り戻す
        drop(adc_channel);
        drop(adc_driver);

        Ok((voltage_percent, adc)) // ADC1の所有権を返す
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voltage_percentage_calculation_50_percent() {
        // 中間値のテスト: (1629 - 128) / (3130 - 128) ≈ 0.5
        let result = VoltageSensor::calculate_voltage_percentage(1629.0, 128.0, 3130.0);
        assert_eq!(result, 50);
    }

    #[test]
    fn test_voltage_percentage_calculation_0_percent() {
        // 最小値のテスト
        let result = VoltageSensor::calculate_voltage_percentage(128.0, 128.0, 3130.0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_voltage_percentage_calculation_100_percent() {
        // 最大値のテスト
        let result = VoltageSensor::calculate_voltage_percentage(3130.0, 128.0, 3130.0);
        assert_eq!(result, 100);
    }

    #[test]
    fn test_voltage_percentage_below_minimum() {
        // 最小値より低い場合は0%にクランプされる
        let result = VoltageSensor::calculate_voltage_percentage(50.0, 128.0, 3130.0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_voltage_percentage_above_maximum() {
        // 最大値より高い場合は100%にクランプされる
        let result = VoltageSensor::calculate_voltage_percentage(3500.0, 128.0, 3130.0);
        assert_eq!(result, 100);
    }

    #[test]
    fn test_voltage_percentage_invalid_range() {
        // 無効な範囲（max <= min）の場合は0%を返す
        let result = VoltageSensor::calculate_voltage_percentage(1500.0, 3130.0, 128.0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_voltage_percentage_zero_range() {
        // 範囲が0の場合は0%を返す
        let result = VoltageSensor::calculate_voltage_percentage(1500.0, 1500.0, 1500.0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_voltage_percentage_realistic_values() {
        // 実際の使用例: 2000mV (約62%)
        let result = VoltageSensor::calculate_voltage_percentage(2000.0, 128.0, 3130.0);
        assert_eq!(result, 62);
    }
}
