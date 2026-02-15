/// コアシステムモジュール
pub mod app_controller;
pub mod capture_policy;
pub mod config;
pub mod config_validation;
pub mod data_service;
pub mod data_prep;
pub mod domain_logic;
pub mod rtc_manager;

pub use app_controller::AppController;
pub use capture_policy::{
    should_capture_image,
    should_capture_image_with_overrides,
    INVALID_VOLTAGE_PERCENT,
    LOW_VOLTAGE_THRESHOLD_PERCENT,
};
pub use config::{AppConfig, ConfigError};
pub use data_service::{DataService, MeasuredData};
pub use data_prep::{prepare_image_payload, simple_image_hash, DUMMY_HASH};
pub use domain_logic::{clamp_wifi_tx_power_dbm, resolve_sleep_duration_seconds, voltage_to_percentage};
pub use rtc_manager::RtcManager;
