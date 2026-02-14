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
pub mod core;
pub mod hardware;
pub mod mac_address;
pub mod power;

// 内部で使用する型をまとめてエクスポート
pub use communication::esp_now::{EspNowError, EspNowSender, EspNowReceiver};
pub use core::{AppConfig, ConfigError, DataService, MeasuredData};
pub use hardware::camera::CameraController;
pub use hardware::led::status_led::{LedError, StatusLed};
pub use hardware::{CameraPins, VoltageSensor};
pub use mac_address::MacAddress;
pub use power::{DeepSleep, DeepSleepError};

/// ライブラリのバージョン情報
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// libstart feature expects the root crate to provide a main symbol.
#[cfg(all(test, target_os = "none"))]
#[no_mangle]
pub extern "C" fn main() {}
