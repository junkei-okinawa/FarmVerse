use crate::mac_address::MacAddress;
use crate::core::config_validation::{
    parse_camera_warmup_frames, parse_receiver_mac, ValidationError,
};
use crate::core::clamp_wifi_tx_power_dbm;

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

    #[default(255)]
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

    #[default(10)] // デフォルト10秒
    sleep_command_timeout_seconds: u64,

    // ADC電圧測定設定
    #[default(128)] // UnitCam GPIO0 の実測値に合わせて調整
    adc_voltage_min_mv: u16,

    #[default(3130)] // UnitCam GPIO0 の実測値に合わせて調整
    adc_voltage_max_mv: u16,

    // ESP-NOW 画像送信設定
    #[default(250)] // チャンクサイズ（バイト）
    esp_now_chunk_size: u16,

    #[default(50)] // チャンク間遅延（ミリ秒）
    esp_now_chunk_delay_ms: u32,

    // テスト・デバッグ設定
    #[default(false)]
    force_voltage_percent_50: bool,

    #[default(false)]
    force_camera_test: bool,

    #[default(false)]
    bypass_voltage_threshold: bool,

    #[default(false)]
    debug_mode: bool,

    // WiFi送信パワー設定（dBm）
    #[default(8)]
    wifi_tx_power_dbm: i8,
}

/// 設定エラー
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("無効な受信機MACアドレス: {0}")]
    InvalidReceiverMac(String),
    #[error("camera_warmup_frames の値が無効です (0-10): {0}")]
    InvalidCameraWarmupFrames(u8),
}

/// アプリケーション設定を表す構造体
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// 受信機のMACアドレス
    pub receiver_mac: MacAddress,

    /// ディープスリープ時間（秒）
    pub sleep_duration_seconds: u64,

    /// フレームサイズ
    pub frame_size: String,

    /// 自動露出設定
    pub auto_exposure_enabled: bool,

    /// SCCB経由のソフトスタンバイ制御を有効化
    pub camera_soft_standby_enabled: bool,

    /// カメラウォームアップフレーム数
    pub camera_warmup_frames: Option<u8>,

    /// タイムゾーン
    pub timezone: String,

    /// スリープコマンド待機タイムアウト（秒）
    pub sleep_command_timeout_seconds: u64,

    /// ADC電圧測定最小値（mV）
    pub adc_voltage_min_mv: u16,

    /// ADC電圧測定最大値（mV）
    pub adc_voltage_max_mv: u16,

    /// ESP-NOW画像送信チャンクサイズ（バイト）
    pub esp_now_chunk_size: u16,

    /// ESP-NOWチャンク間遅延時間（ミリ秒）
    pub esp_now_chunk_delay_ms: u32,

    /// 電圧チェックを無視してカメラテストを強制実行
    pub force_camera_test: bool,

    /// 電圧パーセンテージを強制的に50%として扱う（デバッグ用）
    pub force_voltage_percent_50: bool,

    /// 電圧閾値を無視して実画像送信を行う
    pub bypass_voltage_threshold: bool,

    /// デバッグモード（詳細ログ）
    pub debug_mode: bool,

    /// WiFi送信パワー（dBm, 2-20 にクランプ）
    pub wifi_tx_power_dbm: i8,
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

        // フレームサイズを設定
        let frame_size = config.frame_size.to_string();

        // 自動露出設定を取得
        let auto_exposure_enabled = config.auto_exposure_enabled;
        let camera_soft_standby_enabled = config.camera_soft_standby_enabled;

        // カメラウォームアップフレーム数を取得・検証
        let camera_warmup_frames =
            parse_camera_warmup_frames(config.camera_warmup_frames).map_err(map_validation_error)?;

        // タイムゾーンを取得
        let timezone = config.timezone.to_string();

        // スリープコマンドタイムアウトを取得
        let sleep_command_timeout_seconds = config.sleep_command_timeout_seconds;

        // ADC電圧測定設定を取得
        let adc_voltage_min_mv = config.adc_voltage_min_mv;
        let adc_voltage_max_mv = config.adc_voltage_max_mv;

        // ESP-NOW 画像送信設定を取得
        let esp_now_chunk_size = config.esp_now_chunk_size;
        let esp_now_chunk_delay_ms = config.esp_now_chunk_delay_ms;

        // テスト・デバッグ設定
        let force_voltage_percent_50 = config.force_voltage_percent_50;
        let force_camera_test = config.force_camera_test;
        let bypass_voltage_threshold = config.bypass_voltage_threshold;
        let debug_mode = config.debug_mode;

        // WiFi送信パワー（安全範囲へクランプ）
        let wifi_tx_power_dbm = clamp_wifi_tx_power_dbm(config.wifi_tx_power_dbm);

        Ok(AppConfig {
            receiver_mac,
            sleep_duration_seconds,
            frame_size,
            auto_exposure_enabled,
            camera_soft_standby_enabled,
            camera_warmup_frames,
            timezone,
            sleep_command_timeout_seconds,
            adc_voltage_min_mv,
            adc_voltage_max_mv,
            esp_now_chunk_size,
            esp_now_chunk_delay_ms,
            force_voltage_percent_50,
            force_camera_test,
            bypass_voltage_threshold,
            debug_mode,
            wifi_tx_power_dbm,
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
        ValidationError::InvalidTargetMinuteLastDigit(_)
        | ValidationError::InvalidTargetSecondLastDigit(_)
        | ValidationError::MissingWifiSsid => {
            unreachable!("core/config では target digits / wifi_ssid の検証は呼び出さない")
        }
    }
}
