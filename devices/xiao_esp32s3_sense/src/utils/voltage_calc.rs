/// 電圧計算ユーティリティ
/// ハードウェア非依存の純粋関数を提供

/// 電圧(mV)をパーセンテージに変換する
/// 
/// # Arguments
/// - `voltage_mv`: ADC測定電圧（ミリボルト）
/// - `min_mv`: 最小電圧（0%相当）
/// - `max_mv`: 最大電圧（100%相当）
/// 
/// # Returns
/// - 0-100: 電圧パーセンテージ
/// 
/// # Examples
/// ```
/// use sensor_data_sender::utils::voltage_calc::calculate_voltage_percentage;
/// 
/// let percent = calculate_voltage_percentage(1629.0, 128.0, 3130.0);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voltage_percentage_50_percent() {
        // 中間値: (1629 - 128) / (3130 - 128) ≈ 0.5
        let result = calculate_voltage_percentage(1629.0, 128.0, 3130.0);
        assert_eq!(result, 50);
    }

    #[test]
    fn test_voltage_percentage_0_percent() {
        let result = calculate_voltage_percentage(128.0, 128.0, 3130.0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_voltage_percentage_100_percent() {
        let result = calculate_voltage_percentage(3130.0, 128.0, 3130.0);
        assert_eq!(result, 100);
    }

    #[test]
    fn test_voltage_percentage_below_minimum() {
        let result = calculate_voltage_percentage(50.0, 128.0, 3130.0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_voltage_percentage_above_maximum() {
        let result = calculate_voltage_percentage(3500.0, 128.0, 3130.0);
        assert_eq!(result, 100);
    }

    #[test]
    fn test_voltage_percentage_invalid_range() {
        // max < min
        let result = calculate_voltage_percentage(1500.0, 3130.0, 128.0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_voltage_percentage_zero_range() {
        let result = calculate_voltage_percentage(1500.0, 1500.0, 1500.0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_voltage_percentage_realistic_2000mv() {
        // 2000mV (約62%)
        let result = calculate_voltage_percentage(2000.0, 128.0, 3130.0);
        assert_eq!(result, 62);
    }

    #[test]
    fn test_voltage_percentage_realistic_500mv() {
        // 500mV (約12%)
        let result = calculate_voltage_percentage(500.0, 128.0, 3130.0);
        assert_eq!(result, 12);
    }

    #[test]
    fn test_voltage_percentage_realistic_2500mv() {
        // 2500mV (約79%)
        let result = calculate_voltage_percentage(2500.0, 128.0, 3130.0);
        assert_eq!(result, 79);
    }
}
