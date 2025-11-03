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
#[cfg(not(test))]
pub mod communication;
#[cfg(not(test))]
pub mod config;
pub mod core;  // Always public to support integration testing
#[cfg(not(test))]
pub mod hardware;
pub mod mac_address;
#[cfg(not(test))]
pub mod power;
pub mod utils;

// 内部で使用する型をまとめてエクスポート
#[cfg(not(test))]
pub use communication::esp_now::{EspNowError, EspNowSender, EspNowReceiver};
#[cfg(not(test))]
pub use config::{AppConfig, ConfigError, MemoryConfig};
pub use core::{DataService, MeasuredData};  // Always public to support integration testing
#[cfg(not(test))]
pub use hardware::camera::CameraController;
#[cfg(not(test))]
pub use hardware::led::status_led::{LedError, StatusLed};
#[cfg(not(test))]
pub use hardware::{CameraPins, VoltageSensor};
pub use mac_address::MacAddress;
pub use utils::calculate_voltage_percentage;

/// ライブラリのバージョン情報
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
