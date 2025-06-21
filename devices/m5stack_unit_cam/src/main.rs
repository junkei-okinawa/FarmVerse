use chrono::{Datelike, NaiveDate, Timelike, Utc};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        adc::{
            attenuation::DB_12,
            oneshot::{
                config::{AdcChannelConfig, Calibration},
                AdcChannelDriver, AdcDriver,
            },
        },
        delay::FreeRtos,
        peripherals::Peripherals,
    },
    nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault}, // Added EspNvs, NvsDefault
    wifi::{BlockingWifi, EspWifi},
};
use std::sync::Arc;
use std::time::{Duration, Instant}; // Removed Local, TimeZone

mod camera;
mod config;
mod esp_now;
mod led;
mod mac_address;
mod sleep;

use camera::{CameraController, M5UnitCamConfig};
use config::AppConfig;
use esp_now::{EspNowSender, ImageFrame};
use led::StatusLed;
use log::{error, info, warn};
use sleep::{DeepSleep, EspIdfDeepSleep}; // EspIdfDeepSleep を追加
use simple_ds18b20_temp_sensor::TempSensor;

const DUMMY_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000"; // 64 zeros for SHA256 dummy

// --- 電圧測定用の定数 ---
const MIN_MV: f32 = 128.0; // UnitCam GPIO0 の実測値に合わせて調整
const MAX_MV: f32 = 3130.0; // UnitCam GPIO0 の実測値に合わせて調整
const RANGE_MV: f32 = MAX_MV - MIN_MV;
const LOW_VOLTAGE_THRESHOLD_PERCENT: u8 = 8;

// NVS constants
const NVS_NAMESPACE: &str = "app_state";
const NVS_KEY_LAST_BOOT_DATE: &str = "last_boot_d"; // Max 15 chars for key

// --- ここまで 定数 ---

struct MeasuredData {
    voltage_percent: u8,
    image_data_option: Option<Vec<u8>>, // 画像データは Option<Vec<u8>> に変更
    temperature_celsius_option: Option<f32>,
}

impl MeasuredData {
    fn new(voltage_percent: u8, image_data_option: Option<Vec<u8>>, temperature_celsius_option: Option<f32>) -> Self {
        Self {
            voltage_percent,
            image_data_option,
            temperature_celsius_option,
        }
    }
}

// --- transmit_data_task (modified signature) ---
fn transmit_data_task(
    config: &AppConfig,
    esp_now_sender: &EspNowSender, // Changed from _wifi
    led: &mut StatusLed,
    measured_data: MeasuredData,
) -> anyhow::Result<()> {
    // `esp_wifi_set_ps` moved to main
    // `EspNowSender::new()` and `add_peer` moved to main

    // Prepare timestamp string
    // tz is available in main, if needed here, it should be passed or re-initialized.
    // The original code used config.timezone to parse tz.
    let tz_task: chrono_tz::Tz = config.timezone.parse().unwrap_or(chrono_tz::Asia::Tokyo);
    let current_time_formatted = Utc::now()
        .with_timezone(&tz_task)
        .format("%Y/%m/%d %H:%M:%S%.3f")
        .to_string();
    let temperature = measured_data.temperature_celsius_option.unwrap_or(-999.0); // 温度センサーの結果を取得、エラー時は-999.0とする

    match measured_data.image_data_option {
        Some(image_data) => {
            // image_data は Vec<u8>
            match ImageFrame::calculate_hash(&image_data) {
                Ok(hash_str) => {
                    let mut final_payload_str = String::from("HASH:");
                    final_payload_str.push_str(&hash_str);
                    final_payload_str.push_str(&format!(",VOLT:{}", measured_data.voltage_percent));
                    final_payload_str.push_str(&format!(",TEMP:{}", temperature));
                    final_payload_str.push_str(",");
                    final_payload_str.push_str(&current_time_formatted);
                    let final_payload_bytes = final_payload_str.into_bytes();

                    info!(
                        "送信データ準備完了 (画像あり): ペイロードサイズ={}, 時刻={}, ハッシュ={}, 電圧={}%, 温度={}",
                        final_payload_bytes.len(),
                        current_time_formatted,
                        hash_str,
                        measured_data.voltage_percent,
                        temperature
                    );
                    esp_now_sender.send(&config.receiver_mac, &final_payload_bytes, 1000)?;
                    info!("画像ハッシュ、電圧情報、時刻を送信しました。");
                    // image_data はここで使用終了なので Vec<u8> を直接渡す
                    match esp_now_sender.send_image_chunks(&config.receiver_mac, image_data, 250, 5)
                    {
                        Ok(_) => {
                            info!("画像送信完了");
                            led.indicate_sending()?;
                        }
                        Err(e) => {
                            error!("画像送信エラー: {:?}", e);
                            led.blink_error()?;
                            return Err(e.into());
                        }
                    }
                }
                Err(e) => {
                    error!("ハッシュ計算エラー: {:?}", e);
                    led.blink_error()?;
                    return Err(e.into());
                }
            }
        }
        None => {
            // 画像データがない場合 (低電圧など)
            let mut final_payload_str = String::from("HASH:");
            final_payload_str.push_str(&current_time_formatted);
            final_payload_str.push_str(",");
            final_payload_str.push_str(DUMMY_HASH);
            final_payload_str.push_str(&format!(",VOLT:{}", measured_data.voltage_percent));
            final_payload_str.push_str(&format!(",TEMP:{}", temperature));
            let final_payload_bytes = final_payload_str.into_bytes();
            info!(
                "送信データ準備完了 (画像なし - ダミーハッシュ): ペイロードサイズ={}, 時刻={}, 電圧={}%, 温度={}",
                final_payload_bytes.len(),
                current_time_formatted,
                measured_data.voltage_percent,
                temperature
            );
            esp_now_sender.send(&config.receiver_mac, &final_payload_bytes, 1000)?;
            info!("ダミーハッシュ、電圧情報、時刻を送信しました (画像なし)。");
        }
    }
    Ok(())
}

/// アプリケーションのメインエントリーポイント
fn main() -> anyhow::Result<()> {
    // ESP-IDFの各種初期化
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let loop_start_time = Instant::now();
    let min_sleep_duration = Duration::from_secs(1);

    let app_config = match AppConfig::load() {
        Ok(cfg) => Arc::new(cfg),
        Err(e) => {
            error!("設定ファイルの読み込みに失敗しました: {}", e);
            panic!("設定ファイルの読み込みエラー: {}", e);
        }
    };

    info!("ペリフェラルを初期化しています");
    let peripherals_all = Peripherals::take().unwrap();
    let mut modem_peripheral_option = Some(peripherals_all.modem);

    let sysloop = EspSystemEventLoop::take()?;
    let nvs_default_partition = EspDefaultNvsPartition::take()?; // Renamed for clarity
    let mut nvs = EspNvs::new(nvs_default_partition.clone(), NVS_NAMESPACE, true)?; // NvsDefault is not a type, use bool for create_if_missing

    let mut led = StatusLed::new(peripherals_all.pins.gpio4)?;
    led.turn_off()?;

    let mut deep_sleep_controller = DeepSleep::new(app_config.clone(), EspIdfDeepSleep);

    // `tz` initialization is kept for now as it might be used in fallback sleep logic.
    let tz: chrono_tz::Tz = app_config
        .timezone
        .parse()
        .unwrap_or(chrono_tz::Asia::Tokyo);

    // --- WiFi Initialization for ESP-NOW ---
    info!("ESP-NOW用にWiFiをSTAモードで準備します。");
    let modem_taken_for_espnow = modem_peripheral_option
        .take()
        .ok_or_else(|| anyhow::anyhow!("Modem peripheral already taken for ESP-NOW setup"))?;
    let mut wifi_for_espnow = BlockingWifi::wrap( // Renamed from wifi_instance_for_espnow
        EspWifi::new(
            modem_taken_for_espnow,
            sysloop.clone(),
            Some(nvs_default_partition.clone()),
        )?,
        sysloop.clone(),
    )?;

    wifi_for_espnow.set_configuration(&esp_idf_svc::wifi::Configuration::Client(
        esp_idf_svc::wifi::ClientConfiguration {
            ssid: "".try_into().unwrap(),
            password: "".try_into().unwrap(),
            auth_method: esp_idf_svc::wifi::AuthMethod::None,
            ..Default::default()
        },
    ))?;
    wifi_for_espnow.start()?;
    info!("WiFiがESP-NOW用にSTAモードで起動しました。");

    unsafe {
        esp_idf_svc::sys::esp_wifi_set_ps(esp_idf_svc::sys::wifi_ps_type_t_WIFI_PS_NONE);
    }
    info!("Wi-Fi Power Save を無効化しました (ESP-NOW用)");

    // --- Initialize EspNowSender ---
    let esp_now_sender = match EspNowSender::new() {
        Ok(sender) => sender,
        Err(e) => {
            error!("Failed to initialize ESP-NOW sender: {:?}", e);
            // Perform a simple deep sleep on error before panic or reset
            deep_sleep_controller.sleep_for_duration(app_config.sleep_duration_seconds)?; // Fallback sleep
            panic!("ESP-NOW init failed: {:?}", e);
        }
    };
    esp_now_sender.add_peer(&app_config.receiver_mac)?;
    info!("ESP-NOW sender initialized and peer added.");

    // --- Fallback NVS date storage (if needed for other purposes) ---
    // ... (existing comments)

    // --- WiFi Initialization for ESP-NOW ---
    info!("ESP-NOW用にWiFiをSTAモードで準備します。");
    let modem_taken_for_espnow = modem_peripheral_option
        .take()
        .ok_or_else(|| anyhow::anyhow!("Modem peripheral already taken for ESP-NOW setup"))?;
    let mut wifi_for_espnow = BlockingWifi::wrap(
        EspWifi::new(
            modem_taken_for_espnow,
            sysloop.clone(),
            Some(nvs_default_partition.clone()),
        )?,
        sysloop.clone(),
    )?;

    wifi_for_espnow.set_configuration(&esp_idf_svc::wifi::Configuration::Client(
        esp_idf_svc::wifi::ClientConfiguration {
            ssid: "".try_into().unwrap(),
            password: "".try_into().unwrap(),
            auth_method: esp_idf_svc::wifi::AuthMethod::None,
            ..Default::default()
        },
    ))?;
    wifi_for_espnow.start()?;
    info!("WiFiがESP-NOW用にSTAモードで起動しました。");

    unsafe {
        esp_idf_svc::sys::esp_wifi_set_ps(esp_idf_svc::sys::wifi_ps_type_t_WIFI_PS_NONE);
    }
    info!("Wi-Fi Power Save を無効化しました (ESP-NOW用)");

    // --- Initialize EspNowSender ---
    let esp_now_sender = match EspNowSender::new() {
        Ok(sender) => sender,
        Err(e) => {
            error!("Failed to initialize ESP-NOW sender: {:?}", e);
            deep_sleep_controller.sleep_for_duration(app_config.sleep_duration_seconds)?;
            panic!("ESP-NOW init failed: {:?}", e);
        }
    };
    esp_now_sender.add_peer(&app_config.receiver_mac)?;
    info!("ESP-NOW sender initialized and peer added.");

    // --- ADC2 を初期化 ---
    info!("ADC2を初期化しています (GPIO0)");
    let adc2 = AdcDriver::new(peripherals_all.adc2)?;
    let adc_config = AdcChannelConfig {
        attenuation: DB_12,
        calibration: Calibration::Line,
        ..Default::default()
    };
    let mut adc2_ch1 = AdcChannelDriver::new(&adc2, peripherals_all.pins.gpio0, &adc_config)?;

    // --- 電圧測定 & パーセンテージ計算 ---
    info!("電圧を測定しパーセンテージを計算します...");
    let mut measured_voltage_percent: u8 = u8::MAX;
    match adc2_ch1.read() {
        Ok(voltage_mv_u16) => {
            let voltage_mv = voltage_mv_u16 as f32;
            info!("電圧測定成功: {:.0} mV", voltage_mv);
            let percentage = if RANGE_MV <= 0.0 {
                0.0
            } else {
                ((voltage_mv - MIN_MV) / RANGE_MV * 100.0)
                    .max(0.0)
                    .min(100.0)
            };
            measured_voltage_percent = percentage.round() as u8;
            info!("計算されたパーセンテージ: {} %", measured_voltage_percent);
        }
        Err(e) => {
            error!("ADC読み取りエラー: {:?}. 電圧は255%として扱います。", e);
            measured_voltage_percent = 255;
        }
    }
    drop(adc2_ch1);
    drop(adc2);

    // --- 画像取得タスク ---
    let mut image_data_option: Option<Vec<u8>> = None;

    if measured_voltage_percent >= LOW_VOLTAGE_THRESHOLD_PERCENT {
        info!(
            "電圧 {}% (>= {}%) は十分なため、カメラを初期化し画像をキャプチャします。",
            measured_voltage_percent, LOW_VOLTAGE_THRESHOLD_PERCENT
        );

        let camera_config = camera::M5UnitCamConfig {
            frame_size: M5UnitCamConfig::from_string(&app_config.frame_size),
        };

        let camera = CameraController::new(
            peripherals_all.pins.gpio27, // clock
            peripherals_all.pins.gpio32, // d0
            peripherals_all.pins.gpio35, // d1
            peripherals_all.pins.gpio34, // d2
            peripherals_all.pins.gpio5,  // d3
            peripherals_all.pins.gpio39, // d4
            peripherals_all.pins.gpio18, // d5
            peripherals_all.pins.gpio36, // d6
            peripherals_all.pins.gpio19, // d7
            peripherals_all.pins.gpio22, // vsync
            peripherals_all.pins.gpio26, // href
            peripherals_all.pins.gpio21, // pclk
            peripherals_all.pins.gpio25, // sda
            peripherals_all.pins.gpio23, // scl
            camera_config,
        )?;

        let current_aec_value = camera.get_current_aec_value();
        let _ = camera.configure_exposure(app_config.auto_exposure_enabled, Some(current_aec_value));
        
        if let Some(warmup_frames) = app_config.camera_warmup_frames {
            info!("カメラウォームアップフレーム数: {}", warmup_frames);
            for _ in 0..warmup_frames {
                match camera.capture_image() {
                    Ok(_) => {
                        info!("カメラウォームアップフレームキャプチャ成功");
                    }
                    Err(e) => {
                        error!("カメラウォームアップフレームキャプチャ失敗: {:?}", e);
                        led.blink_error()?;
                    }
                }
                FreeRtos::delay_ms(1000);
            }
        }

        match camera.capture_image() {
            Ok(fb) => {
                info!("画像キャプチャ成功: {} バイト", fb.data().len());
                image_data_option = Some(fb.data().to_vec());
            }
            Err(e) => {
                error!("画像キャプチャ失敗 (最終): {:?}", e);
                led.blink_error()?;
            }
        };
    } else {
        info!(
            "電圧が低い ({}% < {}%) ため、カメラ処理をスキップします。",
            measured_voltage_percent, LOW_VOLTAGE_THRESHOLD_PERCENT
        );
        led.blink_error()?;
    };
    
    // DS18B20 温度センサーの初期化
    let mut temp_sensor = TempSensor::new(17, 16, peripherals_all.rmt.channel0)?;
    info!("DS18B20 温度センサーを初期化しました。");
    let temperature_celsius_option: Option<f32> = match temp_sensor.read_temperature() {
        Ok(t) => Some(t),
        Err(e) => {
            warn!("DS18B20 温度センサーの読み取りに失敗しました: {:?}", e);
            None
        }
    };
    info!("DS18B20 温度センサーの読み取り結果: {:.2}°C", temperature_celsius_option.unwrap_or(-999.0));

    // --- データ送信タスク ---
    info!("データ送信タスクを開始します");
    let measured_data = MeasuredData::new(
        measured_voltage_percent,
        image_data_option,
        temperature_celsius_option,
    );
    if let Err(e) = transmit_data_task(
        &app_config,
        &esp_now_sender,
        &mut led,
        measured_data,
    ) {
        error!("データ送信タスクでエラーが発生しました: {:?}", e);
    }

    led.turn_off()?;

    // --- スリープ処理 ---
    let mut use_fallback_sleep = true;
    info!("Attempting to receive sleep command from server...");
    match esp_now_sender.receive_sleep_command(2000) {
        Ok(Some(duration_seconds)) => {
            if duration_seconds > 0 {
                info!("Received sleep duration from server: {} seconds. Entering deep sleep.", duration_seconds);
                deep_sleep_controller.sleep_for_duration(duration_seconds as u64)?;
                use_fallback_sleep = false;
            } else {
                warn!("Received invalid sleep duration (0 seconds) from server. Using fallback.");
            }
        }
        Ok(None) => {
            warn!("Sleep command received but data was None. Using fallback.");
        }
        Err(e) => {
            warn!("Failed to receive sleep command or timeout: {:?}. Using fallback.", e);
        }
    }

    if use_fallback_sleep {
        info!("Using fallback sleep logic.");
        let elapsed_time_after_tx = loop_start_time.elapsed();
        info!("Main loop processing time (before fallback sleep): {:?}", elapsed_time_after_tx);
        if measured_voltage_percent == 0 {
            let current_hour = Utc::now().with_timezone(&tz).hour();
            if current_hour < 12 {
                info!("Voltage is 0%. Medium sleep: {}s.", app_config.sleep_duration_seconds_for_medium);
                deep_sleep_controller.sleep_for_duration(app_config.sleep_duration_seconds_for_medium)?;
            } else {
                info!("Voltage is 0%. Long sleep: {}s.", app_config.sleep_duration_seconds_for_long);
                deep_sleep_controller.sleep_for_duration(app_config.sleep_duration_seconds_for_long)?;
            }
        } else {
            info!("Fallback: Normal interval sleep. Elapsed: {:?}", elapsed_time_after_tx);
            deep_sleep_controller.sleep(elapsed_time_after_tx, min_sleep_duration)?;
        }
    }

    Ok(())
}
// The duplicated block below has been removed.
