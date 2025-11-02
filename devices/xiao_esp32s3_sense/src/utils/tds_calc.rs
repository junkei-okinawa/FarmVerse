/// TDS（総溶解固形分）計算ユーティリティ
/// ハードウェア非依存の純粋関数を提供

/// EC値（μS/cm）からTDS濃度（ppm）を計算
///
/// # Arguments
/// - `ec_us_cm`: EC値（マイクロジーメンス/センチメートル）
/// - `tds_factor`: TDS変換係数（通常400-700、デフォルト500）
///
/// # Returns
/// - TDS濃度（ppm）
///
/// # Examples
/// ```no_run
/// use sensor_data_sender::utils::tds_calc::calculate_tds_from_ec;
///
/// let tds = calculate_tds_from_ec(1000.0, 500.0);
/// assert_eq!(tds, 500.0); // EC 1000μS/cm × 0.5 = 500ppm
/// ```
pub fn calculate_tds_from_ec(ec_us_cm: f32, tds_factor: f32) -> f32 {
    if ec_us_cm < 0.0 || tds_factor <= 0.0 {
        return 0.0;
    }
    
    // TDS (ppm) = EC (μS/cm) × TDS Factor / 1000
    ec_us_cm * tds_factor / 1000.0
}

/// 温度補正されたEC値を計算
///
/// # Arguments
/// - `ec_raw`: 生EC値（μS/cm）
/// - `temperature_celsius`: 測定時の温度（℃）
/// - `reference_temp`: 基準温度（℃、通常25℃）
/// - `temp_coefficient`: 温度補正係数（通常0.02 = 2%/℃）
///
/// # Returns
/// - 25℃換算のEC値（μS/cm）
///
/// # Examples
/// ```no_run
/// use sensor_data_sender::utils::tds_calc::compensate_ec_temperature;
///
/// // 30℃で測定したEC 1100μS/cmを25℃換算
/// let ec_compensated = compensate_ec_temperature(1100.0, 30.0, 25.0, 0.02);
/// assert!((ec_compensated - 1000.0).abs() < 1.0);
/// ```
pub fn compensate_ec_temperature(
    ec_raw: f32,
    temperature_celsius: f32,
    reference_temp: f32,
    temp_coefficient: f32,
) -> f32 {
    if ec_raw < 0.0 {
        return 0.0;
    }
    
    // EC_25℃ = EC_raw / (1 + coefficient × (T - 25))
    let temp_diff = temperature_celsius - reference_temp;
    let compensation_factor = 1.0 + (temp_coefficient * temp_diff);
    
    if compensation_factor <= 0.0 {
        return ec_raw;
    }
    
    ec_raw / compensation_factor
}

/// ADC生値からEC値を計算（線形補正）
///
/// # Arguments
/// - `adc_value`: ADC生値（0-4095）
/// - `calibrate_adc`: 校正時のADC値
/// - `calibrate_ec`: 校正時のEC値（μS/cm）
///
/// # Returns
/// - EC値（μS/cm）
///
/// # Examples
/// ```no_run
/// use sensor_data_sender::utils::tds_calc::calculate_ec_from_adc;
///
/// // 校正: ADC 1500 = 1413 μS/cm
/// // 測定: ADC 2000 のEC値を計算
/// let ec = calculate_ec_from_adc(2000, 1500, 1413.0);
/// assert!((ec - 1884.0).abs() < 1.0);
/// ```
pub fn calculate_ec_from_adc(adc_value: u16, calibrate_adc: u16, calibrate_ec: f32) -> f32 {
    if calibrate_adc == 0 || calibrate_ec < 0.0 {
        return 0.0;
    }
    
    // 線形補正: EC = (ADC値 / 校正ADC) × 校正EC
    (adc_value as f32 / calibrate_adc as f32) * calibrate_ec
}

#[cfg(test)]
mod tests {
    use super::*;

    // calculate_tds_from_ec のテスト
    
    #[test]
    fn test_tds_from_ec_standard() {
        let tds = calculate_tds_from_ec(1000.0, 500.0);
        assert_eq!(tds, 500.0);
    }

    #[test]
    fn test_tds_from_ec_zero() {
        let tds = calculate_tds_from_ec(0.0, 500.0);
        assert_eq!(tds, 0.0);
    }

    #[test]
    fn test_tds_from_ec_high_factor() {
        let tds = calculate_tds_from_ec(1000.0, 700.0);
        assert_eq!(tds, 700.0);
    }

    #[test]
    fn test_tds_from_ec_low_factor() {
        let tds = calculate_tds_from_ec(1000.0, 400.0);
        assert_eq!(tds, 400.0);
    }

    #[test]
    fn test_tds_from_ec_negative_ec() {
        let tds = calculate_tds_from_ec(-100.0, 500.0);
        assert_eq!(tds, 0.0);
    }

    #[test]
    fn test_tds_from_ec_zero_factor() {
        let tds = calculate_tds_from_ec(1000.0, 0.0);
        assert_eq!(tds, 0.0);
    }

    #[test]
    fn test_tds_from_ec_negative_factor() {
        let tds = calculate_tds_from_ec(1000.0, -500.0);
        assert_eq!(tds, 0.0);
    }

    #[test]
    fn test_tds_from_ec_realistic_values() {
        // 水道水レベル: EC 200μS/cm
        let tds = calculate_tds_from_ec(200.0, 500.0);
        assert_eq!(tds, 100.0);
        
        // 養液レベル: EC 2000μS/cm
        let tds = calculate_tds_from_ec(2000.0, 500.0);
        assert_eq!(tds, 1000.0);
    }

    // compensate_ec_temperature のテスト

    #[test]
    fn test_ec_temp_compensation_same_temp() {
        let ec = compensate_ec_temperature(1000.0, 25.0, 25.0, 0.02);
        assert_eq!(ec, 1000.0);
    }

    #[test]
    fn test_ec_temp_compensation_higher_temp() {
        // 30℃で1100 μS/cm → 25℃換算で約1000 μS/cm
        let ec = compensate_ec_temperature(1100.0, 30.0, 25.0, 0.02);
        assert!((ec - 1000.0).abs() < 1.0);
    }

    #[test]
    fn test_ec_temp_compensation_lower_temp() {
        // 20℃で900 μS/cm → 25℃換算で約1000 μS/cm
        let ec = compensate_ec_temperature(900.0, 20.0, 25.0, 0.02);
        assert!((ec - 1000.0).abs() < 1.0);
    }

    #[test]
    fn test_ec_temp_compensation_negative_ec() {
        let ec = compensate_ec_temperature(-100.0, 25.0, 25.0, 0.02);
        assert_eq!(ec, 0.0);
    }

    #[test]
    fn test_ec_temp_compensation_extreme_temp() {
        // 極端な温度でも動作確認
        let ec = compensate_ec_temperature(1000.0, 50.0, 25.0, 0.02);
        assert!(ec > 0.0);
        assert!(ec < 1000.0);
    }

    #[test]
    fn test_ec_temp_compensation_zero_coefficient() {
        // 温度補正係数0の場合、補正なし
        let ec = compensate_ec_temperature(1000.0, 30.0, 25.0, 0.0);
        assert_eq!(ec, 1000.0);
    }

    // calculate_ec_from_adc のテスト

    #[test]
    fn test_ec_from_adc_exact_calibration() {
        let ec = calculate_ec_from_adc(1500, 1500, 1413.0);
        assert_eq!(ec, 1413.0);
    }

    #[test]
    fn test_ec_from_adc_higher_value() {
        let ec = calculate_ec_from_adc(2000, 1500, 1413.0);
        assert!((ec - 1884.0).abs() < 1.0);
    }

    #[test]
    fn test_ec_from_adc_lower_value() {
        let ec = calculate_ec_from_adc(1000, 1500, 1413.0);
        assert!((ec - 942.0).abs() < 1.0);
    }

    #[test]
    fn test_ec_from_adc_zero_adc() {
        let ec = calculate_ec_from_adc(0, 1500, 1413.0);
        assert_eq!(ec, 0.0);
    }

    #[test]
    fn test_ec_from_adc_zero_calibrate_adc() {
        let ec = calculate_ec_from_adc(1000, 0, 1413.0);
        assert_eq!(ec, 0.0);
    }

    #[test]
    fn test_ec_from_adc_negative_calibrate_ec() {
        let ec = calculate_ec_from_adc(1500, 1500, -1000.0);
        assert_eq!(ec, 0.0);
    }

    #[test]
    fn test_ec_from_adc_realistic_range() {
        // ADC 500-3000, 校正点 1500=1413μS/cm
        let ec_low = calculate_ec_from_adc(500, 1500, 1413.0);
        let ec_mid = calculate_ec_from_adc(1500, 1500, 1413.0);
        let ec_high = calculate_ec_from_adc(3000, 1500, 1413.0);
        
        assert!(ec_low < ec_mid);
        assert!(ec_mid < ec_high);
        assert!(ec_low > 0.0);
        assert!(ec_high < 5000.0);
    }

    // 統合テスト

    #[test]
    fn test_full_tds_calculation_pipeline() {
        // ADC値からTDS ppmまでの完全な計算フロー
        
        // 1. ADCからEC計算
        let adc_value = 2000;
        let ec_raw = calculate_ec_from_adc(adc_value, 1500, 1413.0);
        
        // 2. 温度補正
        let ec_compensated = compensate_ec_temperature(ec_raw, 30.0, 25.0, 0.02);
        
        // 3. TDS計算
        let tds = calculate_tds_from_ec(ec_compensated, 500.0);
        
        assert!(tds > 0.0);
        assert!(tds < 2000.0); // 妥当な範囲
    }

    #[test]
    fn test_boundary_adc_max() {
        // ADC最大値（12bit = 4095）
        let ec = calculate_ec_from_adc(4095, 1500, 1413.0);
        assert!(ec > 0.0);
        assert!(ec < 10000.0);
    }
}
