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

// --- 電圧測定 & パーセンテージ計算 (WiFi開始前) ---
fn transmit_data_task(
    config: &AppConfig,
    _wifi: &mut BlockingWifi<EspWifi<'static>>, // modem, sysloop, nvs を BlockingWifi に置き換え, prefixed with _
    led: &mut StatusLed,
    measured_data: MeasuredData,
) -> anyhow::Result<()> {
    unsafe {
        esp_idf_svc::sys::esp_wifi_set_ps(esp_idf_svc::sys::wifi_ps_type_t_WIFI_PS_NONE);
    }
    info!("Wi-Fi Power Save を無効化しました (ESP-NOW用)");

    let esp_now_sender = EspNowSender::new()?; // EspNowSender::new() は内部でesp_now_init()を呼ぶ
    esp_now_sender.add_peer(&config.receiver_mac)?;
    info!("ESP-NOW送信機を初期化し、ピアを追加しました");

    // Prepare timestamp string
    let tz: chrono_tz::Tz = config.timezone.parse().unwrap_or(chrono_tz::Asia::Tokyo);
    let current_time_formatted = Utc::now()
        .with_timezone(&tz)
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
    let mut wifi_option: Option<BlockingWifi<EspWifi<'static>>> = None;

    // rtcの現在時刻を取得しておく
    let tz: chrono_tz::Tz = app_config
        .timezone
        .parse()
        .unwrap_or(chrono_tz::Asia::Tokyo);
    let mut rtc_time = Utc::now().with_timezone(&tz).date_naive(); // Initialize with RTC time

    // --- NVSから最終起動日を読み込む ---
    let mut last_boot_date_str_opt: Option<String> = None;
    let mut buf = [0u8; 16]; // Buffer for date string "YYYY-MM-DD" + null
    match nvs.get_str(NVS_KEY_LAST_BOOT_DATE, &mut buf) {
        Ok(Some(date_str)) => {
            // Valid string obtained, remove null terminators for proper parsing
            if let Some(nul_pos) = date_str.find('\0') {
                last_boot_date_str_opt = Some(date_str[..nul_pos].to_string());
            } else {
                last_boot_date_str_opt = Some(date_str.to_string());
            }
            info!(
                "NVSから最終起動日を読み込みました: {:?}",
                last_boot_date_str_opt
            );
        }
        Ok(None) => {
            info!("NVSに最終起動日のエントリが見つかりません。");
            last_boot_date_str_opt = None;
        }
        Err(e) => {
            warn!(
                "NVSからの最終起動日の読み込みに失敗しました: {:?}。デフォルト値を使用します。",
                e
            );
            last_boot_date_str_opt = None; // Treat error as no date found
        }
    }

    let mut current_date_for_nvs = Utc::now().with_timezone(&tz).date_naive(); // Initialize with RTC time

    let mut sntp_performed_in_this_cycle = false;

    // --- 条件付き時刻同期処理 ---
    let time_sync_needed_by_date = deep_sleep_controller
        .is_time_sync_required()
        .unwrap_or(true);
    let mut is_first_boot_today = true; // Default to true

    // rtcの現在時刻が2025年以前の場合は電源OFFからの復旧のため時刻同期対象とする
    if rtc_time.year() < 2025 {
        info!("RTCの現在時刻が2025年以前のため、時刻同期が必要です。");
        let time_sync_needed_by_date = true;
    }

    if let Some(ref last_boot_date_str) = last_boot_date_str_opt {
        // Changed to use ref
        match NaiveDate::parse_from_str(last_boot_date_str, "%Y-%m-%d") {
            Ok(parsed_last_boot_date) => {
                // current_date_for_nvs is already NaiveDate in the correct timezone
                if parsed_last_boot_date == current_date_for_nvs {
                    is_first_boot_today = false;
                    info!(
                        "最終起動日 ({}) は今日 ({}) です。is_first_boot_today = false",
                        parsed_last_boot_date, current_date_for_nvs
                    );
                } else {
                    info!(
                        "最終起動日 ({}) は今日 ({}) ではありません。is_first_boot_today = true",
                        parsed_last_boot_date, current_date_for_nvs
                    );
                }
            }
            Err(e) => {
                warn!(
                    "NVSから読み込んだ最終起動日 '{}' のパースに失敗しました: {:?}。is_first_boot_today = true として扱います。",
                    last_boot_date_str, e
                );
                // Keep is_first_boot_today = true
            }
        }
    } else {
        info!("最終起動日の記録がNVSにないため、is_first_boot_today = true として扱います。");
        // Keep is_first_boot_today = true
    }
    info!(
        "初回起動フラグ (SNTP前): is_first_boot_today = {}",
        is_first_boot_today
    );

    if time_sync_needed_by_date || is_first_boot_today {
        info!(
            "時刻同期が必要です (日付による判断: {}, 本日初回起動: {})。",
            time_sync_needed_by_date, is_first_boot_today
        );
        led.blink_error()?; // SNTP試行中を示す点滅。エラーではないので、赤点滅ではなく青点滅にすることも検討

        let modem_taken = modem_peripheral_option
            .take()
            .ok_or_else(|| anyhow::anyhow!("Modem peripheral already taken"))?;

        let mut wifi = match BlockingWifi::wrap(
            EspWifi::new(
                modem_taken,
                sysloop.clone(),
                Some(nvs_default_partition.clone()),
            )?,
            sysloop.clone(),
        ) {
            Ok(w) => w,
            Err(e) => {
                error!("WiFiの初期化に失敗しました: {:?}", e);
                // WiFi初期化失敗時は modem を戻す試み (エラーハンドリング改善)
                return Err(e.into());
            }
        };

        match deep_sleep_controller.perform_actual_time_sync(
            &mut wifi,
            &app_config.wifi_ssid,
            &app_config.wifi_password,
        ) {
            Ok(_) => {
                info!("SNTPによる時刻同期が成功しました。");
                sntp_performed_in_this_cycle = true;
                // 同期後の現在時刻でNVS用の日付を更新
                current_date_for_nvs = Utc::now().with_timezone(&tz).date_naive();
                info!("SNTP同期後のNVS用日付: {}", current_date_for_nvs);

                // SNTP成功後、last_boot_date_str_opt を再評価して is_first_boot_today を更新
                // この時点でNVSにはまだ書き込んでいないが、ロジック上は今日の日付で判断すべき
                if let Some(ref last_boot_date_str) = &last_boot_date_str_opt {
                    // Re-borrow last_boot_date_str_opt
                    match NaiveDate::parse_from_str(last_boot_date_str, "%Y-%m-%d") {
                        Ok(parsed_last_boot_date) => {
                            if parsed_last_boot_date == current_date_for_nvs {
                                is_first_boot_today = false; // SNTP後の日付で再確認
                                info!("SNTP後再評価: 最終起動日 ({}) は今日 ({}) です。is_first_boot_today = false", parsed_last_boot_date, current_date_for_nvs);
                            } else {
                                is_first_boot_today = true; // SNTP後の日付で今日でなければ初回起動
                                info!("SNTP後再評価: 最終起動日 ({}) は今日 ({}) ではありません。is_first_boot_today = true", parsed_last_boot_date, current_date_for_nvs);
                            }
                        }
                        Err(_) => { /* パースエラーの場合は初回起動として扱う（既にログ済み） */
                        }
                    }
                } else {
                    is_first_boot_today = true; // NVSに記録がなければ初回起動
                    info!("SNTP後再評価: NVSに記録なし。is_first_boot_today = true");
                }
                info!(
                    "初回起動フラグ (SNTP後): is_first_boot_today = {}",
                    is_first_boot_today
                );
            }
            Err(e) => {
                error!("SNTPによる時刻同期に失敗しました: {:?}", e);
                // エラーを返すが、致命的ではないかもしれないので処理を続けることも検討できる
                // return Err(e.into()); // ここでリターンすると以降の処理が実行されない
                warn!("SNTP失敗後も処理を続行します。");
            }
        }
        // WiFiインスタンスをOptionに格納
        wifi_option = Some(wifi);
        led.turn_off()?; // SNTP試行終了
    } else {
        info!("時刻同期は不要です。スキップします。");
        // modem_peripheral_option は Some のままのはず
    }

    // --- NVSに現在の日付を書き込む関数 (クロージャとして定義) ---
    let store_current_date_to_nvs = |nvs_instance: &mut EspNvs<NvsDefault>,
                                        naive_date: NaiveDate| {
        let date_str_to_store = naive_date.format("%Y-%m-%d").to_string();
        match nvs_instance.set_str(NVS_KEY_LAST_BOOT_DATE, &date_str_to_store) {
            Ok(_) => info!("NVSに現在の日付を保存しました: {}", date_str_to_store),
            Err(e) => warn!("NVSへの日付の書き込みに失敗: {:?}", e),
        }
    };

    // 時刻同期が必要または本日初回起動の場合はNVSに日付を書き込んで DeepSleep する
    if time_sync_needed_by_date || is_first_boot_today {
        info!("初回起動日としてNVSに日付を書き込みます。");
        store_current_date_to_nvs(&mut nvs, current_date_for_nvs);
        // DeepSleep する
        led.turn_off()?;
        if app_config.target_digits_config.is_some() {
            info!("SNTP未実行 & ターゲットディジットモード: 通常の経過時間でスリープ");
            let elapsed_time = loop_start_time.elapsed();
            let _ = deep_sleep_controller.sleep_until_target_digits_match(elapsed_time);
        } else {
            info!("1秒後に起動するDeepSleepを開始します。");
            let _ = deep_sleep_controller.sleep_for_duration(1)?;
        }
        // ディープスリープから復帰することはないため、以降のコードは実行されない
    }

    // --- ADC2 を初期化 ---
    info!("ADC2を初期化しています (GPIO0)");
    let adc2 = AdcDriver::new(peripherals_all.adc2)?;
    let adc_config = AdcChannelConfig {
        attenuation: DB_12,
        calibration: Calibration::Line,
        ..Default::default()
    };
    let mut adc2_ch1 = AdcChannelDriver::new(&adc2, peripherals_all.pins.gpio0, &adc_config)?;
    // --- ここまで ADC2 初期化 ---

    // --- 電圧測定 & パーセンテージ計算 (WiFi開始前) ---
    info!("電圧を測定しパーセンテージを計算します (WiFi開始前)...");
    let mut measured_voltage_percent: u8 = u8::MAX; // 送信失敗時用のデフォルト値 (255%)
    match adc2_ch1.read() {
        Ok(voltage_mv_u16) => {
            let voltage_mv = voltage_mv_u16 as f32; // f32 に変換して計算
            info!("電圧測定成功: {:.0} mV", voltage_mv);
            // パーセンテージ計算 (0-100 の範囲にクランプし、u8 に丸める)
            let percentage = if RANGE_MV <= 0.0 {
                0.0
            } else {
                ((voltage_mv - MIN_MV) / RANGE_MV * 100.0)
                    .max(0.0)
                    .min(100.0)
            };
            measured_voltage_percent = percentage.round() as u8; // u8 に丸める
            info!("計算されたパーセンテージ: {} %", measured_voltage_percent);
        }
        Err(e) => {
            error!("ADC読み取りエラー: {:?}. 電圧は255%として扱います。", e);
            // エラーでも続行するが、パーセンテージは255として扱う
            measured_voltage_percent = 255;
        }
    }
    // ADCドライバはこの後不要になるので、ここでドロップ
    drop(adc2_ch1);
    drop(adc2);
    // --- 電圧測定ここまで ---

    // --- 画像取得タスク (低電圧時はスキップ) ---
    let mut image_data_option: Option<Vec<u8>> = None; // Option<Vec<u8>> に変更

    if measured_voltage_percent >= LOW_VOLTAGE_THRESHOLD_PERCENT {
        info!(
            "電圧 {}% (>= {}%) は十分なため、カメラを初期化し画像をキャプチャします。",
            measured_voltage_percent, LOW_VOLTAGE_THRESHOLD_PERCENT
        );

        let camera_config = camera::M5UnitCamConfig {
            frame_size: M5UnitCamConfig::from_string(&app_config.frame_size),
        };

        // カメラを初期化。失敗した場合は `?` により main 関数からエラーが返る。
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
        let _ =
            camera.configure_exposure(app_config.auto_exposure_enabled, Some(current_aec_value)); // 自動露出設定を適用
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

        // 3回目の画像をキャプチャし、データをコピーして image_data_for_task に保存
        match camera.capture_image() {
            Ok(fb) => {
                info!("画像キャプチャ成功: {} バイト", fb.data().len());
                image_data_option = Some(fb.data().to_vec()); // 画像データを Vec<u8> としてコピー
            }
            Err(e) => {
                error!("画像キャプチャ失敗 (最終): {:?}", e);
                led.blink_error()?;
                // image_data_for_task は None のまま
            }
        };
    } else {
        info!(
            "電圧が低い ({}% < {}%) ため、カメラ処理をスキップします。",
            measured_voltage_percent, LOW_VOLTAGE_THRESHOLD_PERCENT
        );
        led.blink_error()?;
        // image_data_for_task は None のまま
    };
    // camera インスタンスはここでドロップされる
    
    // DS18B20 温度センサーの初期化. GPIO17 (power pin), GPIO16 (data pin)
    let mut temp_sensor = TempSensor::new(17, 16, peripherals_all.rmt.channel0)?;
    info!("DS18B20 温度センサーを初期化しました。");
    // 温度センサーから温度を読み取る
    let temperature_celsius_option: Option<f32> = match temp_sensor.read_temperature() {
        Ok(t) => Some(t),
        Err(e) => {
            warn!("DS18B20 温度センサーの読み取りに失敗しました: {:?}", e);
            // エラーが発生した場合はNoneのままにする
            None
        }
    };
    info!("DS18B20 温度センサーの読み取り結果: {:.2}°C", temperature_celsius_option.unwrap_or(-999.0));

    // ESP-NOW用にWiFiを再設定・起動
    info!("ESP-NOW用にWiFiをSTAモードで準備します。");
    let mut wifi_instance_for_espnow = if let Some(wifi) = wifi_option.take() {
        info!("時刻同期で使用した(かもしれない)WiFiインスタンスをESP-NOW用に再設定します。");
        wifi
    } else {
        // modem_peripheral_option が None の場合、SNTP処理で消費されたか、最初からなかった
        if modem_peripheral_option.is_none() {
            info!("ESP-NOW用WiFi初期化のためにmodemペリフェラルを再取得します。");
            modem_peripheral_option = Some(Peripherals::take().unwrap().modem);
        }
        info!("ESP-NOW用にWiFiを新規に初期化します。");
        BlockingWifi::wrap(
            EspWifi::new(
                modem_peripheral_option.take().unwrap(),
                sysloop.clone(),
                Some(nvs_default_partition.clone()), // nvs_default_partition を使用
            )?,
            sysloop.clone(),
        )?
    };

    // ESP-NOWは特定のSSIDへの接続を必要としないため、ダミー設定で起動
    // ただし、esp_wifi_start() は必要
    wifi_instance_for_espnow.set_configuration(&esp_idf_svc::wifi::Configuration::Client(
        esp_idf_svc::wifi::ClientConfiguration {
            ssid: "".try_into().unwrap(),     // ESP-NOWではSSIDは通常関係ない
            password: "".try_into().unwrap(), // パスワードも同様
            auth_method: esp_idf_svc::wifi::AuthMethod::None,
            ..Default::default()
        },
    ))?;
    wifi_instance_for_espnow.start()?;
    info!("WiFiがESP-NOW用にSTAモードで起動しました。");

    // --- データ送信タスク ---
    info!("データ送信タスクを開始します");
    let measured_data = MeasuredData::new(
        measured_voltage_percent,
        image_data_option, // Option<Vec<u8>> を渡す
        temperature_celsius_option, // 温度センサーの結果を渡す
    );
    if let Err(e) = transmit_data_task(
        &app_config,
        &mut wifi_instance_for_espnow, // WiFiインスタンスを渡す
        &mut led,
        measured_data,
    ) {
        error!("データ送信タスクでエラーが発生しました: {:?}", e);
        // エラーが発生してもスリープ処理は行う
    }

    led.turn_off()?;
    store_current_date_to_nvs(&mut nvs, current_date_for_nvs); // Store date before normal sleep

    // --- スリープ処理 ---
    let elapsed_time = loop_start_time.elapsed();
    info!("メインループ処理時間 : {:?}", elapsed_time);
    // 電圧ゼロの場合は ミディアムスリープ or ロングスリープ、そうでない場合は固定インターバルでスリープ
    if measured_voltage_percent == 0 {
        // 現在時間が午前の場合はミディアムスリープ、午後の場合はロングスリープ
        let current_hour = Utc::now().with_timezone(&tz).hour();
        if current_hour < 12 {
            info!(
                "電圧が0%のため、{}秒間の中時間ディープスリープに入ります。",
                app_config.sleep_duration_seconds_for_medium
            );
            let _ = deep_sleep_controller.sleep_for_duration(app_config.sleep_duration_seconds_for_medium)?;
        } else {
            info!(
                "電圧が0%のため、{}秒間の長時間ディープスリープに入ります。",
                app_config.sleep_duration_seconds_for_long
            );
            let _ = deep_sleep_controller.sleep_for_duration(app_config.sleep_duration_seconds_for_long)?;
        }
        // ディープスリープから復帰することはないため、以降のコードは実行されない
        return Ok(()); // 明示的に終了
    }else{
        info!(
            "電圧が0%ではないため、固定インターバルモードでスリープします。経過時間: {:?}",
            elapsed_time
        );
        let _ = deep_sleep_controller.sleep(elapsed_time, min_sleep_duration);
    }

    Ok(())
}
