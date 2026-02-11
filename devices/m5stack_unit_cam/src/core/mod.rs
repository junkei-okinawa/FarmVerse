/// コアシステムモジュール
pub mod app_controller;
pub mod config;
pub mod config_validation;
pub mod data_service;
pub mod data_prep;
pub mod domain_logic;
pub mod rtc_manager;

pub use app_controller::AppController;
pub use config::{AppConfig, ConfigError};
pub use data_service::{DataService, MeasuredData};
pub use data_prep::{prepare_image_payload, simple_image_hash, DUMMY_HASH};
pub use domain_logic::{resolve_sleep_duration_seconds, voltage_to_percentage};
pub use rtc_manager::RtcManager;
