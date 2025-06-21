use chrono::{DateTime, Datelike, NaiveDate, Utc};
use chrono_tz::Tz;
use log::info;

use crate::sleep::{DeepSleep, DeepSleepPlatform};

/// RTC時刻管理モジュール
pub struct RtcManager;

impl RtcManager {
    /// RTCの時刻をチェックし、必要に応じて2025年1月1日に設定
    /// 
    /// 年が2025年未満の場合、RTCを2025年1月1日に設定して1秒スリープ
    pub fn check_and_initialize_rtc<P: DeepSleepPlatform>(
        timezone: &Tz,
        deep_sleep_controller: &DeepSleep<P>,
    ) -> anyhow::Result<()> {
        info!("RTCの現在時刻をチェックしています...");
        let current_time = Utc::now().with_timezone(timezone);
        
        if current_time.year() < 2025 {
            info!("RTCの現在時刻が2025年以前です。RTCを2025年1月1日に設定し、1秒スリープします。");
            Self::set_rtc_to_2025()?;
            info!("RTCを2025年1月1日に設定しました。1秒間スリープします。");
            deep_sleep_controller.sleep_for_duration(1)?;
        }
        
        info!("RTC時刻チェック完了。処理を続行します。");
        Ok(())
    }

    /// RTCを2025年1月1日 00:00:00に設定
    fn set_rtc_to_2025() -> anyhow::Result<()> {
        let target_date = NaiveDate::from_ymd_opt(2025, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let target_utc: DateTime<Utc> = DateTime::from_naive_utc_and_offset(target_date, Utc);
        
        // ESP32のRTC時刻設定
        unsafe {
            let timestamp_seconds = target_utc.timestamp();
            let tv_sec = timestamp_seconds;
            let tv_usec = 0;
            let tv = esp_idf_svc::sys::timeval { tv_sec, tv_usec };
            esp_idf_svc::sys::settimeofday(&tv, std::ptr::null());
        }
        
        Ok(())
    }
}
