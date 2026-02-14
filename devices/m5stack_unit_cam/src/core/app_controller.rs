use log::{error, info, warn};
use std::sync::Arc;

use crate::core::config::AppConfig;
use crate::core::resolve_sleep_duration_seconds;
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
        
        let received = esp_now_receiver.wait_for_sleep_command(config.sleep_command_timeout_seconds as u32);
        let target_duration = resolve_sleep_duration_seconds(received, config.sleep_duration_seconds);

        match received {
            Some(duration_seconds) if duration_seconds > 0 => {
                info!(
                    "✓ サーバーからスリープ時間を受信: {}秒。ディープスリープに入ります。",
                    duration_seconds
                );
            }
            Some(_) => {
                warn!("無効なスリープ時間 (0秒) を受信。デフォルト時間を使用します。");
                info!("デフォルトスリープ時間でディープスリープに入ります: {}秒", config.sleep_duration_seconds);
            }
            None => {
                warn!("✗ スリープコマンドを受信できませんでした。デフォルト時間を使用します。");
                info!("デフォルトスリープ時間でディープスリープに入ります: {}秒", config.sleep_duration_seconds);
            }
        }

        deep_sleep_controller.sleep_for_duration(target_duration)?;
        
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
