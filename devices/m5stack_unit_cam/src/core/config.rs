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

    /// カメラウォームアップフレーム数
    pub camera_warmup_frames: Option<u8>,

    /// タイムゾーン
    pub timezone: String,
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

        // タイムゾーンを取得
        let timezone = config.timezone.to_string();

        Ok(AppConfig {
            receiver_mac,
            sleep_duration_seconds,
            frame_size,
            auto_exposure_enabled,
            camera_warmup_frames,
            timezone,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mac_address_parsing() {
        let mac = MacAddress::from_str("00:11:22:33:44:55").unwrap();
        assert_eq!(mac.to_string(), "00:11:22:33:44:55");
    }

    #[test]
    fn test_invalid_mac_address() {
        let result = MacAddress::from_str("invalid");
        assert!(result.is_err());
    }
}
