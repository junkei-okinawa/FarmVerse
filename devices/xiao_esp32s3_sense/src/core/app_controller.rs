use log::{error, info, warn};
use std::sync::Arc;
use crate::config::AppConfig;
use crate::communication::esp_now::{EspNowReceiver};
use crate::power::sleep::{SleepManager, SleepType, DeepSleepPlatform, LightSleepPlatform};

/// アプリケーションの主要な制御フローを管理するモジュール
pub struct AppController;

impl AppController {
    /// スリープコマンドを受信して最適なモード（Deep/Light）でスリープを実行
    pub fn handle_sleep_with_server_command<D: DeepSleepPlatform, L: LightSleepPlatform>(
        esp_now_receiver: &EspNowReceiver,
        sleep_manager: &SleepManager<D, L>,
        config: &Arc<AppConfig>,
    ) -> anyhow::Result<SleepType> {
        info!("=== サーバーからのスリープコマンド待機開始 ===");
        info!("設定されたデフォルトスリープ時間: {}秒", config.sleep_duration_seconds);
        info!("スリープコマンド待機タイムアウト: {}秒", config.sleep_command_timeout_seconds);
        
        // ESP-NOW受信状態をリセット（前回の受信データをクリア）
        EspNowReceiver::reset_receiver_state();
        
        let duration = match esp_now_receiver.wait_for_sleep_command(config.sleep_command_timeout_seconds as u32) {
            Some(duration_seconds) => {
                if duration_seconds > 0 {
                    info!(
                        "✓ サーバーからスリープ時間を受信: {}秒。",
                        duration_seconds
                    );
                    duration_seconds as u64
                } else {
                    warn!("無効なスリープ時間 (0秒) を受信。デフォルト時間を使用します。");
                    config.sleep_duration_seconds
                }
            }
            None => {
                warn!("✗ スリープコマンドを受信できませんでした。デフォルト時間を使用します。");
                config.sleep_duration_seconds
            }
        };
        
        Self::secure_shutdown_and_sleep(sleep_manager, duration, config)
    }

    /// 無線停止、GPIO Hold設定を行い、安全にスリープへ移行
    fn secure_shutdown_and_sleep<D: DeepSleepPlatform, L: LightSleepPlatform>(
        sleep_manager: &SleepManager<D, L>,
        duration_seconds: u64,
        _config: &Arc<AppConfig>,
    ) -> anyhow::Result<SleepType> {
        info!("=== スリープ準備シーケンスを開始します ({}秒) ===", duration_seconds);

        // スリープタイプを判定（Deep SleepかLight Sleepか）
        // 10分(600秒)がデフォルトの閾値
        let is_light_sleep = duration_seconds <= 600;

        if !is_light_sleep {
            info!("DEEP SLEEPのためのハードウェア遮断を実行します...");
            // [PHASE 8] ステータスLED (GPIO 21) を強制的に消灯・固定
            unsafe {
                esp_idf_sys::gpio_set_level(21, 1);
                esp_idf_sys::gpio_hold_en(21);
                
                // [PHASE 8] センサー用電源ピン (GPIO 2, 5) も確実にオフに固定
                esp_idf_sys::gpio_set_level(2, 0); // Temp sensor power
                esp_idf_sys::gpio_hold_en(2);
                esp_idf_sys::gpio_set_level(5, 0); // TDS sensor power
                esp_idf_sys::gpio_hold_en(5);
            }
            info!("✓ LEDとセンサー電源ピンをDeep Sleep用にHoldしました");

            // [PHASE 8] 無線機能を物理的に停止（電力ドレインの最大の原因の一つ）
            unsafe {
                let _ = esp_idf_sys::esp_now_deinit();
                let _ = esp_idf_sys::esp_wifi_stop();
                let _ = esp_idf_sys::esp_wifi_deinit();
            }
            info!("✓ WiFi/ESP-NOWスタックを完全にシャットダウンしました");
        } else {
            info!("LIGHT SLEEPのため、周辺機器の状態を保持しますが、無線(RF)は完全に停止します。");
            // [PHASE 10] 無線機能を完全に停止（復帰後の再初期化を前提とする）
            unsafe {
                let _ = esp_idf_sys::esp_now_deinit();
                let _ = esp_idf_sys::esp_wifi_stop();
                let _ = esp_idf_sys::esp_wifi_deinit();
                
                // [PHASE 11] Light Sleep中もセンサー電源は不要なのでOFFにする
                // リーク電流を防ぐために明示的にLow出力を行う
                esp_idf_sys::gpio_set_level(2, 0); // Temp sensor power
                esp_idf_sys::gpio_set_level(5, 0); // TDS sensor power
                // Light SleepではGPIO状態が保持されるため、Holdは必須ではないが
                // 念のためDeep Sleep同様に電源ラインは落としておく
            }
            // ステータスLEDは消灯
            unsafe {
                esp_idf_sys::gpio_set_level(21, 1);
            }
        }

        // 最適化されたスリープを実行
        sleep_manager.sleep_optimized(duration_seconds)
    }

    /// エラー時のフォールバックスリープ
    pub fn fallback_sleep<D: DeepSleepPlatform, L: LightSleepPlatform>(
        sleep_manager: &SleepManager<D, L>,
        config: &Arc<AppConfig>,
        error_msg: &str,
    ) -> anyhow::Result<SleepType> {
        error!("{}", error_msg);
        sleep_manager.sleep_optimized(config.sleep_duration_seconds)
    }
}

#[cfg(all(test, not(target_os = "espidf")))]
mod tests {
    use super::*;
    use crate::mac_address::MacAddress;
    use crate::power::sleep::DeepSleepPlatform;
    use std::sync::{Arc, Mutex};
    use std::str::FromStr;

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
            receiver_mac: MacAddress::from_str("AA:BB:CC:DD:EE:FF").unwrap(),
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
        let deep_sleep = DeepSleep::new(mock_platform.clone());

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
        let deep_sleep = DeepSleep::new(mock_platform.clone());

        let result = AppController::fallback_sleep(&deep_sleep, &config, "Error");
        assert!(result.is_ok());
        
        let calls = mock_platform.get_sleep_calls();
        assert_eq!(calls[0], 60_000_000); // 60 seconds

        // Test with 600 seconds
        let config2 = create_test_config(600, 10);
        let mock_platform2 = MockDeepSleepPlatform::new();
        let deep_sleep2 = DeepSleep::new(mock_platform2.clone());

        let result2 = AppController::fallback_sleep(&deep_sleep2, &config2, "Error");
        assert!(result2.is_ok());
        
        let calls2 = mock_platform2.get_sleep_calls();
        assert_eq!(calls2[0], 600_000_000); // 600 seconds
    }
}
