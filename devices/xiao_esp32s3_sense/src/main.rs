use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripherals::Peripherals,
    nvs::EspDefaultNvsPartition,
    wifi::{BlockingWifi, EspWifi},
    espnow::EspNow,
};
use std::sync::{Arc, Mutex};

// 内部モジュール
mod communication;
mod config;
mod core;
mod hardware;
mod mac_address;
mod power;
mod utils;

// 使用するモジュールのインポート
use communication::{NetworkManager, esp_now::{EspNowSender, EspNowReceiver}};
use config::AppConfig;
use core::{AppController, DataService, MeasuredData, RtcManager};
use hardware::{CameraPins, VoltageSensor, TempSensor};
use hardware::led::StatusLed;
use log::{error, info, warn};
use power::sleep::{SleepManager, EspIdfDeepSleep, EspIdfLightSleep, SleepType};

/// アプリケーションのメインエントリーポイント
fn main() -> anyhow::Result<()> {
    // ESP-IDFの基本初期化
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    // [PHASE 8] スリープ中に固定されていたピンを解放
    unsafe {
        for pin in [2, 5, 21, 10, 15, 17, 18, 16, 14, 12, 11, 48, 38, 47, 13, 40, 39] {
            esp_idf_sys::gpio_hold_dis(pin as i32);
        }
    }
    info!("✓ スリープ解除に伴い全ピンの固定(Hold)を解除しました");

    // 設定ファイル読み込み
    let app_config = Arc::new(AppConfig::load().map_err(|e| {
        error!("設定ファイルの読み込みに失敗しました: {}", e);
        anyhow::anyhow!("設定ファイルの読み込みエラー: {}", e)
    })?);

    // ペリフェラルとシステムリソースの初期化 (これらは一度だけ行う)
    info!("ペリフェラルを初期化しています");
    let peripherals = Peripherals::take().expect("Failed to take peripherals");
    let sysloop = EspSystemEventLoop::take()?;
    let nvs_partition = EspDefaultNvsPartition::take()?;

    let pins = peripherals.pins;

    // ステータスLEDの初期化 (一度だけ)
    let mut led = StatusLed::new(pins.gpio21)?;
    led.turn_off()?;
    
    if app_config.debug_mode {
        led.blink_count(1)?;
    }

    // スリープマネージャーの初期化
    let sleep_manager = SleepManager::new(EspIdfDeepSleep, EspIdfLightSleep, 600);

    // タイムゾーン設定
    let timezone = app_config.timezone.parse().unwrap_or(chrono_tz::Asia::Tokyo);

    // RTCタイム管理
    RtcManager::check_and_initialize_rtc(&timezone, &EspIdfDeepSleep)?;
    
    // WiFiリソース管理 (Light Sleep復帰後の再初期化対応)
    let mut wifi_resources: Option<(BlockingWifi<EspWifi<'static>>, Arc<Mutex<EspNow<'static>>>, EspNowReceiver)> = None;

    let mut adc1 = peripherals.adc1;
    let mut voltage_pin = pins.gpio4;
    let rmt0 = peripherals.rmt.channel0;

    info!("=== HYBRID SLEEP LOOPを開始します ===");

    loop {
        info!("ループ開始");

        // WiFi/ESP-NOWの初期化（未初期化の場合のみ）
        if wifi_resources.is_none() {
            info!("WiFi初期化を開始します");
            let wifi_conn = NetworkManager::initialize_wifi_for_esp_now(
                unsafe { std::mem::transmute_copy(&peripherals.modem) },
                &sysloop,
                &nvs_partition,
                app_config.wifi_tx_power_dbm,
                app_config.wifi_init_delay_ms,
            )?;
            
            let (esp_now_arc, receiver) = NetworkManager::initialize_esp_now(&wifi_conn)?;
            wifi_resources = Some((wifi_conn, esp_now_arc, receiver));
            info!("✓ WiFi/ESP-NOWリソースの初期化が完了しました");
        }

        // 電圧測定
        let (voltage_percent, returned_adc1, returned_vpin) = VoltageSensor::measure_voltage_percentage(
            adc1,
            voltage_pin,
        )?;
        adc1 = returned_adc1;
        voltage_pin = returned_vpin;

        /* デバッグのためスキップ
        // 低電圧チェック
        if voltage_percent <= LOW_VOLTAGE_THRESHOLD_PERCENT && !app_config.bypass_voltage_threshold {
            warn!("低電圧を検知 ({}%)。Deep Sleepを実行します。", voltage_percent);
            led.turn_off()?;
            let _ = sleep_manager.sleep_optimized(600);
        }*/

        // データ収集
        let mut measured_data = MeasuredData::new(voltage_percent, None);

        // 温度測定
        if app_config.temp_sensor_enabled {
            let channel_copy: esp_idf_svc::hal::rmt::CHANNEL0 = unsafe { std::mem::transmute_copy(&rmt0) };
            if let Ok(mut sensor) = TempSensor::new(
                app_config.temp_sensor_power_pin,
                app_config.temp_sensor_data_pin,
                app_config.temperature_offset_celsius,
                channel_copy,
            ) {
                if let Ok(reading) = sensor.read_temperature() {
                    measured_data = measured_data.with_temperature(Some(reading.corrected_temperature_celsius));
                }
                let _ = sensor.power_off();
            }
        }

        // 起動カウンタ
        let boot_count = RtcManager::get_boot_count();
        measured_data = measured_data.with_tds_voltage(Some(boot_count as f32));

        // 画像キャプチャ
        let camera_pins = unsafe {
            CameraPins::new(
                std::mem::transmute_copy(&pins.gpio10),
                std::mem::transmute_copy(&pins.gpio15),
                std::mem::transmute_copy(&pins.gpio17),
                std::mem::transmute_copy(&pins.gpio18),
                std::mem::transmute_copy(&pins.gpio16),
                std::mem::transmute_copy(&pins.gpio14),
                std::mem::transmute_copy(&pins.gpio12),
                std::mem::transmute_copy(&pins.gpio11),
                std::mem::transmute_copy(&pins.gpio48),
                std::mem::transmute_copy(&pins.gpio38),
                std::mem::transmute_copy(&pins.gpio47),
                std::mem::transmute_copy(&pins.gpio13),
                std::mem::transmute_copy(&pins.gpio40),
                std::mem::transmute_copy(&pins.gpio39),
            )
        };

        match DataService::capture_image_if_voltage_sufficient(
            voltage_percent,
            camera_pins,
            &app_config,
            &mut led,
        ) {
            Ok(image_data) => {
                measured_data.image_data = image_data;
            },
            Err(e) => {
                error!("❌ カメラ失敗: {:?}", e);
                // カメラピンの状態を安全のためにリセット（失敗時も）
                crate::hardware::camera::reset_camera_pins();
            }
        }

        // データ送信
        {
            let (_, ref esp_now_arc, _) = wifi_resources.as_ref().unwrap();
            let sender = EspNowSender::new(Arc::clone(esp_now_arc), app_config.receiver_mac.clone())?;
            info!("データ送信中...");
            let _ = DataService::transmit_data(&app_config, &sender, &mut led, measured_data);
        }

        // スリープ管理
        led.turn_off()?;
        let sleep_type = {
            let (_, _, ref receiver) = wifi_resources.as_ref().unwrap();
            AppController::handle_sleep_with_server_command(receiver, &sleep_manager, &app_config)?
        };

        if sleep_type == SleepType::Light {
            // [PHASE 11] Light Sleep復帰後、Deep Sleepと同様にピンの固定を解除する
            // これにより reset_camera_pins() で固定されたピンを再利用可能にする
            unsafe {
                for pin in [2, 5, 21, 10, 15, 17, 18, 16, 14, 12, 11, 48, 38, 47, 13, 40, 39] {
                    esp_idf_sys::gpio_hold_dis(pin as i32);
                }
            }
            info!("✓ Light Sleep復帰に伴い全ピンの固定(Hold)を解除しました");

            // 復帰確認の点滅
            unsafe {
                for _ in 0..10 {
                    esp_idf_sys::gpio_set_level(21, 0);
                    esp_idf_sys::vTaskDelay(5);
                    esp_idf_sys::gpio_set_level(21, 1);
                    esp_idf_sys::vTaskDelay(5);
                }
            }
            
            // WiFiリソースを破棄 (スリープ前にdeinitされているため)
            wifi_resources = None;
            
            RtcManager::increment_boot_count();
            info!("=== ループを継続します ===");
        } else {
            info!("Deep Sleep移行完了");
            break;
        }
    }
    Ok(())
}

const LOW_VOLTAGE_THRESHOLD_PERCENT: u8 = 30;