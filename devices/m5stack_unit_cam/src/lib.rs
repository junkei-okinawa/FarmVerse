/*!
 * # Image Sender Library
 *
 * ESP32カメラ画像を撮影して ESP-NOW プロトコルで送信するためのライブラリ
 *
 * このライブラリは以下の主要な機能を提供します：
 * - カメラ制御と画像キャプチャ
 * - ESP-NOW プロトコルを使った画像データの送信
 * - ステータスLED制御
 * - 設定管理
 * - MACアドレス処理
 * - ディープスリープ制御
 */

// 公開モジュール
pub mod camera;
pub mod config;
pub mod esp_now;
pub mod led;
pub mod mac_address;
pub mod sleep;

// 内部で使用する型をまとめてエクスポート
pub use camera::CameraController;
pub use config::{AppConfig, ConfigError};
pub use esp_now::{EspNowError, EspNowSender, ImageFrame, SendResult};
pub use led::status_led::{LedError, StatusLed};
pub use mac_address::MacAddress;
pub use sleep::{DeepSleep, DeepSleepError};

/// ライブラリのバージョン情報
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    /// テスト用のモジュール
    ///
    /// インテグレーションテストはここに追加します。
    /// 個別のモジュールのテストは各モジュールファイル内で行います。

    #[test]
    fn it_works() {
        // 基本的なテスト
        assert_eq!(2 + 2, 4);
    }
}
