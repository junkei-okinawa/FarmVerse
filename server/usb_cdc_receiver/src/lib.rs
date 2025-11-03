// リファクタリングされたモジュールをエクスポート

// ESP-NOW関連モジュール（ホストテストでも使用可能）
pub mod esp_now;
pub mod mac_address;

// コマンド解析（ホストテストでも使用可能）
#[cfg_attr(not(feature = "esp"), allow(dead_code))]
pub mod command;

// USB モジュール（常に公開 - Mock実装を含む）
pub mod usb;

// 以下のモジュールはESP-IDF依存のため、"esp"フィーチャー有効時のみコンパイル
#[cfg(feature = "esp")]
pub mod config;

#[cfg(feature = "esp")]
pub mod queue;

#[cfg(feature = "esp")]
pub mod streaming;

#[cfg(feature = "esp")]
pub mod sleep_command_queue;

// 必要に応じてユーティリティ関数もエクスポート
