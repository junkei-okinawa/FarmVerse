use esp_ec_sensor::{EcSensor, SensorConfig, EcReading};
use esp_idf_svc::hal::gpio::Gpio1;
use esp_idf_svc::hal::adc::ADC1;
use esp_idf_svc::hal::delay::FreeRtos;
use log::{info, warn, error};
use anyhow::Result;

/// EC/TDSセンサー管理構造体
/// 
/// esp-ec-sensorライブラリを使用してEC（電気伝導度）とTDS（総溶解固形分）を測定します。
/// 電源制御とADC1ベースのアナログ読み取りに対応しています。
pub struct EcTdsSensor {
    sensor: Option<EcSensor<Gpio1>>,
    power_pin_number: u8,
    adc_pin_number: u8,
    tds_factor: f32,
    temp_coefficient: f32,
}

/// EC/TDS測定結果
#[derive(Debug, Clone)]
pub struct EcTdsReading {
    /// EC値（μS/cm）
    pub ec_us_cm: f32,
    /// TDS濃度（ppm）
    pub tds_ppm: f32,
    /// ADC生値
    pub adc_value: u16,
    /// 測定の信頼性（true: 正常、false: 警告あり）
    pub is_reliable: bool,
    /// 警告メッセージ（ある場合）
    pub warning_message: Option<String>,
}

impl From<EcReading> for EcTdsReading {
    fn from(reading: EcReading) -> Self {
        Self {
            ec_us_cm: reading.ec_us_cm,
            tds_ppm: reading.tds_ppm,
            adc_value: reading.adc_value,
            is_reliable: true, // esp-ec-sensorは内部で検証済み
            warning_message: None,
        }
    }
}

impl EcTdsSensor {
    /// 新しいEC/TDSセンサーインスタンスを作成
    ///
    /// # 引数
    /// * `power_pin_number` - 電源制御用GPIO番号
    /// * `adc_pin_number` - ADC入力GPIO番号（実際にはGPIO1固定）
    /// * `tds_factor` - TDS変換係数（通常400-700）
    /// * `temp_coefficient` - 温度補正係数（通常0.02 = 2%/°C）
    /// * `adc_pin` - GPIO1ピン（ADC1対応、WiFi競合回避）
    /// * `adc1` - ADC1ペリフェラル
    ///
    /// # 配線例（XIAO ESP32S3）
    /// ```
    /// EC/TDS Sensor:
    /// - VCC -> GPIO4 (Power control)
    /// - GND -> GND  
    /// - Signal -> GPIO1 (ADC1対応、WiFi競合回避)
    /// ```
    pub fn new(
        power_pin_number: u8,
        adc_pin_number: u8,
        tds_factor: f32,
        calibrate_reference_adc: u16,
        calibrate_reference_ec: f32,
        temp_coefficient: f32,
        adc_pin: Gpio1,
        adc1: ADC1,
    ) -> Result<Self> {
        info!("EC/TDSセンサーを初期化中... (Power: GPIO{}, ADC: GPIO{}, TDS Factor: {:.1})", 
              power_pin_number, adc_pin_number, tds_factor);

        // センサー設定を作成
        let sensor_config = SensorConfig::new()
            .with_tds_factor(tds_factor)
            .with_temp_coefficient(temp_coefficient);

        // esp-ec-sensorライブラリを使用してセンサーを初期化
        let sensor = match EcSensor::new(
            power_pin_number as i32, // power_pin
            adc_pin, // ADC1対応ピン（WiFi競合回避）
            adc1,
            Some(sensor_config)
        ) {
            Ok(mut sensor) => {
                info!("✓ EC/TDSセンサーの初期化に成功");
                
                // 簡易キャリブレーション（デフォルト値使用）
                // 本格運用時は、実際の校正溶液を使用してキャリブレーションを行う
                if let Err(e) = sensor.calibrate_zero(0) {
                    warn!("ゼロ点キャリブレーション失敗: {:?}", e);
                } else {
                    info!("✓ ゼロ点キャリブレーション完了");
                }

                if let Err(e) = sensor.calibrate_reference(
                    calibrate_reference_adc,
                    calibrate_reference_ec
                ) {
                    warn!("参照点キャリブレーション失敗: {:?}", e);
                } else {
                    info!("✓ 参照点キャリブレーション完了 (1400 ADC = 1413.0 μS/cm)");
                }
                
                Some(sensor)
            }
            Err(e) => {
                error!("EC/TDSセンサーの初期化に失敗: {:?}", e);
                warn!("EC/TDSセンサーなしで動作します（ダミー値使用）");
                None
            }
        };

        Ok(Self {
            sensor,
            power_pin_number,
            adc_pin_number,
            tds_factor,
            temp_coefficient,
        })
    }

    /// EC/TDSセンサーからADC値を取得し電圧変換して値を返す
    /// 
    /// # 引数
    /// * `samples` - ADC読み取りのサンプル数
    /// * `delay_ms` - 各サンプル間の遅延時間（ミリ秒）
    /// 
    /// # 戻り値
    /// - (voltage, 成功時はSome(f32)、失敗時はNone)
    pub fn read_voltage(&mut self, samples: u8, delay_ms: u32) -> Result<Option<f32>> {
        if let Some(ref mut sensor) = self.sensor {
            // 単発のADC読み取り（平均化はライブラリ内で実施）
            match sensor.read_adc_averaged(samples, delay_ms) {
                Ok(adc_value) => {
                    let voltage = sensor.adc_to_voltage(adc_value);
                    match voltage {
                        Ok(voltage) => {
                            info!("✓ ADC電圧測定成功: {:.2} mV", voltage);
                            Ok(Some(voltage))
                        }
                        Err(e) => {
                            warn!("ADC電圧から電圧への変換エラー: {}, 電源をオフにします", e);
                            let _ = self.power_off();
                            Ok(None)
                        }
                    }
                }
                Err(e) => {
                    warn!("ADC平均読み取りエラー: {:?}, 電源をオフにします", e);
                    let _ = self.power_off();
                    Ok(None)
                }
            }
        } else {
            // センサーが初期化されていない場合はNoneを返す
            Ok(None)
        }
    }

    /// EC/TDS値を測定
    ///
    /// # 引数
    /// * `temperature_celsius` - 温度補正用の温度値（℃）
    ///
    /// # 戻り値
    /// EC/TDS測定結果（EcTdsReading構造体）
    /// センサーエラー時はダミー値を返します
    pub fn measure_ec_tds(&mut self, temperature_celsius: Option<f32>) -> Result<EcTdsReading> {
        if let Some(ref mut sensor) = self.sensor {
            match sensor.measure(temperature_celsius) {
                Ok(reading) => {
                    let mut result = EcTdsReading::from(reading);
                    
                    // 測定値の妥当性チェック
                    let (is_reliable, warning) = self.validate_measurement(&result);
                    result.is_reliable = is_reliable;
                    result.warning_message = warning;
                    
                    info!("🌊 EC/TDS測定完了: EC={:.1}μS/cm, TDS={:.1}ppm (ADC: {})", 
                          result.ec_us_cm, result.tds_ppm, result.adc_value);
                    
                    if let Some(ref msg) = result.warning_message {
                        warn!("EC/TDS測定警告: {}", msg);
                    }

                    Ok(result)
                }
                Err(e) => {
                    warn!("EC/TDSセンサー読み取りエラー: {:?}, 電源をオフにしダミー値を使用", e);
                    // [CASE 1] エラー発生時に電源を確実にオフにする
                    let _ = self.power_off();
                    self.get_default_reading()
                }
            }
        } else {
            // センサーが初期化されていない場合はダミー値を返す
            self.get_default_reading()
        }
    }

    /// センサーの電源を強制的にオフにする（Deep Sleepリーク対策）
    pub fn power_off(&self) -> Result<()> {
        use esp_idf_sys::{gpio_set_direction, gpio_set_level, gpio_mode_t_GPIO_MODE_OUTPUT};
        
        info!("EC/TDSセンサーの電源をオフにしています (GPIO{})", self.power_pin_number);
        unsafe {
            gpio_set_direction(self.power_pin_number as i32, gpio_mode_t_GPIO_MODE_OUTPUT);
            gpio_set_level(self.power_pin_number as i32, 0);
        }
        Ok(())
    }

    /// デフォルトEC/TDS読み取り結果を取得
    fn get_default_reading(&self) -> Result<EcTdsReading> {
        let default_ec = 100.0; // 100 μS/cm
        let default_tds = default_ec * (self.tds_factor / 1000.0);
        
        Ok(EcTdsReading {
            ec_us_cm: default_ec,
            tds_ppm: default_tds,
            adc_value: 500, // ダミーADC値
            is_reliable: false,
            warning_message: Some("センサーが利用できないため、ダミー値を使用".to_string()),
        })
    }

    /// 測定値の妥当性を検証
    fn validate_measurement(&self, reading: &EcTdsReading) -> (bool, Option<String>) {
        // EC値の妥当性チェック
        if reading.ec_us_cm < 0.0 {
            return (false, Some("EC値が負の値です".to_string()));
        }

        if reading.ec_us_cm > 10000.0 {
            return (false, Some(format!("EC値が異常に高いです: {:.1}μS/cm", reading.ec_us_cm)));
        }

        // TDS値の妥当性チェック（農業用途での一般的な範囲）
        if reading.tds_ppm > 2000.0 {
            return (true, Some(format!("TDS値が高いです: {:.1}ppm - 水質を確認してください", reading.tds_ppm)));
        }

        if reading.tds_ppm < 0.0 {
            return (false, Some("TDS値が負の値です".to_string()));
        }

        // ADC値の妥当性チェック
        if reading.adc_value == 0 {
            return (false, Some("ADC値が0です - センサー接続を確認してください".to_string()));
        }

        if reading.adc_value >= 4095 {
            return (false, Some("ADC値が飽和しています - 入力電圧が高すぎます".to_string()));
        }

        (true, None)
    }

    /// センサーの状態を取得
    pub fn is_sensor_available(&self) -> bool {
        self.sensor.is_some()
    }

    /// 設定情報を取得
    pub fn get_info(&self) -> String {
        format!(
            "EC/TDSセンサー (Power: GPIO{}, ADC: GPIO{}, TDS Factor: {:.1}, Temp Coeff: {:.3}, Status: {})",
            self.power_pin_number,
            self.adc_pin_number,
            self.tds_factor,
            self.temp_coefficient,
            if self.is_sensor_available() { "利用可能" } else { "利用不可" }
        )
    }

    /// TDS変換係数を取得
    pub fn get_tds_factor(&self) -> f32 {
        self.tds_factor
    }

    /// 温度補正係数を取得
    pub fn get_temp_coefficient(&self) -> f32 {
        self.temp_coefficient
    }
}