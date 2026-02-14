use log::{info, warn};
use crate::power::sleep::DeepSleepPlatform;

/// RTC時刻管理モジュール
pub struct RtcManager;

/// RTCメモリエリア（Deep Sleep中も保持される特殊なRAM）に起動情報を保持します。
/// #[link_section = ".rtc.data"] により、通常のRAMではなくRTC RAMに配置されます。
#[link_section = ".rtc.data"]
static mut RTC_BOOT_COUNT: u32 = 0;

impl RtcManager {
    /// RTCの状態を確認し、起動カウンタを管理します
    pub fn check_and_initialize_rtc<P: DeepSleepPlatform>(
        _timezone: &chrono_tz::Tz,
        _deep_sleep_platform: &P,
    ) -> anyhow::Result<()> {
        let cause = unsafe { esp_idf_sys::esp_sleep_get_wakeup_cause() };
        let reset_reason = unsafe { esp_idf_sys::esp_reset_reason() };
        
        let reason_str = match reset_reason {
            esp_idf_sys::esp_reset_reason_t_ESP_RST_POWERON => "POWERON (電源投入)",
            esp_idf_sys::esp_reset_reason_t_ESP_RST_EXT => "EXT (外部ピンリセット)",
            esp_idf_sys::esp_reset_reason_t_ESP_RST_SW => "SW (ソフトウェアリセット)",
            esp_idf_sys::esp_reset_reason_t_ESP_RST_PANIC => "PANIC (例外/パニック)",
            esp_idf_sys::esp_reset_reason_t_ESP_RST_INT_WDT => "INT_WDT (割り込みWDT)",
            esp_idf_sys::esp_reset_reason_t_ESP_RST_TASK_WDT => "TASK_WDT (タスクWDT)",
            esp_idf_sys::esp_reset_reason_t_ESP_RST_WDT => "WDT (その他WDT)",
            esp_idf_sys::esp_reset_reason_t_ESP_RST_DEEPSLEEP => "DEEPSLEEP (正常復帰)",
            esp_idf_sys::esp_reset_reason_t_ESP_RST_BROWNOUT => "BROWNOUT (電圧低下検出)",
            esp_idf_sys::esp_reset_reason_t_ESP_RST_SDIO => "SDIO (SDIOホストリセット)",
            _ => "UNKNOWN (不明)",
        };

        unsafe {
            if reset_reason == esp_idf_sys::esp_reset_reason_t_ESP_RST_DEEPSLEEP {
                // 真のDeep Sleepからの復帰
                RTC_BOOT_COUNT += 1;
                info!("✅ [DIAG] Deep Sleepからの復帰を確認しました (Reason: {}, Cause: {})", reason_str, cause);
                info!("✓ 継承カウンタ: {}", RTC_BOOT_COUNT);
            } else {
                // 初回起動やその他のリセット（スリープ失敗含む）
                RTC_BOOT_COUNT = 1;
                warn!("⚠️ [DIAG] 非Deepsleep起動を確認しました (Reason: {})", reason_str);
                info!("✓ カウンタを 1.0 にリセットしました");
            }
        }
        
        Ok(())
    }

    /// 現在の有効な起動回数を取得
    pub fn get_boot_count() -> u32 {
        unsafe { RTC_BOOT_COUNT }
    }

    /// 起動回数をインクリメント（Light Sleep復帰時などに使用）
    pub fn increment_boot_count() {
        unsafe { RTC_BOOT_COUNT += 1; }
    }
}
