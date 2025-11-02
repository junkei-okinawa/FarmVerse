use simple_ds18b20_temp_sensor::TempSensor as DS18B20TempSensor;
use esp_idf_svc::hal::rmt::RmtChannel;
use esp_idf_svc::hal::peripheral::Peripheral;
use log::{info, warn, error};
use anyhow::Result;

/// 温度センサー管理構造体
/// 
/// DS18B20デジタル温度センサーを使用した温度測定を提供します。
/// 電源制御とRMTベース1-Wire通信に対応しています。
pub struct TempSensor {
    sensor: Option<DS18B20TempSensor>,
    power_pin: i32,
    data_pin: i32,
    temperature_offset: f32,
}

/// 温度測定結果
#[derive(Debug, Clone)]
pub struct TemperatureReading {
    /// 測定温度（℃）
    pub temperature_celsius: f32,
    /// 補正済み温度（℃）
    pub corrected_temperature_celsius: f32,
    /// 測定の信頼性（true: 正常、false: 警告あり）
    pub is_reliable: bool,
    /// 警告メッセージ（ある場合）
    pub warning_message: Option<String>,
}

impl TempSensor {
    /// 新しい温度センサーインスタンスを作成
    ///
    /// # 引数
    /// * `power_pin` - 電源制御用GPIO番号
    /// * `data_pin` - データ通信用GPIO番号
    /// * `temperature_offset` - 温度補正値（℃）
    /// * `rmt_channel` - RMTチャンネル（1-Wire通信用）
    ///
    /// # 配線例（XIAO ESP32S3）
    /// ```
    /// DS18B20 Temperature Sensor:
    /// - VCC -> GPIO2 (Power control)
    /// - GND -> GND
    /// - Data -> GPIO3 (with 4.7kΩ pull-up to 3.3V)
    /// ```
    pub fn new<C: RmtChannel>(
        power_pin: i32, 
        data_pin: i32, 
        temperature_offset: f32,
        rmt_channel: impl Peripheral<P = C> + 'static
    ) -> Result<Self> {
        info!("温度センサーを初期化中... (Power: GPIO{}, Data: GPIO{}, Offset: {:.1}°C)", 
              power_pin, data_pin, temperature_offset);

        // DS18B20センサーを初期化
        let sensor = match DS18B20TempSensor::new(power_pin, data_pin, rmt_channel) {
            Ok(sensor) => {
                info!("✓ DS18B20温度センサーの初期化に成功");
                Some(sensor)
            }
            Err(e) => {
                error!("DS18B20温度センサーの初期化に失敗: {:?}", e);
                warn!("温度センサーなしで動作します（デフォルト温度: 25.0°C）");
                None
            }
        };

        Ok(Self {
            sensor,
            power_pin,
            data_pin,
            temperature_offset,
        })
    }

    /// 温度を測定
    ///
    /// # 戻り値
    /// 温度測定結果（TemperatureReading構造体）
    /// センサーエラー時はデフォルト値（25.0°C）を返します
    pub fn read_temperature(&mut self) -> Result<TemperatureReading> {
        if let Some(ref mut sensor) = self.sensor {
            match sensor.read_temperature() {
                Ok(raw_temp) => {
                    let corrected_temp = raw_temp + self.temperature_offset;
                    
                    // 妥当性チェック
                    let (is_reliable, warning) = self.validate_temperature(corrected_temp);
                    
                    info!("🌡️ 温度測定: {:.1}°C (補正前: {:.1}°C, オフセット: {:.1}°C)", 
                          corrected_temp, raw_temp, self.temperature_offset);
                    
                    if let Some(ref msg) = warning {
                        warn!("温度測定警告: {}", msg);
                    }

                    Ok(TemperatureReading {
                        temperature_celsius: raw_temp,
                        corrected_temperature_celsius: corrected_temp,
                        is_reliable,
                        warning_message: warning,
                    })
                }
                Err(e) => {
                    warn!("温度センサー読み取りエラー: {:?}, デフォルト値を使用", e);
                    self.get_default_reading()
                }
            }
        } else {
            // センサーが初期化されていない場合はデフォルト値を返す
            self.get_default_reading()
        }
    }

    /// デフォルト温度読み取り結果を取得
    fn get_default_reading(&self) -> Result<TemperatureReading> {
        let default_temp = 25.0;
        let corrected_temp = default_temp + self.temperature_offset;
        
        Ok(TemperatureReading {
            temperature_celsius: default_temp,
            corrected_temperature_celsius: corrected_temp,
            is_reliable: false,
            warning_message: Some("センサーが利用できないため、デフォルト温度を使用".to_string()),
        })
    }

    /// 温度の妥当性を検証
    fn validate_temperature(&self, temperature: f32) -> (bool, Option<String>) {
        // 妥当な温度範囲をチェック（-40°C ~ +85°C: DS18B20の仕様範囲）
        if temperature < -40.0 || temperature > 85.0 {
            return (false, Some(format!("温度が仕様範囲外です: {:.1}°C", temperature)));
        }

        // 農業用途での一般的な範囲をチェック（-10°C ~ +60°C）
        if temperature < -10.0 || temperature > 60.0 {
            return (true, Some(format!("温度が一般的な農業用範囲外です: {:.1}°C", temperature)));
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
            "DS18B20温度センサー (Power: GPIO{}, Data: GPIO{}, Offset: {:.1}°C, Status: {})",
            self.power_pin,
            self.data_pin,
            self.temperature_offset,
            if self.is_sensor_available() { "利用可能" } else { "利用不可" }
        )
    }
}