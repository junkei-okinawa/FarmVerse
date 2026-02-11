/// コアシステムモジュール
pub mod app_controller;
pub mod config;
pub mod data_service;
pub mod domain_logic;
pub mod rtc_manager;

pub use app_controller::AppController;
pub use config::{AppConfig, ConfigError};
pub use data_service::{DataService, MeasuredData};
pub use domain_logic::{resolve_sleep_duration_seconds, voltage_to_percentage};
pub use rtc_manager::RtcManager;
