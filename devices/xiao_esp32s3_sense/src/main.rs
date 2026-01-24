use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::peripherals::Peripherals,
    nvs::EspDefaultNvsPartition,
};
use std::sync::Arc;

// 内部モジュール
mod communication;
mod config;
mod core;
mod hardware;
mod mac_address;
mod power;
mod utils;

// 使用するモジュールのインポート
use communication::{NetworkManager, esp_now::{EspNowSender}};
use config::AppConfig;
use core::{AppController, DataService, MeasuredData, RtcManager};
use hardware::{CameraPins, VoltageSensor, TempSensor, EcTdsSensor};
use hardware::led::StatusLed;
use log::{error, info, warn};
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
    let led_pin = pins.gpio21;
    let voltage_pin = pins.gpio4; // D3

    // RMTチャンネルを分離（温度センサー用）
    let rmt_channel = peripherals.rmt.channel0;

    // ステータスLEDの初期化
    let mut led = StatusLed::new(led_pin)?;
    led.turn_off()?;

    // ディープスリープコントローラーの初期化
    let deep_sleep_controller = DeepSleep::new(EspIdfDeepSleep);

    // タイムゾーン設定
    let timezone = app_config
        .timezone
        .parse()
        .unwrap_or(chrono_tz::Asia::Tokyo);

    // RTCタイム管理
    RtcManager::check_and_initialize_rtc(&timezone, &deep_sleep_controller)?;
    
    // ADC電圧測定 ADC1 は使用後に所有権が解放され、後続処理で利用可能になる。
    let (voltage_percent, adc1) = VoltageSensor::measure_voltage_percentage(
        peripherals.adc1,
        voltage_pin,
    )?;

    // 低電圧チェック (要件: 3.3V=0%以下ならDeepSleep 10分)
    // voltage_sensor.rsの実装により、min_mv (3300mV) 以下は 0% となる
    if voltage_percent == 0 {
        warn!("バッテリー電圧が低下しています (0%)。処理をスキップしてDeepSleepに入ります。");
        
        // 安全のためLEDを消灯
        led.turn_off()?;
        
        // 10分間 (600秒) のDeepSleepに入る
        let sleep_duration = std::time::Duration::from_secs(600);
        info!("DeepSleepに入ります: {}秒", sleep_duration.as_secs());
        
        deep_sleep_controller.enter_deep_sleep(
            sleep_duration,
            app_config.sleep_compensation_micros,
        );
        
        // DeepSleepに入るとここには戻らない
        return Ok(());
    }

    info!("設定されている受信先MAC: {}", app_config.receiver_mac);
    info!("設定されているスリープ時間: {}秒", app_config.sleep_duration_seconds);

    // センサー測定の実行
    let mut measured_data = MeasuredData::new(voltage_percent, None);

    // 温度センサーの初期化（設定が有効な場合）
    let mut temp_sensor = if app_config.temp_sensor_enabled {
        info!("温度センサーを初期化中...");
        match TempSensor::new(
            app_config.temp_sensor_power_pin,
            app_config.temp_sensor_data_pin,
            app_config.temperature_offset_celsius,
            rmt_channel,
        ) {
            Ok(sensor) => {
                info!("✓ 温度センサーの初期化に成功: {}", sensor.get_info());
                Some(sensor)
            }
            Err(e) => {
                warn!("温度センサーの初期化に失敗: {:?}", e);
                warn!("温度センサーなしで続行します");
                None
            }
        }
    } else {
        info!("温度センサーは設定で無効化されています");
        None
    };

    // 温度測定（利用可能な場合）
    if let Some(ref mut sensor) = temp_sensor {
        match sensor.read_temperature() {
            Ok(reading) => {
                info!("🌡️ 温度測定結果: {:.1}°C (補正済み)", reading.corrected_temperature_celsius);
                measured_data = measured_data.with_temperature(Some(reading.corrected_temperature_celsius));
                
                if let Some(ref warning) = reading.warning_message {
                    measured_data.add_warning(format!("温度センサー: {}", warning));
                }
            }
            Err(e) => {
                warn!("温度測定に失敗: {:?}", e);
                measured_data.add_warning("温度測定に失敗しました".to_string());
            }
        }
    } else {
        info!("温度センサーが利用できません");
    }

    // EC/TDSセンサーの初期化（設定が有効な場合、電圧測定後のADC1を使用）
    let mut ec_tds_sensor = if app_config.tds_sensor_enabled {
        info!("EC/TDSセンサーを初期化中...");
        
        match EcTdsSensor::new(
            app_config.tds_sensor_power_pin,
            1, // GPIO1固定（ADC1対応、WiFi競合回避）
            app_config.tds_factor,
            app_config.tds_calibrate_reference_adc,
            app_config.tds_calibrate_reference_ec,
            app_config.tds_temp_coefficient,
            pins.gpio1,
            adc1, // ADC1を再利用
        ) {
            Ok(sensor) => {
                info!("✓ EC/TDSセンサーの初期化に成功: {}", sensor.get_info());
                Some(sensor)
            }
            Err(e) => {
                warn!("EC/TDSセンサーの初期化に失敗: {:?}", e);
                warn!("EC/TDSセンサーなしで続行します");
                None
            }
        }
    } else {
        info!("EC/TDSセンサーは設定で無効化されています");
        None
    };

    // EC/TDS測定（利用可能な場合）
    if let Some(ref mut sensor) = ec_tds_sensor {
        // 温度補正のために測定済み温度を使用
        let temp_for_compensation = measured_data.temperature_celsius;

        match sensor.read_voltage(app_config.tds_measurement_samples, 10) {
            Ok(Some(voltage)) => {
                info!("✓ EC/TDSセンサーの電圧測定成功: {:.2} V", voltage);
                measured_data = measured_data.with_tds_voltage(Some(voltage));
            }
            Ok(None) => {
                warn!("EC/TDSセンサーの電圧測定結果がNoneです");
            }
            Err(e) => {
                warn!("EC/TDSセンサーの電圧測定エラー: {:?}", e);
            }
        }
    } else {
        info!("EC/TDSセンサーが利用できません");
    }

    info!("=== 測定結果サマリ ===");
    info!("{}", measured_data.get_summary());
    if !measured_data.sensor_warnings.is_empty() {
        warn!("センサー警告: {:?}", measured_data.sensor_warnings);
    }

    // カメラ用ピンの準備
    let camera_pins = CameraPins::new(
        pins.gpio10, // clock
        pins.gpio15, // d0
        pins.gpio17, // d1
        pins.gpio18, // d2
        pins.gpio16, // d3
        pins.gpio14, // d4
        pins.gpio12, // d5
        pins.gpio11, // d6
        pins.gpio48, // d7
        pins.gpio38, // vsync
        pins.gpio47, // href
        pins.gpio13, // pclk
        pins.gpio40, // sda
        pins.gpio39, // scl
    );

    // 画像キャプチャ（電圧に基づく条件付き）
    let image_data = DataService::capture_image_if_voltage_sufficient(
        voltage_percent,
        camera_pins,
        &app_config,
        &mut led,
    )?;

    // 画像データを測定データに追加
    measured_data.image_data = image_data;

    // 測定データの送信
    info!("データ送信タスクを開始します");
    info!("送信データサマリ: {}", measured_data.get_summary());

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
