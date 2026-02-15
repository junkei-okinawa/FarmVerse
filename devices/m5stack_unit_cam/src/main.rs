use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripherals::Peripherals,
    hal::reset::ResetReason,
    nvs::EspDefaultNvsPartition,
};
use std::sync::Arc;

// 内部モジュール
mod communication;
mod core;
mod hardware;
mod mac_address;
mod power;

// 使用するモジュールのインポート
use communication::{NetworkManager, esp_now::EspNowSender};
use core::{AppController, AppConfig, DataService, MeasuredData, RtcManager};
use hardware::camera::{CameraController, M5UnitCamConfig};
use hardware::VoltageSensor;
use hardware::led::StatusLed;
use log::{error, info, warn};
use power::sleep::{DeepSleep, EspIdfDeepSleep};

/// アプリケーションのメインエントリーポイント
fn main() -> anyhow::Result<()> {
    // ESP-IDFの基本初期化
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    if cfg!(feature = "qemu-smoke") {
        info!("QEMU smoke mode enabled");
        println!("QEMU_SMOKE_PASS");
        return Ok(());
    }

    // 設定ファイル読み込み
    let app_config = Arc::new(AppConfig::load().map_err(|e| {
        error!("設定ファイルの読み込みに失敗しました: {}", e);
        anyhow::anyhow!("設定ファイルの読み込みエラー: {}", e)
    })?);
    if app_config.debug_mode {
        info!("debug mode enabled");
    }

    // ペリフェラルとシステムリソースの初期化
    info!("ペリフェラルを初期化しています");
    let peripherals = Peripherals::take().unwrap();
    let sysloop = EspSystemEventLoop::take()?;
    let nvs_partition = EspDefaultNvsPartition::take()?;

    // 必要なピンを先に抽出
    let pins = peripherals.pins;
    let led_pin = pins.gpio4;
    let voltage_pin = pins.gpio0;

    // ステータスLEDの初期化
    let mut led = StatusLed::new(led_pin)?;
    led.turn_off()?;

    // スリープコントローラーの初期化
    let deep_sleep_controller = DeepSleep::new(app_config.clone(), EspIdfDeepSleep);

    // タイムゾーン設定
    let timezone = app_config
        .timezone
        .parse()
        .unwrap_or(chrono_tz::Asia::Tokyo);

    // RTCタイム管理
    RtcManager::check_and_initialize_rtc(&timezone, &deep_sleep_controller)?;
    
    info!("設定されている受信先MAC: {}", app_config.receiver_mac);
    info!("設定されているスリープ時間: {}秒", app_config.sleep_duration_seconds);
    let reset_reason = ResetReason::get();
    info!("リセット理由: {:?}", reset_reason);

    let mut adc2 = peripherals.adc2;
    let mut gpio0 = voltage_pin;
    let mut last_valid_voltage_percent: Option<u8> = None;

    // 起動直後はOV2640のSCCB応答が不安定な場合があるため待機する
    info!("カメラ電源安定化待ち: 1000ms");
    esp_idf_svc::hal::delay::FreeRtos::delay_ms(1000);

    let camera = CameraController::new(
        pins.gpio15,
        pins.gpio27,
        pins.gpio32,
        pins.gpio35,
        pins.gpio34,
        pins.gpio5,
        pins.gpio39,
        pins.gpio18,
        pins.gpio36,
        pins.gpio19,
        pins.gpio22,
        pins.gpio26,
        pins.gpio21,
        pins.gpio25,
        pins.gpio23,
        M5UnitCamConfig::default(),
    );
    let camera = match camera {
        Ok(camera) => Some(camera),
        Err(e) => {
            error!(
                "カメラ初期化失敗。再書き込み直後は Unit Cam の電源を一度抜き差しして再起動してください: {:?}",
                e
            );
            if app_config.force_camera_test || app_config.bypass_voltage_threshold {
                return Err(anyhow::anyhow!("カメラ初期化に失敗しました: {:?}", e));
            }
            warn!("カメラ初期化に失敗しました。画像なしで継続します: {:?}", e);
            None
        }
    };

    // WiFi起動前に一度だけADC2を読み、以降のサイクルでのフォールバックに使う
    let (initial_voltage_percent, returned_adc2, returned_gpio0) =
        VoltageSensor::measure_voltage_percentage(adc2, gpio0)?;
    adc2 = returned_adc2;
    gpio0 = returned_gpio0;
    if initial_voltage_percent < crate::core::INVALID_VOLTAGE_PERCENT {
        last_valid_voltage_percent = Some(initial_voltage_percent);
    }

    let wifi_connection = NetworkManager::initialize_wifi_for_esp_now(
        peripherals.modem,
        &sysloop,
        &nvs_partition,
        app_config.wifi_tx_power_dbm,
    ).map_err(|e| {
        if let Err(sleep_err) = AppController::fallback_sleep(
            &deep_sleep_controller,
            &app_config,
            &format!("WiFi初期化に失敗: {:?}", e),
        ) {
            log::error!("Deep sleep failed: {:?}", sleep_err);
        }
        e
    })?;

    loop {
        // ADC電圧測定
        let (measured_voltage_percent, returned_adc2, returned_gpio0) =
            VoltageSensor::measure_voltage_percentage(adc2, gpio0)?;
        adc2 = returned_adc2;
        gpio0 = returned_gpio0;

        let voltage_percent = if measured_voltage_percent < crate::core::INVALID_VOLTAGE_PERCENT {
            last_valid_voltage_percent = Some(measured_voltage_percent);
            measured_voltage_percent
        } else if let Some(last_good) = last_valid_voltage_percent {
            warn!(
                "ADC2読み取りが無効値(255%)のため、直近の有効値 {}% を使用します（WiFi競合対策）",
                last_good
            );
            last_good
        } else {
            measured_voltage_percent
        };

        // 画像キャプチャ（短いリトライ付き）
        let mut capture_result = None;
        let mut last_capture_err = None;
        for attempt in 1..=3 {
            match DataService::capture_image_if_voltage_sufficient(
                voltage_percent,
                camera.as_ref(),
                &app_config,
                &mut led,
            ) {
                Ok(data) => {
                    capture_result = Some(data);
                    break;
                }
                Err(e) => {
                    error!("カメラ処理に失敗しました (attempt {}/3): {:?}", attempt, e);
                    last_capture_err = Some(e);
                    if attempt < 3 {
                        esp_idf_svc::hal::delay::FreeRtos::delay_ms(250);
                    }
                }
            }
        }

        let image_data = match capture_result {
            Some(data) => data,
            None => {
                if app_config.force_camera_test || app_config.bypass_voltage_threshold {
                    return Err(anyhow::anyhow!(
                        "カメラキャプチャに失敗したため送信を中止します: {:?}",
                        last_capture_err
                    ));
                }
                warn!("カメラ処理に失敗したため画像なしで継続します");
                None
            }
        };
        info!("データ送信タスクを開始します");
        let measured_data = MeasuredData::new(voltage_percent, image_data);

        // ESP-NOWはサイクルごとに再初期化して内部TXキューをクリーンに保つ
        info!("ESP-NOWセンダーを初期化中...");
        let (esp_now_arc, esp_now_receiver) = NetworkManager::initialize_esp_now(&wifi_connection).map_err(|e| {
            log::error!("ESP-NOW初期化に失敗: {:?}", e);
            if let Err(sleep_err) = AppController::fallback_sleep(
                &deep_sleep_controller,
                &app_config,
                &format!("ESP-NOW初期化に失敗: {:?}", e),
            ) {
                log::error!("Deep sleep failed: {:?}", sleep_err);
            }
            anyhow::anyhow!("ESP-NOW初期化に失敗: {:?}", e)
        })?;

        let esp_now_sender = EspNowSender::new(esp_now_arc, app_config.receiver_mac.clone()).map_err(|e| {
            log::error!("ESP-NOWセンダー初期化に失敗: {:?}", e);
            if let Err(sleep_err) = AppController::fallback_sleep(
                &deep_sleep_controller,
                &app_config,
                &format!("ESP-NOWセンダー初期化に失敗: {:?}", e),
            ) {
                log::error!("Deep sleep failed: {:?}", sleep_err);
            }
            anyhow::anyhow!("ESP-NOWセンダー初期化に失敗: {:?}", e)
        })?;

        if let Err(e) = DataService::transmit_data(
            &app_config,
            &esp_now_sender,
            &mut led,
            measured_data,
        ) {
            error!("データ送信タスクでエラーが発生しました: {:?}", e);
        }

        led.turn_off()?;

        // スリープ管理（サーバーからのコマンド待機）
        let sleep_duration_sec = AppController::resolve_sleep_duration(&esp_now_receiver, &app_config)?;

        // 省電力要件: DeepSleep前にSCCBスタンバイへ移行する。
        if app_config.camera_soft_standby_enabled {
            if let Some(cam) = camera.as_ref() {
                let standby_result = cam.enter_deep_sleep_standby_via_sccb();
                if let Err(e) = standby_result {
                    warn!(
                        "Sleep前のSCCBスタンバイ移行に失敗しました（処理継続）: {:?}",
                        e
                    );
                }
            }
        }

        deep_sleep_controller.sleep_for_duration(sleep_duration_sec)?;
        break;
    }

    Ok(())
}
