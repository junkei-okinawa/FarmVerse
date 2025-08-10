use crate::mac_address::MacAddress;

/// アプリケーション設定
///
/// この構造体はビルド時に`build.rs`によって`cfg.toml`ファイルから
/// 読み込まれた設定を保持します。
#[toml_cfg::toml_config]
pub struct Config {
    #[default("11:22:33:44:55:66")]
    receiver_mac: &'static str,

    #[default(60)]
    sleep_duration_seconds: u64,

    #[default(3600)] // Default to 30 minutes
    sleep_duration_seconds_for_medium: u64,

    #[default(3600)]
    sleep_duration_seconds_for_long: u64,

    #[default(0)] // デフォルトは補正なし
    sleep_compensation_micros: i64,

    #[default("SVGA")]
    frame_size: &'static str,

    #[default(false)]
    auto_exposure_enabled: bool,

    #[default(0)]
    camera_warmup_frames: u8,

    #[default(255)]
    target_minute_last_digit: u8,

    #[default(255)]
    target_second_last_digit: u8,

    #[default("")]
    wifi_ssid: &'static str,

    #[default("")]
    wifi_password: &'static str,

    #[default("Asia/Tokyo")] // Default to Tokyo timezone
    timezone: &'static str,

    #[default(30)] // 30 seconds timeout
    sleep_command_timeout_seconds: u64,

    #[default(240)]
    esp_now_chunk_size: u16,

    #[default(10)]
    esp_now_chunk_delay_ms: u16,

    #[default(3300)]
    adc_voltage_min_mv: u16,

    #[default(4200)]
    adc_voltage_max_mv: u16,
}

/// 設定エラー
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("無効な受信機MACアドレス: {0}")]
    InvalidReceiverMac(String),
    #[error("camera_warmup_frames の値が無効です (0-10): {0}")]
    InvalidCameraWarmupFrames(u8),
    #[error("target_minute_last_digit の値が無効です (0-9): {0}")]
    InvalidTargetMinuteLastDigit(u8),
    #[error("target_second_last_digit の値が無効です (0-5): {0}")]
    InvalidTargetSecondLastDigit(u8),
    #[error("WiFi SSIDが設定されていません")]
    MissingWifiSsid,
    #[error("WiFi パスワードが設定されていません")]
    MissingWifiPassword,
}

/// 目標時刻設定
#[derive(Debug, Clone, Copy)] // Added Copy
#[allow(dead_code)] // 将来的に時刻制御機能で使用予定
pub struct TargetDigitsConfig {
    pub minute_last_digit: Option<u8>, // Changed to Option<u8>
    pub second_tens_digit: Option<u8>, // Changed to Option<u8>
}

/// アプリケーション設定を表す構造体
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// 受信機のMACアドレス
    pub receiver_mac: MacAddress,

    /// ディープスリープ時間（秒）
    pub sleep_duration_seconds: u64,

    /// 日の出までの調整スリープ時間（秒）
    pub sleep_duration_seconds_for_medium: u64,

    /// ディープスリープ時間（長時間用、秒）
    pub sleep_duration_seconds_for_long: u64,

    /// フレームサイズ
    pub frame_size: String,

    /// 自動露出設定
    pub auto_exposure_enabled: bool,

    /// カメラウォームアップフレーム数
    pub camera_warmup_frames: Option<u8>,

    /// 目標時刻設定 (分と秒の組み合わせ)
    pub target_digits_config: Option<TargetDigitsConfig>, // Added

    /// WiFi SSID
    pub wifi_ssid: String,

    /// WiFi パスワード
    pub wifi_password: String,

    /// タイムゾーン
    pub timezone: String,

    /// スリープ時間補正値 (マイクロ秒)
    pub sleep_compensation_micros: i64,

    /// スリープコマンドタイムアウト時間（秒）
    pub sleep_command_timeout_seconds: u64,

    /// ESP-NOWチャンクサイズ
    pub esp_now_chunk_size: u16,

    /// ESP-NOWチャンク間遅延（ミリ秒）
    pub esp_now_chunk_delay_ms: u16,

    /// ADC電圧最小値（ミリボルト）
    pub adc_voltage_min_mv: u16,

    /// ADC電圧最大値（ミリボルト）
    pub adc_voltage_max_mv: u16,
}

/// メモリ管理設定
#[derive(Debug, Clone)]
#[allow(dead_code)] // Issue #12 ストリーミング機能実装で使用予定
pub struct MemoryConfig {
    /// 最大チャンクサイズ（バイト）
    pub max_chunk_size: usize,
    /// 送信バッファサイズ（バイト）
    pub send_buffer_size: usize,
    /// 受信バッファサイズ（バイト）
    pub receive_buffer_size: usize,
    /// 同時処理可能な最大ストリーム数
    pub max_concurrent_streams: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_chunk_size: 240,
            send_buffer_size: 4096,
            receive_buffer_size: 8192,
            max_concurrent_streams: 3,
        }
    }
}

impl MemoryConfig {
    /// 新しいMemoryConfigインスタンスを作成
    pub fn new() -> Self {
        Self::default()
    }

    /// PSRAMが有効かどうか
    pub fn is_psram_enabled(&self) -> bool {
        true // ESP32S3はPSRAMサポート
    }

    /// 利用可能なヒープサイズを取得
    pub fn get_available_heap_size(&self) -> usize {
        8 * 1024 * 1024 // 8MB PSRAM
    }

    /// カメラバッファサイズを取得
    pub fn get_camera_buffer_size(&self) -> usize {
        300 * 1024 // 300KB for UXGA
    }
}

impl AppConfig {
    /// 設定ファイルから設定をロードします
    pub fn load() -> Result<Self, ConfigError> {
        // toml_cfg によって生成された定数
        let config = CONFIG;

        // 受信機のMACアドレスをパース
        let receiver_mac_str = config.receiver_mac;
        if receiver_mac_str == "11:22:33:44:55:66" || receiver_mac_str == "" {
            // デフォルト値または空文字の場合はエラー
            return Err(ConfigError::InvalidReceiverMac(
                "受信機MACアドレスが設定されていません。cfg.tomlを確認してください。".to_string(),
            ));
        }
        let receiver_mac = MacAddress::from_str(receiver_mac_str)
            .map_err(|_| ConfigError::InvalidReceiverMac(receiver_mac_str.to_string()))?;

        // ディープスリープ時間を設定
        let sleep_duration_seconds = config.sleep_duration_seconds;
        let sleep_duration_seconds_for_medium = config.sleep_duration_seconds_for_medium;
        let sleep_duration_seconds_for_long = config.sleep_duration_seconds_for_long;

        // フレームサイズを設定
        let frame_size = config.frame_size.to_string();

        // 自動露出設定を取得
        let auto_exposure_enabled = config.auto_exposure_enabled;

        // カメラウォームアップフレーム数を取得・検証
        let camera_warmup_frames_val = config.camera_warmup_frames;
        if !(camera_warmup_frames_val <= 10 || camera_warmup_frames_val == 255) {
            return Err(ConfigError::InvalidCameraWarmupFrames(
                camera_warmup_frames_val,
            ));
        }
        let camera_warmup_frames = if camera_warmup_frames_val == 255 {
            None
        } else {
            Some(camera_warmup_frames_val)
        };

        // 目標時刻設定を処理
        let minute_config_val = config.target_minute_last_digit;
        let second_tens_config_val = config.target_second_last_digit; // This is for the tens digit of the second

        let target_minute_opt = if minute_config_val <= 9 {
            Some(minute_config_val)
        } else if minute_config_val == 255 {
            None
        } else {
            return Err(ConfigError::InvalidTargetMinuteLastDigit(
                minute_config_val,
            ));
        };

        let target_second_opt = if second_tens_config_val <= 5 {
            Some(second_tens_config_val)
        } else if second_tens_config_val == 255 {
            None
        } else {
            return Err(ConfigError::InvalidTargetSecondLastDigit(
                second_tens_config_val,
            ));
        };

        let target_digits_config = if target_minute_opt.is_some() || target_second_opt.is_some() {
            Some(TargetDigitsConfig {
                minute_last_digit: target_minute_opt,
                second_tens_digit: target_second_opt,
            })
        } else {
            None
        };

        // WiFi設定を取得
        let wifi_ssid = config.wifi_ssid.to_string();
        if wifi_ssid.is_empty() {
            return Err(ConfigError::MissingWifiSsid);
        }
        let wifi_password = config.wifi_password.to_string();
        // Password can be empty for open networks, so no check for emptiness here.

        // タイムゾーンを取得
        let timezone = config.timezone.to_string();

        // スリープ時間補正値を取得
        let sleep_compensation_micros = config.sleep_compensation_micros;

        // その他の設定値を取得
        let sleep_command_timeout_seconds = config.sleep_command_timeout_seconds;
        let esp_now_chunk_size = config.esp_now_chunk_size;
        let esp_now_chunk_delay_ms = config.esp_now_chunk_delay_ms;
        let adc_voltage_min_mv = config.adc_voltage_min_mv;
        let adc_voltage_max_mv = config.adc_voltage_max_mv;

        Ok(AppConfig {
            receiver_mac,
            sleep_duration_seconds,
            sleep_duration_seconds_for_medium,
            sleep_duration_seconds_for_long,
            frame_size,
            auto_exposure_enabled,
            camera_warmup_frames,
            target_digits_config,
            wifi_ssid,
            wifi_password,
            timezone,
            sleep_compensation_micros,
            sleep_command_timeout_seconds,
            esp_now_chunk_size,
            esp_now_chunk_delay_ms,
            adc_voltage_min_mv,
            adc_voltage_max_mv,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // AppConfig のフィールドをすべて含むようにシミュレーション関数を更新
    // 注意: このテストは toml_cfg によって生成される CONFIG 定数を直接モックできないため、
    // AppConfig::load() のロジックを部分的に再現する形になります。
    // 実際の toml_cfg の動作確認はビルドと実行を通じて行われます。
    fn simulate_app_config_creation(
        receiver_mac_str: &str,
        sleep_duration: u64,
        sleep_duration_medium: u64,
        sleep_duration_for_long: u64,
        frame_size_str: &str,
        auto_exposure: bool,
        warmup_frames_val: u8,
        minute_digit_val: u8, // Corresponds to CONFIG.target_minute_last_digit
        second_tens_digit_val: u8, // Corresponds to CONFIG.target_second_last_digit
        wifi_ssid_str: &str,
        wifi_password_str: &str,
        timezone_str: &str,
    ) -> Result<Box<AppConfig>, ConfigError> {
        let mac = MacAddress::from_str(receiver_mac_str)
            .map_err(|_| ConfigError::InvalidReceiverMac(receiver_mac_str.to_string()))?;

        let cam_warmup = if warmup_frames_val == 255 {
            None
        } else if warmup_frames_val <= 10 {
            Some(warmup_frames_val)
        } else {
            return Err(ConfigError::InvalidCameraWarmupFrames(warmup_frames_val));
        };

        let minute_opt = if minute_digit_val <= 9 {
            Some(minute_digit_val)
        } else if minute_digit_val == 255 {
            None
        } else {
            return Err(ConfigError::InvalidTargetMinuteLastDigit(minute_digit_val));
        };

        let second_opt = if second_tens_digit_val <= 5 {
            Some(second_tens_digit_val)
        } else if second_tens_digit_val == 255 {
            None
        } else {
            return Err(ConfigError::InvalidTargetSecondLastDigit(
                second_tens_digit_val,
            ));
        };

        let target_digits_conf = if minute_opt.is_some() || second_opt.is_some() {
            Some(TargetDigitsConfig {
                minute_last_digit: minute_opt,
                second_tens_digit: second_opt,
            })
        } else {
            None
        };

        if wifi_ssid_str.is_empty() {
            return Err(ConfigError::MissingWifiSsid);
        }

        Ok(Box::new(AppConfig {
            receiver_mac: mac,
            sleep_duration_seconds: sleep_duration,
            sleep_duration_seconds_for_medium: sleep_duration_medium,
            sleep_duration_seconds_for_long: sleep_duration_for_long,
            frame_size: frame_size_str.to_string(),
            auto_exposure_enabled: auto_exposure,
            camera_warmup_frames: cam_warmup,
            target_digits_config: target_digits_conf,
            wifi_ssid: wifi_ssid_str.to_string(),
            wifi_password: wifi_password_str.to_string(),
            timezone: timezone_str.to_string(),
            sleep_compensation_micros: 0, // Default compensation micros
            sleep_command_timeout_seconds: 30, // Default timeout
            esp_now_chunk_size: 240, // Default chunk size
            esp_now_chunk_delay_ms: 10, // Default delay
            adc_voltage_min_mv: 3300, // Default min voltage
            adc_voltage_max_mv: 4200, // Default max voltage
        }))
    }

    #[test]
    fn test_config_valid_values() {
        let config = simulate_app_config_creation(
            "00:11:22:33:44:55",
            30,
            900,
            1800,
            "QVGA",
            true,
            5,  // warmup_frames
            1,  // minute_digit
            2,  // second_tens_digit
            "test_ssid",
            "test_password",
            "Europe/London",
        )
        .unwrap();
        assert_eq!(config.receiver_mac.to_string(), "00:11:22:33:44:55");
        assert_eq!(config.sleep_duration_seconds, 30);
        assert_eq!(config.sleep_duration_seconds_for_medium, 900);
        assert_eq!(config.sleep_duration_seconds_for_long, 1800);
        assert_eq!(config.frame_size, "QVGA");
        assert!(config.auto_exposure_enabled);
        assert_eq!(config.camera_warmup_frames, Some(5));
        assert_eq!(
            config.target_digits_config.unwrap().minute_last_digit,
            Some(1)
        );
        assert_eq!(
            config.target_digits_config.unwrap().second_tens_digit,
            Some(2)
        );
        assert_eq!(config.wifi_ssid, "test_ssid");
        assert_eq!(config.wifi_password, "test_password");
        assert_eq!(config.timezone, "Europe/London");
    }

    #[test]
    fn test_config_values_as_none() {
        // Test with 255 for optional values to signify None
        let config = simulate_app_config_creation(
            "AA:BB:CC:DD:EE:FF",
            60,
            1200,
            3600,
            "SVGA",
            false,
            255, // camera_warmup_frames -> None
            255, // target_minute_last_digit -> None
            255, // target_second_last_digit -> None
            "another_ssid",
            "", // Empty password
            "UTC",
        )
        .unwrap();
        assert_eq!(config.camera_warmup_frames, None);
        assert!(config.target_digits_config.is_none());
        assert_eq!(config.wifi_password, "");
        assert_eq!(config.timezone, "UTC");
    }

    #[test]
    fn test_config_invalid_camera_warmup_frames() {
        let result = simulate_app_config_creation(
            "00:11:22:33:44:55",
            30,
            900,
            1800,
            "QVGA",
            true,
            11, // Invalid warmup_frames
            1,
            2,
            "ssid",
            "pass",
            "Asia/Tokyo",
        );
        assert!(matches!(
            result,
            Err(ConfigError::InvalidCameraWarmupFrames(11))
        ));
    }

    #[test]
    fn test_config_invalid_target_minute_digit() {
        let result = simulate_app_config_creation(
            "00:11:22:33:44:55",
            30,
            900,
            1800,
            "QVGA",
            true,
            5,
            10, // Invalid minute_digit
            2,
            "ssid",
            "pass",
            "Asia/Tokyo",
        );
        assert!(matches!(
            result,
            Err(ConfigError::InvalidTargetMinuteLastDigit(10))
        ));
    }

    #[test]
    fn test_config_invalid_target_second_digit() {
        let result = simulate_app_config_creation(
            "00:11:22:33:44:55",
            30,
            900,
            1800,
            "QVGA",
            true,
            5,
            1,
            6, // Invalid second_digit
            "ssid",
            "pass",
            "Asia/Tokyo",
        );
        assert!(matches!(
            result,
            Err(ConfigError::InvalidTargetSecondLastDigit(6))
        ));
    }

    // Test for default MAC address (which is now an error)
    #[test]
    fn test_original_invalid_mac_address() {
        // This test now expects an error because the default MAC is invalid
        let result = AppConfig::load(); // Simulates loading with default/empty MAC
                                        // We need to ensure build.rs doesn't set a valid one for this test,
                                        // or mock CONFIG.receiver_mac to be the default.
                                        // For now, this test is hard to make perfect without deeper mocking.
                                        // Assuming `CONFIG.receiver_mac` could be "11:22:33:44:55:66"
                                        // or "" from a fresh cfg.toml.
                                        // This test will likely fail if cfg.toml has a valid MAC.
                                        // A more robust test would involve mocking `CONFIG`.
                                        // However, based on current AppConfig::load logic:
        if CONFIG.receiver_mac == "11:22:33:44:55:66" || CONFIG.receiver_mac == "" {
            assert!(matches!(
                result,
                Err(ConfigError::InvalidReceiverMac(_))
            ));
        } else {
            // If cfg.toml has a valid MAC, this specific error won't be triggered.
            // The test's intent is to check the handling of the *default* placeholder.
            // Consider skipping or adjusting if this becomes flaky due to `cfg.toml` content.
            println!("Skipping default MAC test as cfg.toml likely has a valid MAC configured.");
        }
    }


    #[test]
    fn test_missing_wifi_ssid() {
        let result = simulate_app_config_creation(
            "00:11:22:33:44:55",
            30,
            900,
            1800,
            "QVGA",
            true,
            5,
            1,
            2,
            "", // Missing SSID
            "pass",
            "Asia/Tokyo",
        );
        assert!(matches!(result, Err(ConfigError::MissingWifiSsid)));
    }

    // Test for missing WiFi password (which is allowed for open networks)
    #[test]
    fn test_missing_wifi_password() {
         let config = simulate_app_config_creation(
            "00:11:22:33:44:55",
            30,
            900,
            1800,
            "QVGA",
            true,
            5,
            1,
            2,
            "open_network_ssid",
            "", // Empty password
            "Asia/Tokyo",
        )
        .unwrap();
        assert_eq!(config.wifi_password, "");
    }

    // Test specific case: only minute digit set
    #[test]
    fn test_only_minute_digit_set() {
        let config = simulate_app_config_creation(
            "00:11:22:33:44:55",
            60,
            1200,
            3600,
            "SVGA",
            false,
            255,
            7,   // minute_digit
            255, // second_tens_digit -> None
            "ssid",
            "pass",
            "Asia/Tokyo",
        )
        .unwrap();
        assert!(config.target_digits_config.is_some());
        let target_conf = config.target_digits_config.unwrap();
        assert_eq!(target_conf.minute_last_digit, Some(7));
        assert_eq!(target_conf.second_tens_digit, None);
    }

    // Test specific case: only second digit set
    #[test]
    fn test_only_second_digit_set() {
        let config = simulate_app_config_creation(
            "00:11:22:33:44:55",
            60,
            1200,
            3600,
            "SVGA",
            false,
            255,
            255, // minute_digit -> None
            3,   // second_tens_digit
            "ssid",
            "pass",
            "Asia/Tokyo",
        )
        .unwrap();
        assert!(config.target_digits_config.is_some());
        let target_conf = config.target_digits_config.unwrap();
        assert_eq!(target_conf.minute_last_digit, None);
        assert_eq!(target_conf.second_tens_digit, Some(3));
    }
}
