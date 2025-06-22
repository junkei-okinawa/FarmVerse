use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripherals::Peripherals,
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
use communication::{NetworkManager, esp_now::{EspNowSender, EspNowReceiver}};
use core::{AppController, AppConfig, DataService, MeasuredData, RtcManager};
use hardware::{CameraPins, VoltageSensor};
use hardware::camera::M5UnitCamConfig;
use hardware::led::StatusLed;
use log::{error, info};
use power::sleep::{DeepSleep, EspIdfDeepSleep};

/// アプリケーションのメインエントリーポイント
fn main() -> anyhow::Result<()> {
    // ESP-IDFの基本初期化
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    // 設定ファイル読み込み
    let app_config = Arc::new(AppConfig::load().map_err(|e| {
        error!("設定ファイルの読み込みに失敗しました: {}", e);
        anyhow::anyhow!("設定ファイルの読み込みエラー: {}", e)
    })?);

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

    // ディープスリープコントローラーの初期化
    let deep_sleep_controller = DeepSleep::new(app_config.clone(), EspIdfDeepSleep);

    // タイムゾーン設定
    let timezone = app_config
        .timezone
        .parse()
        .unwrap_or(chrono_tz::Asia::Tokyo);

    // RTCタイム管理
    RtcManager::check_and_initialize_rtc(&timezone, &deep_sleep_controller)?;
    
    // ADC電圧測定
    let voltage_percent = VoltageSensor::measure_voltage_percentage(
        peripherals.adc2,
        voltage_pin,
    )?;

    // ネットワーク（WiFi）初期化
    let _wifi_connection = NetworkManager::initialize_wifi_for_esp_now(
        peripherals.modem,
        &sysloop,
        &nvs_partition,
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

    // ESP-NOW初期化（WiFi初期化完了後）
    info!("ESP-NOWセンダーを初期化中...");
    let (esp_now_arc, esp_now_receiver) = NetworkManager::initialize_esp_now(&_wifi_connection).map_err(|e| {
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
    
    info!("ESP-NOW sender initialized and peer added. Receiver MAC: {}", app_config.receiver_mac);

    // デバイス情報の表示
    info!("=== デバイス情報 ===");
    
    // 実際のMACアドレスを取得・表示
    let wifi_mac = unsafe {
        let mut mac = [0u8; 6];
        let result = esp_idf_sys::esp_wifi_get_mac(esp_idf_sys::wifi_interface_t_WIFI_IF_STA, mac.as_mut_ptr());
        if result == 0 {
            format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", 
                    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5])
        } else {
            "UNKNOWN".to_string()
        }
    };
    info!("実際のWiFi STA MAC: {}", wifi_mac);
    
    // WiFiチャンネル情報を取得・表示
    let wifi_channel = unsafe {
        let mut primary = 0u8;
        let mut second = 0;
        let result = esp_idf_sys::esp_wifi_get_channel(&mut primary, &mut second);
        if result == 0 {
            format!("Primary: {}, Secondary: {}", primary, second)
        } else {
            "UNKNOWN".to_string()
        }
    };
    info!("WiFiチャンネル: {}", wifi_channel);
    
    info!("設定されている受信先MAC: {}", app_config.receiver_mac);
    info!("設定されているスリープ時間: {}秒", app_config.sleep_duration_seconds);

    // カメラ用ピンの準備
    let camera_pins = CameraPins::new(
        pins.gpio27, pins.gpio32, pins.gpio35, pins.gpio34,
        pins.gpio5, pins.gpio39, pins.gpio18, pins.gpio36,
        pins.gpio19, pins.gpio22, pins.gpio26, pins.gpio21,
        pins.gpio25, pins.gpio23,
    );

    // 画像キャプチャ（電圧に基づく条件付き）
    let image_data = DataService::capture_image_if_voltage_sufficient(
        voltage_percent,
        camera_pins,
        &app_config,
        &mut led,
    )?;

    // 測定データの準備と送信
    info!("データ送信タスクを開始します");
    let measured_data = MeasuredData::new(voltage_percent, image_data);
    
    if let Err(e) = DataService::transmit_data(
        &app_config,
        &esp_now_sender,
        &mut led,
        measured_data,
    ) {
        error!("データ送信タスクでエラーが発生しました: {:?}", e);
    }

    // LEDをオフにする
    led.turn_off()?;

    // スリープ管理（サーバーからのコマンド待機）
    AppController::handle_sleep_with_server_command(
        &esp_now_receiver,
        &deep_sleep_controller,
        &app_config,
    )?;

    Ok(())
}
