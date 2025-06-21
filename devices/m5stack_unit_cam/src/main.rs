use chrono::{DateTime, Datelike, NaiveDate, Utc};
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
    nvs::EspDefaultNvsPartition,
    wifi::{BlockingWifi, EspWifi},
};
use std::sync::Arc;

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
use sleep::{DeepSleep, EspIdfDeepSleep};

const DUMMY_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000"; // 64 zeros for SHA256 dummy

// --- 電圧測定用の定数 ---
const MIN_MV: f32 = 128.0; // UnitCam GPIO0 の実測値に合わせて調整
const MAX_MV: f32 = 3130.0; // UnitCam GPIO0 の実測値に合わせて調整
const RANGE_MV: f32 = MAX_MV - MIN_MV;
const LOW_VOLTAGE_THRESHOLD_PERCENT: u8 = 8;

// --- ここまで 定数 ---

struct MeasuredData {
    voltage_percent: u8,
    image_data_option: Option<Vec<u8>>,
}

impl MeasuredData {
    fn new(voltage_percent: u8, image_data_option: Option<Vec<u8>>) -> Self {
        Self {
            voltage_percent,
            image_data_option,
        }
    }
}

// --- transmit_data_task (modified signature) ---
fn transmit_data_task(
    config: &AppConfig,
    esp_now_sender: &EspNowSender,
    led: &mut StatusLed,
    measured_data: MeasuredData,
) -> anyhow::Result<()> {
    // Prepare timestamp string
    let tz_task: chrono_tz::Tz = config.timezone.parse().unwrap_or(chrono_tz::Asia::Tokyo);
    let current_time_formatted = Utc::now()
        .with_timezone(&tz_task)
        .format("%Y/%m/%d %H:%M:%S%.3f")
        .to_string();

    match measured_data.image_data_option {
        Some(image_data) => {
            // image_data は Vec<u8>
            match ImageFrame::calculate_hash(&image_data) {
                Ok(hash_str) => {
                    let mut final_payload_str = String::from("HASH:");
                    final_payload_str.push_str(&hash_str);
                    final_payload_str.push_str(&format!(",VOLT:{}", measured_data.voltage_percent));
                    final_payload_str.push_str(",");
                    final_payload_str.push_str(&current_time_formatted);
                    let final_payload_bytes = final_payload_str.into_bytes();

                    info!(
                        "送信データ準備完了 (画像あり): ペイロードサイズ={}, 時刻={}, ハッシュ={}, 電圧={}%",
                        final_payload_bytes.len(),
                        current_time_formatted,
                        hash_str,
                        measured_data.voltage_percent
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
            let final_payload_bytes = final_payload_str.into_bytes();
            info!(
                "送信データ準備完了 (画像なし - ダミーハッシュ): ペイロードサイズ={}, 時刻={}, 電圧={}%",
                final_payload_bytes.len(),
                current_time_formatted,
                measured_data.voltage_percent
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
    let nvs_default_partition = EspDefaultNvsPartition::take()?;

    let mut led = StatusLed::new(peripherals_all.pins.gpio4)?;
    led.turn_off()?;

    let deep_sleep_controller = DeepSleep::new(app_config.clone(), EspIdfDeepSleep);

    let tz: chrono_tz::Tz = app_config
        .timezone
        .parse()
        .unwrap_or(chrono_tz::Asia::Tokyo);

    // --- RTC Time Check ---
    info!("RTCの現在時刻をチェックしています...");
    let current_time = Utc::now().with_timezone(&tz);
    if current_time.year() < 2025 {
        info!("RTCの現在時刻が2025年以前です。RTCを2025年1月1日に設定し、1秒スリープします。");
        // RTC時刻を2025年1月1日 00:00:00に設定
        let target_date = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap();
        let target_utc: DateTime<Utc> = DateTime::from_naive_utc_and_offset(target_date, Utc);
        
        // ESP32のRTC時刻設定
        unsafe {
            let timestamp_seconds = target_utc.timestamp();
            // Use esp_sntp_set_time instead of esp_rtc_time_set
            let tv_sec = timestamp_seconds;
            let tv_usec = 0;
            let tv = esp_idf_svc::sys::timeval { tv_sec, tv_usec };
            esp_idf_svc::sys::settimeofday(&tv, std::ptr::null());
        }
        info!("RTCを2025年1月1日に設定しました。1秒間スリープします。");
        deep_sleep_controller.sleep_for_duration(1)?;
    }
    info!("RTC時刻チェック完了。処理を続行します。");

    // --- WiFi Initialization for ESP-NOW ---
    info!("ESP-NOW用にWiFiをSTAモードで準備します。");
    let modem_taken_for_espnow = modem_peripheral_option
        .take()
        .ok_or_else(|| anyhow::anyhow!("Modem peripheral already taken for ESP-NOW setup"))?;
    let mut _wifi_for_espnow = BlockingWifi::wrap(
        EspWifi::new(
            modem_taken_for_espnow,
            sysloop.clone(),
            Some(nvs_default_partition.clone()),
        )?,
        sysloop.clone(),
    )?;

    _wifi_for_espnow.set_configuration(&esp_idf_svc::wifi::Configuration::Client(
        esp_idf_svc::wifi::ClientConfiguration {
            ssid: "".try_into().unwrap(),
            password: "".try_into().unwrap(),
            auth_method: esp_idf_svc::wifi::AuthMethod::None,
            ..Default::default()
        },
    ))?;
    _wifi_for_espnow.start()?;
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
    #[allow(unused_assignments)]
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

    // --- 画像取得タスク (低電圧時・エラー時はスキップ) ---
    let mut image_data_option: Option<Vec<u8>> = None;

    if measured_voltage_percent >= LOW_VOLTAGE_THRESHOLD_PERCENT && measured_voltage_percent != 255 {
        info!(
            "電圧 {}% (>= {}% かつ != 255%) は十分なため、カメラを初期化し画像をキャプチャします。",
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
        if measured_voltage_percent == 255 {
            info!("電圧測定エラー (255%) のため、カメラ処理をスキップします。");
        } else {
            info!(
                "電圧が低い ({}% < {}%) ため、カメラ処理をスキップします。",
                measured_voltage_percent, LOW_VOLTAGE_THRESHOLD_PERCENT
            );
        }
        led.blink_error()?;
    }

    // --- データ送信タスク ---
    info!("データ送信タスクを開始します");
    let measured_data = MeasuredData::new(
        measured_voltage_percent,
        image_data_option,
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
    info!("サーバーからのスリープ時間を受信中...");
    match esp_now_sender.receive_sleep_command(2000) {
        Some(duration_seconds) => {
            if duration_seconds > 0 {
                info!("サーバーからスリープ時間を受信: {}秒。ディープスリープに入ります。", duration_seconds);
                deep_sleep_controller.sleep_for_duration(duration_seconds as u64)?;
            } else {
                warn!("無効なスリープ時間 (0秒) を受信。デフォルト時間を使用します。");
                deep_sleep_controller.sleep_for_duration(app_config.sleep_duration_seconds)?;
            }
        }
        None => {
            warn!("スリープコマンドを受信できませんでした。デフォルト時間を使用します。");
            deep_sleep_controller.sleep_for_duration(app_config.sleep_duration_seconds)?;
        }
    }

    Ok(())
}
