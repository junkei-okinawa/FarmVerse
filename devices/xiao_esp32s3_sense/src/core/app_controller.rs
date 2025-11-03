use log::{error, info, warn};
use std::sync::Arc;

use crate::config::AppConfig;
use crate::communication::esp_now::EspNowReceiver;
use crate::power::sleep::{DeepSleep, DeepSleepPlatform};

/// アプリケーションの主要な制御フローを管理するモジュール
pub struct AppController;

impl AppController {
    /// スリープコマンドを受信してディープスリープを実行
    pub fn handle_sleep_with_server_command<P: DeepSleepPlatform>(
        esp_now_receiver: &EspNowReceiver,
        deep_sleep_controller: &DeepSleep<P>,
        config: &Arc<AppConfig>,
    ) -> anyhow::Result<()> {
        info!("=== サーバーからのスリープコマンド待機開始 ===");
        info!("設定されたデフォルトスリープ時間: {}秒", config.sleep_duration_seconds);
        info!("スリープコマンド待機タイムアウト: {}秒", config.sleep_command_timeout_seconds);
        
        // ESP-NOW受信状態をリセット（前回の受信データをクリア）
        EspNowReceiver::reset_receiver_state();
        
        match esp_now_receiver.wait_for_sleep_command(config.sleep_command_timeout_seconds as u32) {
            Some(duration_seconds) => {
                if duration_seconds > 0 {
                    info!(
                        "✓ サーバーからスリープ時間を受信: {}秒。ディープスリープに入ります。",
                        duration_seconds
                    );
                    deep_sleep_controller.sleep_for_duration(duration_seconds as u64)?;
                } else {
                    warn!("無効なスリープ時間 (0秒) を受信。デフォルト時間を使用します。");
                    info!("デフォルトスリープ時間でディープスリープに入ります: {}秒", config.sleep_duration_seconds);
                    deep_sleep_controller.sleep_for_duration(config.sleep_duration_seconds)?;
                }
            }
            None => {
                warn!("✗ スリープコマンドを受信できませんでした。デフォルト時間を使用します。");
                info!("デフォルトスリープ時間でディープスリープに入ります: {}秒", config.sleep_duration_seconds);
                deep_sleep_controller.sleep_for_duration(config.sleep_duration_seconds)?;
            }
        }
        
        Ok(())
    }

    /// エラー時のフォールバックスリープ
    pub fn fallback_sleep<P: DeepSleepPlatform>(
        deep_sleep_controller: &DeepSleep<P>,
        config: &Arc<AppConfig>,
        error_msg: &str,
    ) -> anyhow::Result<()> {
        error!("{}", error_msg);
        deep_sleep_controller.sleep_for_duration(config.sleep_duration_seconds)?;
        Ok(())
    }
}

#[cfg(all(test, not(target_os = "espidf")))]
mod tests {
    use super::*;
    use crate::power::sleep::DeepSleepPlatform;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct MockDeepSleepPlatform {
        sleep_calls: Arc<Mutex<Vec<u64>>>,
    }

    impl MockDeepSleepPlatform {
        fn new() -> Self {
            Self {
                sleep_calls: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn get_sleep_calls(&self) -> Vec<u64> {
            self.sleep_calls.lock().unwrap().clone()
        }
    }

    impl DeepSleepPlatform for MockDeepSleepPlatform {
        fn deep_sleep(&self, duration_us: u64) {
            self.sleep_calls.lock().unwrap().push(duration_us);
        }
    }

    fn create_test_config(sleep_duration: u64, timeout: u64) -> Arc<AppConfig> {
        Arc::new(AppConfig {
            receiver_mac: "AA:BB:CC:DD:EE:FF".to_string(),
            sleep_duration_seconds: sleep_duration,
            sleep_command_timeout_seconds: timeout,
            debug_mode: false,
            bypass_voltage_threshold: false,
            force_camera_test: false,
            temp_sensor_enabled: false,
            temp_sensor_power_pin: 0,
            temp_sensor_data_pin: 0,
            temperature_offset_celsius: 0.0,
            camera_warmup_frames: Some(0),
            timezone: "Asia/Tokyo".to_string(),
            ec_tds_sensor_enabled: false,
            ec_tds_sensor_pin: 0,
        })
    }

    #[test]
    fn test_fallback_sleep_success() {
        let config = create_test_config(300, 10);
        let mock_platform = MockDeepSleepPlatform::new();
        let deep_sleep = DeepSleep::new(config.clone(), mock_platform.clone());

        let result = AppController::fallback_sleep(
            &deep_sleep,
            &config,
            "Test error",
        );

        assert!(result.is_ok());
        let calls = mock_platform.get_sleep_calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], 300_000_000); // 300 seconds in microseconds
    }

    #[test]
    fn test_fallback_sleep_with_different_durations() {
        // Test with 60 seconds
        let config = create_test_config(60, 10);
        let mock_platform = MockDeepSleepPlatform::new();
        let deep_sleep = DeepSleep::new(config.clone(), mock_platform.clone());

        let result = AppController::fallback_sleep(&deep_sleep, &config, "Error");
        assert!(result.is_ok());
        
        let calls = mock_platform.get_sleep_calls();
        assert_eq!(calls[0], 60_000_000); // 60 seconds

        // Test with 600 seconds
        let config2 = create_test_config(600, 10);
        let mock_platform2 = MockDeepSleepPlatform::new();
        let deep_sleep2 = DeepSleep::new(config2.clone(), mock_platform2.clone());

        let result2 = AppController::fallback_sleep(&deep_sleep2, &config2, "Error");
        assert!(result2.is_ok());
        
        let calls2 = mock_platform2.get_sleep_calls();
        assert_eq!(calls2[0], 600_000_000); // 600 seconds
    }
}
