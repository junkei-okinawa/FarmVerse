// ユニットテスト実装例
// このファイルは実装例として作成されており、実際の統合には修正が必要です

/// 電圧計算ロジック（ハードウェア非依存）
pub fn calculate_voltage_percentage(voltage_mv: f32, min_mv: f32, max_mv: f32) -> u8 {
    let range_mv = max_mv - min_mv;
    
    if range_mv <= 0.0 {
        return 0;
    }
    
    let percentage = ((voltage_mv - min_mv) / range_mv * 100.0)
        .max(0.0)
        .min(100.0);
    
    percentage.round() as u8
}

/// ESP-NOWフレーム構築ロジック
pub fn build_hash_frame(
    hash: &str,
    voltage_percentage: u8,
    temperature_celsius: Option<f32>,
    tds_voltage: Option<f32>,
    timestamp: &str,
) -> String {
    let temp_data = temperature_celsius.unwrap_or(-999.0);
    let tds_data = tds_voltage.unwrap_or(-999.0);
    format!(
        "HASH:{},VOLT:{},TEMP:{:.1},TDS_VOLT:{:.1},{}",
        hash, voltage_percentage, temp_data, tds_data, timestamp
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voltage_percentage_normal_range() {
        // 正常範囲内の電圧
        let result = calculate_voltage_percentage(1629.0, 128.0, 3130.0);
        assert_eq!(result, 50); // (1629 - 128) / (3130 - 128) * 100 ≈ 50%
    }

    #[test]
    fn test_voltage_percentage_min() {
        // 最小値
        let result = calculate_voltage_percentage(128.0, 128.0, 3130.0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_voltage_percentage_max() {
        // 最大値
        let result = calculate_voltage_percentage(3130.0, 128.0, 3130.0);
        assert_eq!(result, 100);
    }

    #[test]
    fn test_voltage_percentage_below_min() {
        // 最小値以下（クランプされる）
        let result = calculate_voltage_percentage(50.0, 128.0, 3130.0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_voltage_percentage_above_max() {
        // 最大値以上（クランプされる）
        let result = calculate_voltage_percentage(4000.0, 128.0, 3130.0);
        assert_eq!(result, 100);
    }

    #[test]
    fn test_voltage_percentage_invalid_range() {
        // 無効な範囲（min >= max）
        let result = calculate_voltage_percentage(1500.0, 3130.0, 128.0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_build_hash_frame_with_all_data() {
        let frame = build_hash_frame(
            "abc123def456",
            85,
            Some(25.5),
            Some(3.2),
            "2024-01-15T10:30:00Z"
        );
        
        assert_eq!(
            frame,
            "HASH:abc123def456,VOLT:85,TEMP:25.5,TDS_VOLT:3.2,2024-01-15T10:30:00Z"
        );
    }

    #[test]
    fn test_build_hash_frame_without_temperature() {
        let frame = build_hash_frame(
            "abc123def456",
            75,
            None,
            Some(2.8),
            "2024-01-15T10:30:00Z"
        );
        
        assert_eq!(
            frame,
            "HASH:abc123def456,VOLT:75,TEMP:-999.0,TDS_VOLT:2.8,2024-01-15T10:30:00Z"
        );
    }

    #[test]
    fn test_build_hash_frame_without_tds() {
        let frame = build_hash_frame(
            "abc123def456",
            90,
            Some(26.0),
            None,
            "2024-01-15T10:30:00Z"
        );
        
        assert_eq!(
            frame,
            "HASH:abc123def456,VOLT:90,TEMP:26.0,TDS_VOLT:-999.0,2024-01-15T10:30:00Z"
        );
    }

    #[test]
    fn test_build_hash_frame_without_sensors() {
        let frame = build_hash_frame(
            "xyz789",
            100,
            None,
            None,
            "2024-01-15T10:30:00Z"
        );
        
        assert_eq!(
            frame,
            "HASH:xyz789,VOLT:100,TEMP:-999.0,TDS_VOLT:-999.0,2024-01-15T10:30:00Z"
        );
    }
}

// 温度計算のテスト例
#[cfg(test)]
mod temperature_tests {
    /// DS18B20の生データを摂氏温度に変換（仮想実装）
    fn raw_to_celsius(raw_value: i16) -> f32 {
        // DS18B20は16ビット符号付き整数で温度を返す
        // 1LSB = 0.0625℃
        (raw_value as f32) * 0.0625
    }

    /// 温度オフセット補正を適用
    fn apply_temperature_offset(celsius: f32, offset: f32) -> f32 {
        celsius + offset
    }

    #[test]
    fn test_raw_to_celsius_positive() {
        // 25℃ = 400 (0x0190)
        assert_eq!(raw_to_celsius(400), 25.0);
    }

    #[test]
    fn test_raw_to_celsius_negative() {
        // -10℃ = -160 (0xFF60)
        assert_eq!(raw_to_celsius(-160), -10.0);
    }

    #[test]
    fn test_raw_to_celsius_zero() {
        assert_eq!(raw_to_celsius(0), 0.0);
    }

    #[test]
    fn test_raw_to_celsius_fractional() {
        // 25.5℃ = 408 (0x0198)
        assert_eq!(raw_to_celsius(408), 25.5);
    }

    #[test]
    fn test_temperature_offset_positive() {
        let result = apply_temperature_offset(25.0, 0.7);
        assert_eq!(result, 25.7);
    }

    #[test]
    fn test_temperature_offset_negative() {
        let result = apply_temperature_offset(25.0, -0.5);
        assert_eq!(result, 24.5);
    }
}

// TDS/EC計算のテスト例
#[cfg(test)]
mod tds_tests {
    /// ADC電圧からTDS値を計算（ppm）
    fn calculate_tds_ppm(voltage: f32, temperature_celsius: f32, tds_factor: f32) -> f32 {
        // 温度補償係数（25℃基準）
        let temp_coefficient = 1.0 + 0.02 * (temperature_celsius - 25.0);
        
        // TDS計算（簡易版）
        // TDS (ppm) = (voltage * tds_factor) / temp_coefficient
        (voltage * tds_factor) / temp_coefficient
    }

    #[test]
    fn test_tds_calculation_at_25c() {
        // 25℃での計算（補償なし）
        let result = calculate_tds_ppm(2.5, 25.0, 500.0);
        assert_eq!(result, 1250.0); // 2.5V * 500 factor
    }

    #[test]
    fn test_tds_calculation_above_25c() {
        // 30℃での計算（温度補償あり）
        let result = calculate_tds_ppm(2.5, 30.0, 500.0);
        // temp_coefficient = 1.0 + 0.02 * (30 - 25) = 1.1
        // TDS = (2.5 * 500) / 1.1 ≈ 1136.36
        assert!((result - 1136.36).abs() < 0.01);
    }

    #[test]
    fn test_tds_calculation_below_25c() {
        // 20℃での計算（温度補償あり）
        let result = calculate_tds_ppm(2.5, 20.0, 500.0);
        // temp_coefficient = 1.0 + 0.02 * (20 - 25) = 0.9
        // TDS = (2.5 * 500) / 0.9 ≈ 1388.89
        assert!((result - 1388.89).abs() < 0.01);
    }

    #[test]
    fn test_tds_zero_voltage() {
        let result = calculate_tds_ppm(0.0, 25.0, 500.0);
        assert_eq!(result, 0.0);
    }
}
