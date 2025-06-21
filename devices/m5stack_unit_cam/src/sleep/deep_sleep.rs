//! Deep Sleep Management Module
//!
//! This module provides:
//! - Determination of whether SNTP time‐sync over Wi-Fi is required.
//! - Execution of SNTP time synchronization.
//! - Fixed-interval and “target-digit” deep-sleep strategies.
//! - Wi-Fi connection and disconnection helpers during time sync.
use crate::config::AppConfig;
use esp_idf_svc::sntp::{EspSntp, OperatingMode, SntpConf, SyncStatus};
use esp_idf_svc::wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi};
use log::{error, info, warn}; // Removed debug
use std::sync::Arc;
use std::time::{Duration as StdDuration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, NaiveDate, TimeZone, Timelike, Utc, Local}; // Removed Duration as ChronoDuration
use chrono::Duration; // Added for ChronoDuration
use chrono_tz::Tz; // Import Tz directly

// Constants for SNTP
const MAX_SNTP_RETRIES: u32 = 30;
const SNTP_RETRY_DELAY_MS_IN_PROGRESS: u64 = 2000; // 2 seconds
const SNTP_RETRY_DELAY_MS_OTHER: u64 = 5000; // 5 seconds

// Constant for sleep logic
// const MIN_SLEEP_SECONDS_IN_TSLD_MODE: i64 = 11; // Minimum sleep if target_second_last_digit is used


#[derive(Debug, thiserror::Error)]
pub enum DeepSleepError {
    #[error("Invalid sleep duration: {0}")]
    InvalidDuration(String),
    #[error("Failed to get system time: {0}")]
    SystemTimeError(String),
    #[error("Failed to convert to Chrono type: {0}")]
    ChronoConversionError(String),
    #[error("Wi-Fi error: {0}")]
    WifiError(String),
    #[error("SNTP error: {0}")]
    SntpError(String),
    #[error("Wi-Fi connection failed: {0}")]
    WifiConnectionFailed(String),
    #[error("Time synchronization failed: {0}")]
    TimeSyncFailed(String),
    #[error("Invalid configuration: {0}")]
    InvalidConfiguration(String),
}

/// Platform-agnostic deep-sleep abstraction.
///
/// Implement this trait for any target that can enter
/// deep sleep by providing a `deep_sleep(duration_us)` method.
pub trait DeepSleepPlatform {
    /// Enter deep sleep for the specified duration in microseconds.
    ///
    /// # Arguments
    ///
    /// * `duration_us` – Sleep duration in microseconds.
    fn deep_sleep(&self, duration_us: u64);
}

/// ESP-IDF implementation of `DeepSleepPlatform`.
///
/// Calls the raw `esp_deep_sleep` API under the hood.
pub struct EspIdfDeepSleep;

impl DeepSleepPlatform for EspIdfDeepSleep {
    /// Enters deep sleep using the ESP-IDF `esp_deep_sleep` function.
    ///
    /// # Arguments
    ///
    /// * `duration_us` – Sleep duration in microseconds.
    fn deep_sleep(&self, duration_us: u64) {
        unsafe {
            esp_idf_sys::esp_deep_sleep(duration_us);
        }
    }
}

/// Core deep-sleep controller.
///
/// Holds application configuration and a platform implementation.
pub struct DeepSleep<P: DeepSleepPlatform> {
    config: Arc<AppConfig>,
    platform: P,
}

impl<P: DeepSleepPlatform> DeepSleep<P> {
    /// Create a new `DeepSleep` controller.
    ///
    /// # Arguments
    ///
    /// * `config` – Shared application settings.
    /// * `platform` – Platform-specific deep-sleep impl.
    pub fn new(config: Arc<AppConfig>, platform: P) -> Self {
        DeepSleep { config, platform }
    }

    /// Check whether SNTP time synchronization is required.
    ///
    /// Returns `Ok(true)` if the current system time is before
    /// 2025-01-01 00:00:00 UTC, else `Ok(false)`.
    ///
    /// # Errors
    ///
    /// Returns `DeepSleepError` if system time cannot be read or
    /// converted.
    pub fn is_time_sync_required(&self) -> Result<bool, DeepSleepError> {
        let now_unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| {
                DeepSleepError::SystemTimeError(format!("Failed to get system time: {:?}", e))
            })?
            .as_secs();

        let current_time_chrono = Utc
            .timestamp_opt(now_unix_secs as i64, 0)
            .single()
            .ok_or_else(|| {
                DeepSleepError::ChronoConversionError(
                    "Failed to convert system time to chrono DateTime".to_string(),
                )
            })?;

        // 2025年1月1日 00:00:00 UTC のDateTimeオブジェクトを作成
        let threshold_naive_date = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(); // Should be safe
        let threshold_naive_datetime = threshold_naive_date.and_hms_opt(0, 0, 0).unwrap(); // Should be safe
        let threshold_time = Utc.from_utc_datetime(&threshold_naive_datetime);

        info!(
            "Current system time for sync check: {}, Threshold time: {}",
            current_time_chrono, threshold_time
        );

        if current_time_chrono < threshold_time {
            Ok(true)
        } else {
            info!("Time synchronization not needed (current time is past threshold).");
            Ok(false)
        }
    }

    /// Perform actual time synchronization using SNTP.
    ///
    /// # Arguments
    ///
    /// * `wifi` – Blocking Wi-Fi interface.
    /// * `ssid` – Network SSID.
    /// * `password` – Network password.
    ///
    /// # Errors
    ///
    /// Returns `DeepSleepError` on Wi-Fi or SNTP failures.
    pub fn perform_actual_time_sync(
        &mut self,
        wifi: &mut BlockingWifi<EspWifi<'static>>,
        ssid: &str,
        password: &str,
    ) -> Result<(), DeepSleepError> {
        info!("時刻同期を開始します。SSID: {}", ssid);

        // 既存のWiFi設定をクリアし、停止する
        match wifi.stop() {
            Ok(_) => info!("既存のWiFiセッションを停止しました。"),
            Err(e) if e.code() == esp_idf_sys::ESP_ERR_WIFI_NOT_INIT => {
                info!("WiFiはまだ初期化されていません。");
            }
            Err(e) => {
                warn!(
                    "既存のWiFiセッションの停止に失敗しました: {:?}。処理を続行します。",
                    e
                );
            }
        }

        let auth_method = if password.is_empty() {
            info!("WiFiパスワードが空のため、認証方式を None に設定します。");
            AuthMethod::None
        } else {
            AuthMethod::WPA2Personal
        };

        let client_config = ClientConfiguration {
            ssid: ssid
                .try_into()
                .map_err(|e| DeepSleepError::WifiError(format!("無効なSSID: {:?}", e)))?,
            password: password
                .try_into()
                .map_err(|e| DeepSleepError::WifiError(format!("無効なパスワード: {:?}", e)))?,
            auth_method,
            ..Default::default()
        };
        wifi.set_configuration(&Configuration::Client(client_config))
            .map_err(|e| DeepSleepError::WifiError(format!("WiFi設定の適用に失敗: {:?}", e)))?;

        wifi.start()
            .map_err(|e| DeepSleepError::WifiError(format!("WiFiの開始に失敗: {:?}", e)))?;
        info!("WiFiを開始しました。接続を試みます...");

        wifi.connect()
            .map_err(|e| DeepSleepError::WifiError(format!("WiFi接続に失敗: {:?}", e)))?;
        info!("WiFiに接続しました。IPアドレスの取得を待ちます...");
        let ip_info = wifi.wifi().sta_netif().get_ip_info().map_err(|e| {
            error!("IPアドレスの取得に失敗しました: {:?}", e);
            DeepSleepError::WifiConnectionFailed(format!("Failed to get IP info: {:?}", e))
        })?;
        info!("IPアドレスを取得しました: {:?}", ip_info);

        info!("SNTPを初期化しています...");
        let sntp_config = SntpConf {
            operating_mode: OperatingMode::Poll,
            servers: ["ntp.nict.jp"],
            ..Default::default()
        };
        let sntp = match EspSntp::new(&sntp_config) {
            Ok(s) => s,
            Err(e) => {
                error!("SNTPの初期化に失敗しました: {:?}", e);
                Self::disconnect_wifi_static_helper(wifi)?; // Use associated function
                return Err(DeepSleepError::TimeSyncFailed(format!(
                    "SNTP initialization failed: {:?}",
                    e
                )));
            }
        };

        let mut retry_count = 0;
        let mut synchronized = false;
        info!(
            "SNTP初期化完了。時刻同期を開始します... (最大試行回数: {})",
            MAX_SNTP_RETRIES
        );

        while retry_count < MAX_SNTP_RETRIES {
            let status = sntp.get_sync_status();
            match status {
                SyncStatus::Completed => {
                    info!(
                        "SNTPステータス: Completed. 時刻同期に成功しました。 (試行 {}/{})",
                        retry_count + 1,
                        MAX_SNTP_RETRIES
                    );
                    synchronized = true;
                    break;
                }
                SyncStatus::InProgress => {
                    info!(
                        "SNTPステータス: InProgress. 同期処理中... (試行 {}/{})",
                        retry_count + 1,
                        MAX_SNTP_RETRIES
                    );
                    std::thread::sleep(StdDuration::from_millis(SNTP_RETRY_DELAY_MS_IN_PROGRESS));
                }
                SyncStatus::Reset => {
                    info!(
                        "SNTPステータス: Reset. リトライします... (試行 {}/{})",
                        retry_count + 1,
                        MAX_SNTP_RETRIES
                    );
                    std::thread::sleep(StdDuration::from_millis(SNTP_RETRY_DELAY_MS_OTHER));
                }
            }
            retry_count += 1;

            if !synchronized && retry_count == MAX_SNTP_RETRIES / 2 {
                warn!(
                    "SNTP同期が試行の半数 ({}/{}) に達しましたが、まだ完了していません。",
                    retry_count, MAX_SNTP_RETRIES
                );
            }
        }

        if synchronized {
            let now = Utc::now();
            info!("SNTP時刻同期成功。現在のUTC時刻: {}", now);
            let local_time_check = Utc::now().with_timezone(
                &self
                    .config
                    .timezone
                    .parse::<Tz>()
                    .unwrap_or(chrono_tz::Asia::Tokyo),
            );
            info!("現在のローカル時刻 (確認用): {}", local_time_check);
        } else {
            error!(
                "SNTP時刻同期タイムアウト。最大試行回数 ({}) を超えました。",
                MAX_SNTP_RETRIES
            );
            Self::disconnect_wifi_static_helper(wifi)?; // Use associated function
            return Err(DeepSleepError::TimeSyncFailed(
                "SNTP sync timeout after max retries".to_string(),
            ));
        }

        Self::disconnect_wifi_static_helper(wifi)?; // Use associated function
        Ok(())
    }

    /// Deep-sleep based on either:
    /// 1. Fixed interval (`sleep_duration_seconds`), or
    /// 2. Target-digit mode (minute’s last digit or second’s tens digit).
    ///
    /// # Arguments
    ///
    /// * `elapsed_time_in_current_loop` – Time spent in this loop.
    /// * `min_sleep_duration` – Minimum sleep duration.
    ///
    /// # Errors
    ///
    /// Returns `DeepSleepError` if timezone parsing fails or other
    /// calculation errors occur.
    pub fn sleep(
        &self,
        elapsed_time_in_current_loop: StdDuration,
        min_sleep_duration: StdDuration,
    ) -> Result<(), DeepSleepError> {
        let timezone_str = self.config.timezone.as_str();

        // The decision to use target digit sleep is now made in main.rs
        // This method now only handles fixed interval sleep.
        info!("Entering fixed interval sleep mode.");
        let interval_total_secs = self.config.sleep_duration_seconds as u64;

        let sleep_for_secs = if interval_total_secs > elapsed_time_in_current_loop.as_secs() {
            interval_total_secs - elapsed_time_in_current_loop.as_secs()
        } else {
            warn!(
                "Processing time ({:?}) exceeded sleep interval ({}s). Using minimum sleep duration ({:?}).",
                elapsed_time_in_current_loop, interval_total_secs, min_sleep_duration
            );
            min_sleep_duration.as_secs()
        };

        // マイクロ秒に変換
        let mut sleep_for_micros = sleep_for_secs * 1_000_000;

        // 設定ファイルから補正値を取得して加算 (マイクロ秒単位)
        // sleep_compensation_micros は i64 なので、符号を考慮して加算
        let compensation_micros = self.config.sleep_compensation_micros;
        if compensation_micros >= 0 {
            sleep_for_micros = sleep_for_micros.saturating_add(compensation_micros as u64);
        } else {
            // 補正値が負の場合は、絶対値を減算する
            sleep_for_micros = sleep_for_micros.saturating_sub(compensation_micros.abs() as u64);
        }


        if sleep_for_micros == 0 {
            warn!(
                "Calculated interval sleep duration is zero. Overriding to min_sleep_duration."
            );
            // min_sleep_duration もマイクロ秒で扱う
            sleep_for_micros = min_sleep_duration.as_micros() as u64;
        }
        if sleep_for_micros == 0 && min_sleep_duration.as_micros() == 0 {
            warn!("Min sleep duration is also zero. Setting to 1 second to avoid issues.");
            sleep_for_micros = 1_000_000; // 1秒
        }

        info!(
            "Deep sleeping for {} microseconds (interval mode, timezone: {}).",
            sleep_for_micros, timezone_str
        );
        self.platform.deep_sleep(sleep_for_micros);
        // deep_sleep からは復帰しないため、Ok(()) は実際には返らない
        // しかし、シグネチャ上は Result を返す必要がある
        #[allow(unreachable_code)]
        Ok(())
    }

    /// Enter deep sleep for a fixed number of seconds.
    ///
    /// # Arguments
    ///
    /// * `duration_seconds` – Sleep duration in seconds (must be > 0).
    ///
    /// # Errors
    ///
    /// Returns `InvalidDuration` if `duration_seconds == 0`.
    pub fn sleep_for_duration(&self, duration_seconds: u64) -> Result<(), DeepSleepError> {
        if duration_seconds == 0 {
            return Err(DeepSleepError::InvalidDuration(
                "スリープ時間は0より大きくなければなりません".to_string(),
            ));
        }
        info!("{}秒間のディープスリープに入ります...", duration_seconds);
        self.platform.deep_sleep(duration_seconds * 1_000_000);
        // deep_sleep からは復帰しないため、Ok(()) は実際には返らない
        // しかし、シグネチャ上は Result を返す必要がある
        #[allow(unreachable_code)]
        Ok(())
    }

    /// 指定された目標の分と秒の数字に合致するまでディープスリープします。
    /// RTCの時刻が目標のパターンに一致する次のタイミングで起動します。
    ///
    /// # Arguments
    /// * `elapsed_time_in_current_loop` - 現在のメインループ処理に費やされた時間。
    ///                                    この時間は、計算されるスリープ期間から差し引かれます。
    ///
    /// # Returns
    /// `Ok(())` もしスリープが正常に開始された場合 (ただし、この関数は通常戻りません)、
    /// またはスリープがスキップされた場合。
    /// `Err(DeepSleepError)` もしエラーが発生した場合 (現在は未使用だが将来のため)。
    pub fn sleep_until_target_digits_match(
        &mut self,
        elapsed_time_in_current_loop: StdDuration,
    ) -> Result<(), DeepSleepError> {
        if let Some(ref target_conf) = self.config.target_digits_config {
            let now = Local::now(); // Capture current time once

            info!(
                "Attempting to sleep until target digits. Current time: {}. Target config: min_last_digit={:?}, sec_tens_digit={:?}. Elapsed loop time: {:?}",
                now.format("%Y-%m-%d %H:%M:%S"),
                target_conf.minute_last_digit,
                target_conf.second_tens_digit,
                elapsed_time_in_current_loop
            );

            // Calculate seconds to the *next* target time
            // This is called only if target_conf has at least one digit set (guaranteed by AppConfig::load logic)
            if target_conf.minute_last_digit.is_none() && target_conf.second_tens_digit.is_none() {
                info!("TargetDigitsConfig is Some, but no specific digits are set. This should not happen if AppConfig::load is correct. Skipping target digit sleep.");
                return Ok(());
            }

            if let Some(seconds_to_target_u32) = Self::calculate_seconds_to_target(
                now, // Pass current time
                target_conf.minute_last_digit, // This is already Option<u8>
                target_conf.second_tens_digit, // This is already Option<u8>
            ) {
                if seconds_to_target_u32 > 0 {
                    info!(
                        "Calculated seconds to next target: {} s",
                        seconds_to_target_u32
                    );

                    let sleep_for_seconds_u64 = seconds_to_target_u32 as u64;

                    if sleep_for_seconds_u64 > elapsed_time_in_current_loop.as_secs() {
                        let actual_sleep_duration_secs =
                            sleep_for_seconds_u64 - elapsed_time_in_current_loop.as_secs();
                        
                        if actual_sleep_duration_secs > 0 {
                            info!(
                                "Deep sleeping for {} seconds (until target digits).",
                                actual_sleep_duration_secs
                            );
                            self.platform
                                .deep_sleep(actual_sleep_duration_secs * 1_000_000);
                            // Unreachable code after deep_sleep
                            #[allow(unreachable_code)]
                            return Ok(()); // Should not be reached
                        } else {
                            info!(
                                "Adjusted sleep duration is zero or negative (target: {}s, elapsed: {:?}). Skipping sleep.",
                                sleep_for_seconds_u64,
                                elapsed_time_in_current_loop
                            );
                        }
                    } else {
                        info!(
                            "Time to next target ({}s) is less than or equal to elapsed loop time ({:?}). Skipping sleep.",
                            sleep_for_seconds_u64,
                            elapsed_time_in_current_loop
                        );
                    }
                } else {
                    // seconds_to_target_u32 is 0. This means calculate_seconds_to_target found an issue or
                    // the target is "now" but it should return >0 for future targets.
                    warn!(
                        "Calculated seconds_to_target is 0. This indicates an issue or immediate match where future was expected. Skipping sleep."
                    );
                }
            } else {
                warn!("Could not determine next target time slot. Check target configuration and RTC. Skipping target digit sleep.");
            }
        } else {
            // This branch should ideally not be hit if main.rs calls this function only when target_digits_config is Some.
            info!("Target digits not configured. Skipping target-digit sleep.");
        }
        Ok(()) // Return Ok if no sleep occurred or if config was None.
    }

    /// Calculate the number of seconds from `start_time` until the next future time
    /// matching the specified optional target minute's last digit and optional target second's tens digit.
    ///
    /// # Arguments
    ///
    /// * `start_time` - The reference time from which to calculate the duration.
    /// * `target_minute_last_digit_opt` - Optional target last digit of the minute (0-9).
    /// * `target_second_tens_digit_opt` - Optional target tens digit of the second (0-5).
    ///
    /// # Returns
    ///
    /// The number of seconds (as u32) until the next target time, if found within
    /// a 120-minute search window. Returns `None` if no such time is found or if
    /// no targets are specified.
    fn calculate_seconds_to_target(
        start_time: DateTime<Local>,
        target_minute_last_digit_opt: Option<u8>,
        target_second_tens_digit_opt: Option<u8>,
    ) -> Option<u32> {
        // If no targets are specified, then any time is "valid" but this function's purpose is specific.
        // This should ideally be guarded by the caller ensuring at least one target is Some.
        if target_minute_last_digit_opt.is_none() && target_second_tens_digit_opt.is_none() {
            warn!("calculate_seconds_to_target called with no target digits specified.");
            return None;
        }

        // Start searching from the second *after* start_time to ensure future time.
        let mut current_check_time = start_time + Duration::seconds(1); // Changed ChronoDuration to Duration

        // Search for up to 120 minutes (7200 seconds).
        for _ in 0..(120 * 60) {
            let minute_val = current_check_time.minute();
            let second_val = current_check_time.second();

            let mut minute_criteria_met = target_minute_last_digit_opt.is_none(); // True if not specified
            if let Some(target_mld) = target_minute_last_digit_opt {
                if minute_val % 10 == target_mld as u32 {
                    minute_criteria_met = true;
                }
            }

            let mut second_criteria_met = target_second_tens_digit_opt.is_none(); // True if not specified
            if let Some(target_std) = target_second_tens_digit_opt {
                // Ensure target_std is within valid range (0-5 for tens digit of second)
                // This check should ideally be at config load time, but defensive check here is okay.
                if (0..=5).contains(&target_std) && second_val / 10 == target_std as u32 {
                    second_criteria_met = true;
                } else if !(0..=5).contains(&target_std) {
                    // Invalid target, effectively makes this criteria unmatchable if specified
                    warn!("Invalid target_second_tens_digit: {}. Must be 0-5.", target_std);
                    second_criteria_met = false; // Ensure it fails if an invalid target was somehow passed
                }
            }
            
            if minute_criteria_met && second_criteria_met {
                // Found the target time.
                let duration_to_target = current_check_time.signed_duration_since(start_time);
                
                // Ensure the found time is strictly after start_time.
                // num_seconds() can be 0 if current_check_time is the same as start_time,
                // but we started search from start_time + 1s.
                if duration_to_target.num_seconds() > 0 {
                    return Some(duration_to_target.num_seconds() as u32);
                } else {
                    // This case should be rare given the search starts at start_time + 1s.
                    // It might occur if time "stands still" or moves backward, or if duration is < 1s and gets truncated.
                    // Log it and continue search, or return None. For now, log and let search continue.
                    warn!(
                        "Calculated non-positive duration ({}s) to target. Current: {}, Start: {}. Continuing search.",
                        duration_to_target.num_seconds(), current_check_time, start_time
                    );
                }
            }
            current_check_time = current_check_time + Duration::seconds(1); // Changed ChronoDuration to Duration
        }

        warn!(
            "No target time found matching criteria (min_last_digit: {:?}, sec_tens_digit: {:?}) within 120 minutes from {}.",
            target_minute_last_digit_opt, target_second_tens_digit_opt, start_time
        );
        None
    }

    /// Enter deep sleep for a fixed duration, adjusted to avoid
    /// waking up exactly at the target time if configured.
    /// If target digits are configured, this function may shorten the sleep
    /// to wake up at the next target digit occurrence, if that occurrence
    /// is sooner than `duration_seconds`.
    ///
    /// # Arguments
    ///
    /// * `duration_seconds` – Sleep duration in seconds (must be > 0).
    ///
    /// # Errors
    ///
    /// Returns `InvalidDuration` if `duration_seconds == 0`.
    pub fn sleep_for_duration_adjusted(&self, duration_seconds: u64) -> Result<(), DeepSleepError> {
        if duration_seconds == 0 {
            return Err(DeepSleepError::InvalidDuration(
                "スリープ時間は0より大きくなければなりません".to_string(),
            ));
        }

        let mut final_sleep_duration_secs = duration_seconds;

        if let Some(ref target_conf) = self.config.target_digits_config {
            // Only adjust if target digits are configured.
            let now = Local::now();
            info!(
                "Adjusting sleep duration. Original: {}s. Current time: {}. Target config: min_last_digit={:?}, sec_tens_digit={:?}",
                duration_seconds,
                now.format("%H:%M:%S"),
                target_conf.minute_last_digit,
                target_conf.second_tens_digit
            );

            if target_conf.minute_last_digit.is_some() || target_conf.second_tens_digit.is_some() {
                // Only calculate if at least one target digit is set
                if let Some(secs_to_target_u32) = Self::calculate_seconds_to_target(
                    now,
                    target_conf.minute_last_digit, // Already Option<u8>
                    target_conf.second_tens_digit, // Already Option<u8>
                ) {
                    let secs_to_target_u64 = secs_to_target_u32 as u64;
                    if secs_to_target_u64 > 0 && secs_to_target_u64 < duration_seconds {
                        info!(
                            "Target is sooner ({}s) than requested duration ({}s). Adjusting sleep to {}s.",
                            secs_to_target_u64, duration_seconds, secs_to_target_u64
                        );
                        final_sleep_duration_secs = secs_to_target_u64;
                    } else if secs_to_target_u64 > 0 {
                        info!(
                            "Requested duration ({}s) is shorter/equal to time to target ({}s). Using requested duration.",
                            duration_seconds, secs_to_target_u64
                        );
                        // final_sleep_duration_secs remains duration_seconds
                    } else { // secs_to_target_u32 was 0 or calculate_seconds_to_target had an issue
                        warn!(
                            "Calculated seconds to target is 0 or invalid ({}s). Using original duration: {}s.",
                            secs_to_target_u32, duration_seconds
                        );
                    }
                } else {
                    warn!(
                        "Could not determine next target time for adjustment. Using original duration: {}s.",
                        duration_seconds
                    );
                }
            } else {
                info!("TargetDigitsConfig is Some, but no specific digits are set. Using original duration.");
            }
        } else {
            info!(
                "No target digits configured. Using original duration: {}s.",
                duration_seconds
            );
        }
        
        if final_sleep_duration_secs > 0 {
            info!(
                "Deep sleeping for {} seconds (adjusted or original).",
                final_sleep_duration_secs
            );
            self.platform.deep_sleep(final_sleep_duration_secs * 1_000_000);
        } else {
            info!("Final calculated sleep duration is zero or negative. Skipping hardware sleep. This might indicate an issue or that the next event is immediate.");
            // Consider a minimal sleep if 0 is problematic for the system.
            // For now, returning Ok(()) implies no actual hardware sleep if duration is 0.
        }

        #[allow(unreachable_code)]
        Ok(())
    }

    /// Sleep until the specified UTC `target_datetime`.
    ///
    /// # Arguments
    ///
    /// * `target_datetime` – Wake-up time in UTC.
    /// * `_elapsed_time_in_current_loop` – (Currently unused) Time spent in the current loop.
    /// * `min_sleep_duration` – Minimum sleep duration.
    ///
    /// If the target time is past, or results in a sleep duration less than
    /// `min_sleep_duration`, `min_sleep_duration` is used.
    /// If `target_datetime` is in the past, it will sleep for `min_sleep_duration`.
    ///
    /// # Errors
    ///
    /// This function itself doesn't return `DeepSleepError` but calls
    /// `platform.deep_sleep` which is expected to not return.
    pub fn sleep_until_target_time(
        &self,
        target_datetime: DateTime<Utc>,
        _elapsed_time_in_current_loop: StdDuration,
        min_sleep_duration: StdDuration,
    ) -> Result<(), DeepSleepError> {
        let now_utc = Utc::now();
        info!(
            "現在のUTC時刻: {}, スリープ開始時刻: {}",
            now_utc,
            target_datetime.format("%Y-%m-%d %H:%M:%S")
        );

        let duration_to_target = target_datetime.signed_duration_since(now_utc);
        let mut sleep_duration_seconds = duration_to_target.num_seconds();

        if sleep_duration_seconds <= 0 {
            warn!("指定された時刻が現在時刻を過ぎています。スリープしません。");
            return Ok(());
        }

        let min_sleep_secs = min_sleep_duration.as_secs();
        if sleep_duration_seconds < min_sleep_secs as i64 {
            sleep_duration_seconds = min_sleep_secs as i64;
        }

        let sleep_micros = (sleep_duration_seconds as u64) * 1_000_000;
        info!(
            "ディープスリープに入ります: {} 秒後に {} に復帰予定。",
            sleep_duration_seconds,
            target_datetime.format("%Y-%m-%d %H:%M:%S")
        );
        self.platform.deep_sleep(sleep_micros);
        // deep_sleep からは復帰しないため、Ok(()) は実際には返らない
        // しかし、シグネチャ上は Result を返す必要がある
        #[allow(unreachable_code)]
        Ok(())
    }

    /// Disconnect the Wi-Fi interface (instance method).
    ///
    /// This is an internal helper.
    ///
    /// # Arguments
    ///
    /// * `wifi` - Mutable reference to the `BlockingWifi` interface.
    ///
    /// # Errors
    ///
    /// Returns `DeepSleepError::WifiError` if disconnection fails.
    fn disconnect_wifi_helper(
        &self,
        wifi: &mut BlockingWifi<EspWifi<'static>>,
    ) -> Result<(), DeepSleepError> {
        info!("WiFiを切断します...");
        wifi.disconnect()
            .map_err(|e| DeepSleepError::WifiError(format!("WiFi切断に失敗: {:?}", e)))?;
        info!("WiFiを切断しました。");
        Ok(())
    }

    /// Disconnect the Wi-Fi interface (static helper).
    ///
    /// This is an internal helper, typically used when `self` is not available
    /// or when cleaning up in error paths of `perform_actual_time_sync`.
    ///
    /// # Arguments
    ///
    /// * `wifi` - Mutable reference to the `BlockingWifi` interface.
    ///
    /// # Errors
    ///
    /// Returns `DeepSleepError::WifiError` if disconnection fails.
    fn disconnect_wifi_static_helper(
        wifi: &mut BlockingWifi<EspWifi<'static>>,
    ) -> Result<(), DeepSleepError> {
        info!("(Static helper) WiFiを切断します...");
        wifi.disconnect().map_err(|e| {
            DeepSleepError::WifiError(format!("WiFi切断に失敗 (static_helper): {:?}", e))
        })?;
        info!("(Static helper) WiFiを切断しました。");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, TargetDigitsConfig}; // AppConfig と TargetDigitsConfig をインポート
    use crate::mac_address::MacAddress;
    use std::sync::Arc;
    use std::time::Duration as StdDuration;

    // MockPlatform の定義 (変更なし)
    struct MockPlatform {
        slept_for: std::cell::Cell<Option<u64>>,
    }

    impl MockPlatform {
        fn new() -> Self {
            MockPlatform {
                slept_for: std::cell::Cell::new(None),
            }
        }
    }

    impl DeepSleepPlatform for MockPlatform {
        fn deep_sleep(&self, duration_micros: u64) {
            self.slept_for.set(Some(duration_micros));
            // テスト中は実際にはスリープしない
            info!("MockPlatform: deep_sleep called with {} us", duration_micros);
        }
    }

    fn create_test_config(
        sleep_duration_seconds: u64,
        sleep_duration_seconds_for_medium: u64, // 追加
        target_minute_last_digit: Option<u8>,
        target_second_tens_digit: Option<u8>,
        sleep_compensation_micros: i64, // 追加
    ) -> Arc<AppConfig> {
        Arc::new(AppConfig {
            receiver_mac: MacAddress::from_str("00:11:22:33:44:55").unwrap(),
            sleep_duration_seconds,
            sleep_duration_seconds_for_medium, // 追加
            sleep_duration_seconds_for_long: 3600,
            frame_size: "VGA".to_string(),
            auto_exposure_enabled: true,
            camera_warmup_frames: Some(2),
            target_digits_config: if target_minute_last_digit.is_some() || target_second_tens_digit.is_some() {
                Some(TargetDigitsConfig {
                    minute_last_digit: target_minute_last_digit,
                    second_tens_digit: target_second_tens_digit,
                })
            } else {
                None
            },
            wifi_ssid: "test_ssid".to_string(),
            wifi_password: "test_password".to_string(),
            timezone: "Asia/Tokyo".to_string(),
            sleep_compensation_micros, // 追加
        })
    }

    #[test]
    fn test_fixed_interval_sleep_with_compensation() {
        let config = create_test_config(600, 600, None, None, 500_000); // 0.5秒補正
        let platform = MockPlatform::new();
        let deep_sleep_controller = DeepSleep::new(config, platform);

        let elapsed_time = StdDuration::from_secs(10);
        let min_sleep_duration = StdDuration::from_secs(1);

        let result = deep_sleep_controller.sleep(elapsed_time, min_sleep_duration);
        assert!(result.is_ok()); // deep_sleepはOk(())を返すが実際には戻らない

        // (600 - 10)秒 * 1_000_000 + 500_000マイクロ秒 = 590_500_000 マイクロ秒
        assert_eq!(
            deep_sleep_controller.platform.slept_for.get(),
            Some(590_500_000)
        );
    }

    #[test]
    fn test_fixed_interval_sleep_with_negative_compensation() {
        let config = create_test_config(600, 600, None, None, -2_000_000); // -2秒補正
        let platform = MockPlatform::new();
        let deep_sleep_controller = DeepSleep::new(config, platform);

        let elapsed_time = StdDuration::from_secs(5);
        let min_sleep_duration = StdDuration::from_secs(1);

        let _ = deep_sleep_controller.sleep(elapsed_time, min_sleep_duration);
        // (600 - 5)秒 * 1_000_000 - 2_000_000マイクロ秒 = 593_000_000 マイクロ秒
        assert_eq!(
            deep_sleep_controller.platform.slept_for.get(),
            Some(593_000_000)
        );
    }

    #[test]
    fn test_fixed_interval_sleep_exceeding_interval() {
        let config = create_test_config(60, 60, None, None, 100_000); // 0.1秒補正
        let platform = MockPlatform::new();
        let deep_sleep_controller = DeepSleep::new(config, platform);

        let elapsed_time = StdDuration::from_secs(70); // 処理時間がインターバルを超過
        let min_sleep_duration = StdDuration::from_secs(5);

        let _ = deep_sleep_controller.sleep(elapsed_time, min_sleep_duration);
        // min_sleep_duration (5秒) * 1_000_000 + 100_000マイクロ秒 = 5_100_000 マイクロ秒
        // 処理時間がインターバルを超えた場合、sleep_for_secs は min_sleep_duration.as_secs() になる。
        // その後、補正値が加算される。
        // 5 * 1_000_000 + 100_000 = 5_100_000
        assert_eq!(
            deep_sleep_controller.platform.slept_for.get(),
            Some(5_100_000)
        );
    }

    #[test]
    fn test_fixed_interval_sleep_zero_calculated_duration() {
        // このテストは、補正前の計算結果が0になるケース
        let config = create_test_config(30, 30, None, None, 200_000); // 0.2秒補正
        let platform = MockPlatform::new();
        let deep_sleep_controller = DeepSleep::new(config, platform);

        let elapsed_time = StdDuration::from_secs(30); // 処理時間 = インターバル
        let min_sleep_duration = StdDuration::from_secs(2);

        let _ = deep_sleep_controller.sleep(elapsed_time, min_sleep_duration);
        // (30 - 30) = 0 秒。 sleep_for_micros = 0.
        // compensation_micros = 200_000.
        // sleep_for_micros = 0 + 200_000 = 200_000.
        // この値は0ではないので、そのまま使われる。
        assert_eq!(
            deep_sleep_controller.platform.slept_for.get(),
            Some(200_000)
        );

        // 補正によって0になるケース
        let config_neg_comp = create_test_config(30, 30, None, None, -2_000_000); // -2秒補正
        let platform_neg_comp = MockPlatform::new();
        let deep_sleep_controller_neg_comp = DeepSleep::new(config_neg_comp, platform_neg_comp);
        let _ = deep_sleep_controller_neg_comp.sleep(StdDuration::from_secs(28), StdDuration::from_secs(3));
        // (30 - 28) = 2秒。 sleep_for_micros = 2_000_000.
        // compensation_micros = -2_000_000.
        // sleep_for_micros = 2_000_000 - 2_000_000 = 0.
        // この場合、min_sleep_duration (3秒 = 3_000_000 us) が使われる。
        assert_eq!(
            deep_sleep_controller_neg_comp.platform.slept_for.get(),
            Some(3_000_000)
        );
    }

    #[test]
    fn test_fixed_interval_sleep_zero_calculated_and_zero_min_duration() {
        let config = create_test_config(10, 10, None, None, -10_000_000); // -10秒補正
        let platform = MockPlatform::new();
        let deep_sleep_controller = DeepSleep::new(config, platform);

        let elapsed_time = StdDuration::from_secs(0); // 処理時間0
        let min_sleep_duration = StdDuration::from_secs(0); // 最小スリープも0

        let _ = deep_sleep_controller.sleep(elapsed_time, min_sleep_duration);
        // (10 - 0) = 10秒。sleep_for_micros = 10_000_000.
        // compensation_micros = -10_000_000.
        // sleep_for_micros = 10_000_000 - 10_000_000 = 0.
        // min_sleep_duration も 0 なので、デフォルトの1秒 (1_000_000 us)
        assert_eq!(
            deep_sleep_controller.platform.slept_for.get(),
            Some(1_000_000)
        );
    }

    #[test]
    fn test_sleep_until_target_digits_match_no_targets() {
        let config = create_test_config(600, 600, None, None, 0); // 補正なし
        let platform = MockPlatform::new();
        let mut deep_sleep_controller = DeepSleep::new(config, platform);
        let result =
            deep_sleep_controller.sleep_until_target_digits_match(StdDuration::from_secs(5));
        assert!(result.is_ok());
        assert!(deep_sleep_controller.platform.slept_for.get().is_none()); // No sleep should occur
    }

    // 他の sleep_until_target_digits_match のテストケースも同様に create_test_config を使用するように修正
    // (calculate_seconds_to_target のテストは AppConfig に依存しないので変更不要)
}
