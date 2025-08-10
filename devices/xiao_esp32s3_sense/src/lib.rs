/*!
 * # M5Stack Unit Cam Image Sender Library
 *
 * ESP32カメラ画像を撮影して ESP-NOW プロトコルで送信するためのライブラリ
 *
 * ## モジュール構成
 * - `core`: アプリケーションの核となる機能（設定、データサービス、制御）
 * - `hardware`: ハードウェア制御（カメラ、LED、電圧センサー、ピン設定）
 * - `communication`: 通信機能（ESP-NOW、ネットワーク管理）
 * - `power`: 電源管理（ディープスリープ）
 */

// 公開モジュール
pub mod communication;
pub mod config;
pub mod core;
pub mod hardware;
pub mod mac_address;
pub mod power;

// 内部で使用する型をまとめてエクスポート
pub use communication::esp_now::{EspNowError, EspNowSender, EspNowReceiver};
pub use config::{AppConfig, ConfigError, MemoryConfig};
pub use core::{DataService, MeasuredData};
pub use hardware::camera::CameraController;
pub use hardware::led::status_led::{LedError, StatusLed};
pub use hardware::{CameraPins, VoltageSensor};
pub use mac_address::MacAddress;

/// ライブラリのバージョン情報
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// XIAO ESP32S3 Sense対応テストモジュール（Issue #12）
#[cfg(test)]
pub mod tests;

#[cfg(test)]
mod integration_tests {
    /// インテグレーションテストモジュール
    ///
    /// 各モジュール間の連携テストをここで実行します。
    /// Issue #12の統合テストも含まれます。

    #[test]
    fn it_works() {
        // 基本的なテスト
        assert_eq!(2 + 2, 4);
    }
}
