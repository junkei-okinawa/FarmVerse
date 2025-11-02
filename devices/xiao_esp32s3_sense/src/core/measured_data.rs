/// 測定データ構造体（ハードウェア非依存）
#[derive(Debug, Clone, PartialEq)]
pub struct MeasuredData {
    pub voltage_percent: u8,
    pub image_data: Option<Vec<u8>>,
    pub temperature_celsius: Option<f32>,
    pub tds_voltage: Option<f32>,
    pub tds_ppm: Option<f32>,
    pub sensor_warnings: Vec<String>,
}

impl MeasuredData {
    /// 新しいMeasuredDataインスタンスを作成
    pub fn new(voltage_percent: u8, image_data: Option<Vec<u8>>) -> Self {
        Self {
            voltage_percent,
            image_data,
            temperature_celsius: None,
            tds_voltage: None,
            tds_ppm: None,
            sensor_warnings: Vec::new(),
        }
    }

    /// 温度データを追加
    pub fn with_temperature(mut self, temperature: Option<f32>) -> Self {
        self.temperature_celsius = temperature;
        self
    }

    /// TDS電圧データを追加
    pub fn with_tds_voltage(mut self, voltage: Option<f32>) -> Self {
        self.tds_voltage = voltage;
        self
    }
    
    /// TDSデータを追加
    pub fn with_tds(mut self, tds: Option<f32>) -> Self {
        self.tds_ppm = tds;
        self
    }

    /// 警告メッセージを追加
    pub fn add_warning(&mut self, warning: String) {
        self.sensor_warnings.push(warning);
    }

    /// 測定データのサマリを取得
    pub fn get_summary(&self) -> String {
        let mut parts = vec![format!("電圧:{}%", self.voltage_percent)];

        if let Some(temp) = self.temperature_celsius {
            parts.push(format!("温度:{:.1}°C", temp));
        }

        if let Some(voltage) = self.tds_voltage {
            parts.push(format!("TDS電圧:{:.2}V", voltage));
        }

        if let Some(tds) = self.tds_ppm {
            parts.push(format!("TDS:{:.1}ppm", tds));
        }

        if let Some(ref image_data) = self.image_data {
            parts.push(format!("画像:{}bytes", image_data.len()));
        }

        if !self.sensor_warnings.is_empty() {
            parts.push(format!("警告:{}件", self.sensor_warnings.len()));
        }

        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_minimal_data() {
        let data = MeasuredData::new(50, None);
        
        assert_eq!(data.voltage_percent, 50);
        assert_eq!(data.image_data, None);
        assert_eq!(data.temperature_celsius, None);
        assert_eq!(data.tds_voltage, None);
        assert_eq!(data.tds_ppm, None);
        assert_eq!(data.sensor_warnings.len(), 0);
    }

    #[test]
    fn test_new_with_image_data() {
        let image = vec![0xFF, 0xD8, 0xFF, 0xE0]; // JPEG header
        let data = MeasuredData::new(75, Some(image.clone()));
        
        assert_eq!(data.voltage_percent, 75);
        assert_eq!(data.image_data, Some(image));
    }

    #[test]
    fn test_builder_pattern_with_temperature() {
        let data = MeasuredData::new(60, None)
            .with_temperature(Some(25.5));
        
        assert_eq!(data.temperature_celsius, Some(25.5));
    }

    #[test]
    fn test_builder_pattern_with_tds_voltage() {
        let data = MeasuredData::new(70, None)
            .with_tds_voltage(Some(2.5));
        
        assert_eq!(data.tds_voltage, Some(2.5));
    }

    #[test]
    fn test_builder_pattern_with_tds() {
        let data = MeasuredData::new(80, None)
            .with_tds(Some(450.0));
        
        assert_eq!(data.tds_ppm, Some(450.0));
    }

    #[test]
    fn test_builder_pattern_chaining() {
        let data = MeasuredData::new(90, None)
            .with_temperature(Some(26.3))
            .with_tds_voltage(Some(1.8))
            .with_tds(Some(320.5));
        
        assert_eq!(data.voltage_percent, 90);
        assert_eq!(data.temperature_celsius, Some(26.3));
        assert_eq!(data.tds_voltage, Some(1.8));
        assert_eq!(data.tds_ppm, Some(320.5));
    }

    #[test]
    fn test_add_warning() {
        let mut data = MeasuredData::new(50, None);
        
        data.add_warning("温度センサーエラー".to_string());
        data.add_warning("TDSセンサー未接続".to_string());
        
        assert_eq!(data.sensor_warnings.len(), 2);
        assert_eq!(data.sensor_warnings[0], "温度センサーエラー");
        assert_eq!(data.sensor_warnings[1], "TDSセンサー未接続");
    }

    #[test]
    fn test_get_summary_minimal() {
        let data = MeasuredData::new(50, None);
        let summary = data.get_summary();
        
        assert_eq!(summary, "電圧:50%");
    }

    #[test]
    fn test_get_summary_with_temperature() {
        let data = MeasuredData::new(60, None)
            .with_temperature(Some(25.7));
        let summary = data.get_summary();
        
        assert_eq!(summary, "電圧:60%, 温度:25.7°C");
    }

    #[test]
    fn test_get_summary_with_tds_voltage() {
        let data = MeasuredData::new(70, None)
            .with_tds_voltage(Some(2.34));
        let summary = data.get_summary();
        
        assert_eq!(summary, "電圧:70%, TDS電圧:2.34V");
    }

    #[test]
    fn test_get_summary_with_tds() {
        let data = MeasuredData::new(80, None)
            .with_tds(Some(456.8));
        let summary = data.get_summary();
        
        assert_eq!(summary, "電圧:80%, TDS:456.8ppm");
    }

    #[test]
    fn test_get_summary_with_image() {
        let image = vec![0u8; 1024];
        let data = MeasuredData::new(90, Some(image));
        let summary = data.get_summary();
        
        assert_eq!(summary, "電圧:90%, 画像:1024bytes");
    }

    #[test]
    fn test_get_summary_with_warnings() {
        let mut data = MeasuredData::new(40, None);
        data.add_warning("低電圧".to_string());
        let summary = data.get_summary();
        
        assert_eq!(summary, "電圧:40%, 警告:1件");
    }

    #[test]
    fn test_get_summary_full() {
        let image = vec![0u8; 512];
        let mut data = MeasuredData::new(95, Some(image))
            .with_temperature(Some(28.3))
            .with_tds_voltage(Some(3.1))
            .with_tds(Some(650.2));
        
        data.add_warning("テスト警告".to_string());
        
        let summary = data.get_summary();
        assert_eq!(summary, "電圧:95%, 温度:28.3°C, TDS電圧:3.10V, TDS:650.2ppm, 画像:512bytes, 警告:1件");
    }

    #[test]
    fn test_voltage_boundary_values() {
        let data_min = MeasuredData::new(0, None);
        assert_eq!(data_min.voltage_percent, 0);
        
        let data_max = MeasuredData::new(100, None);
        assert_eq!(data_max.voltage_percent, 100);
    }

    #[test]
    fn test_temperature_negative() {
        let data = MeasuredData::new(50, None)
            .with_temperature(Some(-5.5));
        
        assert_eq!(data.temperature_celsius, Some(-5.5));
        assert_eq!(data.get_summary(), "電圧:50%, 温度:-5.5°C");
    }

    #[test]
    fn test_empty_image_data() {
        let data = MeasuredData::new(50, Some(Vec::new()));
        let summary = data.get_summary();
        
        assert_eq!(summary, "電圧:50%, 画像:0bytes");
    }

    #[test]
    fn test_clone() {
        let original = MeasuredData::new(75, None)
            .with_temperature(Some(22.5));
        
        let cloned = original.clone();
        
        assert_eq!(cloned.voltage_percent, original.voltage_percent);
        assert_eq!(cloned.temperature_celsius, original.temperature_celsius);
    }
}
