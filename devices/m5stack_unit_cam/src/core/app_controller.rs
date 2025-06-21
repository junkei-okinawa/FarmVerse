use log::{error, info, warn};
use std::sync::Arc;

use crate::core::config::AppConfig;
use crate::communication::esp_now::EspNowSender;
use crate::power::sleep::{DeepSleep, DeepSleepPlatform};

/// アプリケーションの主要な制御フローを管理するモジュール
pub struct AppController;

impl AppController {
    /// スリープコマンドを受信してディープスリープを実行
    pub fn handle_sleep_with_server_command<P: DeepSleepPlatform>(
        esp_now_sender: &EspNowSender,
        deep_sleep_controller: &DeepSleep<P>,
        config: &Arc<AppConfig>,
    ) -> anyhow::Result<()> {
        info!("サーバーからのスリープ時間を受信中...");
        
        match esp_now_sender.receive_sleep_command(2000) {
            Some(duration_seconds) => {
                if duration_seconds > 0 {
                    info!(
                        "サーバーからスリープ時間を受信: {}秒。ディープスリープに入ります。",
                        duration_seconds
                    );
                    deep_sleep_controller.sleep_for_duration(duration_seconds as u64)?;
                } else {
                    warn!("無効なスリープ時間 (0秒) を受信。デフォルト時間を使用します。");
                    deep_sleep_controller.sleep_for_duration(config.sleep_duration_seconds)?;
                }
            }
            None => {
                warn!("スリープコマンドを受信できませんでした。デフォルト時間を使用します。");
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
