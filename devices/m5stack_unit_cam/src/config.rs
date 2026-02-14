use crate::mac_address::MacAddress;
use crate::core::config_validation::{
    parse_camera_warmup_frames, parse_receiver_mac, parse_target_minute_last_digit,
    parse_target_second_tens_digit, validate_wifi_ssid, ValidationError,
};

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

    #[default(false)]
    camera_soft_standby_enabled: bool,

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

    /// SCCB経由のソフトスタンバイ制御を有効化
    pub camera_soft_standby_enabled: bool,

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
}

impl AppConfig {
    /// 設定ファイルから設定をロードします
    pub fn load() -> Result<Self, ConfigError> {
        // toml_cfg によって生成された定数
        let config = CONFIG;

        // 受信機のMACアドレスをパース
        let receiver_mac = parse_receiver_mac(config.receiver_mac).map_err(map_validation_error)?;

        // ディープスリープ時間を設定
        let sleep_duration_seconds = config.sleep_duration_seconds;
        let sleep_duration_seconds_for_medium = config.sleep_duration_seconds_for_medium;
        let sleep_duration_seconds_for_long = config.sleep_duration_seconds_for_long;

        // フレームサイズを設定
        let frame_size = config.frame_size.to_string();

        // 自動露出設定を取得
        let auto_exposure_enabled = config.auto_exposure_enabled;
        let camera_soft_standby_enabled = config.camera_soft_standby_enabled;

        // カメラウォームアップフレーム数を取得・検証
        let camera_warmup_frames =
            parse_camera_warmup_frames(config.camera_warmup_frames).map_err(map_validation_error)?;

        // 目標時刻設定を処理
        let target_minute_opt =
            parse_target_minute_last_digit(config.target_minute_last_digit).map_err(map_validation_error)?;

        let target_second_opt =
            parse_target_second_tens_digit(config.target_second_last_digit).map_err(map_validation_error)?;

        let target_digits_config = if target_minute_opt.is_some() || target_second_opt.is_some() {
            Some(TargetDigitsConfig {
                minute_last_digit: target_minute_opt,
                second_tens_digit: target_second_opt,
            })
        } else {
            None
        };

        // WiFi設定を取得
        validate_wifi_ssid(config.wifi_ssid).map_err(map_validation_error)?;
        let wifi_ssid = config.wifi_ssid.to_string();
        let wifi_password = config.wifi_password.to_string();
        // Password can be empty for open networks, so no check for emptiness here.

        // タイムゾーンを取得
        let timezone = config.timezone.to_string();

        // スリープ時間補正値を取得
        let sleep_compensation_micros = config.sleep_compensation_micros;

        Ok(AppConfig {
            receiver_mac,
            sleep_duration_seconds,
            sleep_duration_seconds_for_medium,
            sleep_duration_seconds_for_long,
            frame_size,
            auto_exposure_enabled,
            camera_soft_standby_enabled,
            camera_warmup_frames,
            target_digits_config,
            wifi_ssid,
            wifi_password,
            timezone,
            sleep_compensation_micros,
        })
    }
}

fn map_validation_error(err: ValidationError) -> ConfigError {
    match err {
        ValidationError::MissingReceiverMac => ConfigError::InvalidReceiverMac(
            "受信機MACアドレスが設定されていません。cfg.tomlを確認してください。".to_string(),
        ),
        ValidationError::InvalidReceiverMac(v) => ConfigError::InvalidReceiverMac(v),
        ValidationError::InvalidCameraWarmupFrames(v) => ConfigError::InvalidCameraWarmupFrames(v),
        ValidationError::InvalidTargetMinuteLastDigit(v) => ConfigError::InvalidTargetMinuteLastDigit(v),
        ValidationError::InvalidTargetSecondLastDigit(v) => ConfigError::InvalidTargetSecondLastDigit(v),
        ValidationError::MissingWifiSsid => ConfigError::MissingWifiSsid,
    }
}
