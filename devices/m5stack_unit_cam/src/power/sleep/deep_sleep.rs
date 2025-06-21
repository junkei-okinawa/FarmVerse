use crate::core::config::AppConfig;
use log::info;
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum DeepSleepError {
    #[error("Invalid sleep duration: {0}")]
    InvalidDuration(String),
}

/// Platform-agnostic deep-sleep abstraction.
pub trait DeepSleepPlatform {
    /// Enter deep sleep for the specified duration in microseconds.
    fn deep_sleep(&self, duration_us: u64);
}

/// ESP-IDF specific deep sleep implementation.
pub struct EspIdfDeepSleep;

impl DeepSleepPlatform for EspIdfDeepSleep {
    fn deep_sleep(&self, duration_us: u64) {
        info!("Entering deep sleep for {} microseconds", duration_us);
        unsafe {
            esp_idf_svc::sys::esp_deep_sleep(duration_us);
        }
    }
}

/// Deep sleep controller with platform abstraction.
pub struct DeepSleep<P: DeepSleepPlatform> {
    platform: P,
}

impl<P: DeepSleepPlatform> DeepSleep<P> {
    /// Create a new `DeepSleep` controller.
    pub fn new(_config: Arc<AppConfig>, platform: P) -> Self {
        DeepSleep { platform }
    }

    /// Sleep for a specified duration in seconds.
    pub fn sleep_for_duration(&self, duration_seconds: u64) -> Result<(), DeepSleepError> {
        if duration_seconds == 0 {
            return Err(DeepSleepError::InvalidDuration(
                "Sleep duration must be greater than 0".to_string(),
            ));
        }

        let duration_us = duration_seconds
            .checked_mul(1_000_000)
            .ok_or_else(|| DeepSleepError::InvalidDuration("Duration overflow".to_string()))?;

        info!("Sleeping for {} seconds ({} microseconds)", duration_seconds, duration_us);
        self.platform.deep_sleep(duration_us);
        Ok(())
    }
}
