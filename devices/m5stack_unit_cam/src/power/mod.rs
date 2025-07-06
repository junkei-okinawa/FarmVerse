/// 電源管理モジュール
pub mod sleep;

pub use sleep::{DeepSleep, DeepSleepError, EspIdfDeepSleep};
