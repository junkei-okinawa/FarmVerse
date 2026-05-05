use anyhow::Result;
use esp_idf_svc::hal::{delay::FreeRtos, peripherals::Peripherals};
use log::info;
use simple_ds18b20_temp_sensor::TempSensor;

mod utils;
#[cfg(feature = "wifi")]
use utils::{format_hash_payload, needs_recalibration, parse_mac, EOF_MARKER};

#[toml_cfg::toml_config]
struct Config {
    /// 計測間隔 (秒)
    #[default(600)]
    measure_interval_s: u32,
    /// Deep Sleep 使用フラグ (true: 省電力, false: FreeRTOS delay)
    #[default(false)]
    use_deep_sleep: bool,
    /// ESP-NOW 送信先 MAC アドレス (wifi feature 使用時のみ参照)
    #[default("11:22:33:44:55:66")]
    receiver_mac: &'static str,
    /// ESP-NOW チャンネル番号 (0 = 現在の STA チャンネル、1-13 = 固定)
    /// ゲートウェイ (usb_cdc_receiver) の起動ログ "WiFiチャンネル: Primary: N" で確認
    #[default(0)]
    wifi_channel: u8,
    /// PHY 強制再キャリブレーション周期 (deep sleep サイクル数、0 = 無効)
    /// 温度変化・経時変化によるRF性能低下を防ぐため定期的に実行する
    #[default(100)]
    recalibration_interval: u32,
}

// GPIO アサイン (XIAO ESP32-S3)
//   D1 = GPIO2 : DS18B20 電源制御
//   D3 = GPIO4 : DS18B20 1-Wire データ (1kΩ プルアップ必要)
const POWER_PIN: i32 = 2;
const DATA_PIN: i32 = 4;

// =============================================================================
// RTC メモリ: Deep Sleep をまたいで保持される変数
// =============================================================================

// POR と Deep Sleep 復帰を区別するためのマジック値
const RTC_MAGIC: u32 = 0xA55A_B00B;

#[link_section = ".rtc.data"]
static mut DEEP_SLEEP_CYCLE: u32 = 0;

// POR 後に 0 (≠ RTC_MAGIC) になることで初回起動を検出する
#[link_section = ".rtc.data"]
static mut RTC_MAGIC_VAL: u32 = 0;

// =============================================================================
// main
// =============================================================================

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    // Deep Sleep 復帰時: 前回スリープ前に設定した gpio_hold を解除する。
    // hold が残ったままだと gpio_set_level 等の操作が無効になる。
    if CONFIG.use_deep_sleep {
        unsafe { esp_idf_svc::sys::gpio_hold_dis(POWER_PIN) };
    }

    info!(
        "XIAO ESP32-S3 DS18B20 starting \
         (power=GPIO{}, data=GPIO{}, interval={}s, deep_sleep={})",
        POWER_PIN, DATA_PIN, CONFIG.measure_interval_s, CONFIG.use_deep_sleep
    );

    #[cfg(feature = "wifi")]
    if CONFIG.wifi_channel > 13 {
        return Err(anyhow::anyhow!(
            "cfg.toml: wifi_channel は 0..=13 の範囲で設定してください (got {})",
            CONFIG.wifi_channel
        ));
    }

    let peripherals = Peripherals::take().unwrap();
    let rmt_channel0 = peripherals.rmt.channel0;
    #[cfg(feature = "wifi")]
    let modem = peripherals.modem;

    // FreeRTOS モード: WiFi をループ前に一度だけ初期化してループ全体で再利用する。
    // Deep Sleep モード: ループ内で温度計測後に初期化する (計測失敗時は WiFi をスキップして省電力)。
    #[cfg(feature = "wifi")]
    let (freertos_wifi, mut sleep_modem): (
        Option<(esp_idf_svc::espnow::EspNow<'static>, [u8; 6])>,
        Option<esp_idf_svc::hal::modem::Modem>,
    ) = if !CONFIG.use_deep_sleep {
        (Some(init_esp_now(modem)?), None)
    } else {
        (None, Some(modem))
    };

    let mut sensor = TempSensor::new(POWER_PIN, DATA_PIN, rmt_channel0)?;

    loop {
        // --- Step 1: 温度計測 (WiFi 起動前・低電力フェーズ) ---
        // WiFi RF が 1-Wire タイミングに干渉しないよう計測を先に完了させる。
        // 計測失敗時は WiFi 初期化をスキップすることで Deep Sleep 時の省電力になる。
        let temp_result = sensor.read_temperature();
        match &temp_result {
            Ok(temp) => info!("Temperature: {:.2}°C", temp),
            Err(e) => log::error!("Failed to read temperature: {:?}", e),
        }

        // --- Step 2: WiFi 初期化 + ESP-NOW 送信 ---
        #[cfg(feature = "wifi")]
        {
            if let Ok(temp) = &temp_result {
                if CONFIG.use_deep_sleep {
                    // Deep Sleep モード: 計測成功時のみ WiFi を初期化する
                    if let Some(modem) = sleep_modem.take() {
                        match init_esp_now(modem) {
                            Ok((esp_now, peer_mac)) => {
                                if let Err(e) = send_temperature(&esp_now, peer_mac, *temp) {
                                    log::warn!("ESP-NOW send failed: {:?}", e);
                                }
                            }
                            Err(e) => log::warn!("WiFi init failed: {:?}", e),
                        }
                    }
                } else {
                    // FreeRTOS モード: 起動前に初期化済みの WiFi で送信
                    if let Some((esp_now, peer_mac)) = &freertos_wifi {
                        if let Err(e) = send_temperature(esp_now, *peer_mac, *temp) {
                            log::warn!("ESP-NOW send failed: {:?}", e);
                        }
                    }
                }
            } else if CONFIG.use_deep_sleep {
                info!("Sensor read failed, skipping WiFi init (power saving)");
            }
        }

        // --- Step 3: スリープ ---
        sleep_or_delay(CONFIG.measure_interval_s);
    }
}

/// 待機処理: cfg.toml の use_deep_sleep に応じて切り替え
///
/// use_deep_sleep = false: FreeRTOS delay (USB モニタリング継続可)
/// use_deep_sleep = true : Deep Sleep → ウェイクアップ後 main() から再実行
fn sleep_or_delay(interval_s: u32) {
    if CONFIG.use_deep_sleep {
        info!("Deep sleep for {}s (restart after wakeup)", interval_s);
        unsafe {
            // DS18B20 電源ピン (GPIO2) を明示的に LOW にしてからスリープ。
            // TempSensor が計測後に HIGH を残す可能性があり、1kΩ プルアップ経由で
            // 電流が流れ続けることを防ぐ。
            // gpio_hold_en でスリープ中も LOW を保持する (xiao_esp32s3_sense と同パターン)。
            esp_idf_svc::sys::gpio_set_level(POWER_PIN, 0);
            esp_idf_svc::sys::gpio_hold_en(POWER_PIN);
            esp_idf_svc::sys::esp_deep_sleep((interval_s as u64) * 1_000_000);
        }
        // esp_deep_sleep() は戻らない
    } else {
        FreeRtos::delay_ms(interval_s.saturating_mul(1_000));
    }
}

// =============================================================================
// WiFi + ESP-NOW (wifi feature のみコンパイル)
// =============================================================================

/// Deep Sleep サイクルカウンタを更新し、PHY 強制再キャリブレーションが必要か判定する
///
/// POR 後の初回起動:      RTC_MAGIC_VAL が不一致 → カウンタを 0 に初期化
/// Deep Sleep (タイマー復帰): カウンタをインクリメント
/// その他のリセット:      カウンタを 0 に初期化
///
/// recalibration_interval サイクルごとに true を返す
#[cfg(feature = "wifi")]
fn should_force_recalibrate() -> bool {
    if CONFIG.recalibration_interval == 0 {
        return false;
    }

    let is_timer_wakeup = unsafe {
        esp_idf_svc::sys::esp_sleep_get_wakeup_cause()
            == esp_idf_svc::sys::esp_sleep_source_t_ESP_SLEEP_WAKEUP_TIMER
    };

    let cycle = unsafe {
        if RTC_MAGIC_VAL != RTC_MAGIC {
            // POR または flash 後の初回起動: RTC メモリを初期化
            RTC_MAGIC_VAL = RTC_MAGIC;
            DEEP_SLEEP_CYCLE = 0;
            0u32
        } else if is_timer_wakeup {
            // Deep Sleep タイマー復帰: カウンタをインクリメント
            DEEP_SLEEP_CYCLE = DEEP_SLEEP_CYCLE.wrapping_add(1);
            DEEP_SLEEP_CYCLE
        } else {
            // USB リセット等その他のリセット: カウンタを初期化
            DEEP_SLEEP_CYCLE = 0;
            0u32
        }
    };

    let force = needs_recalibration(cycle, CONFIG.recalibration_interval);
    if force {
        info!("Cycle {}: PHY recalibration scheduled", cycle);
    } else {
        info!(
            "Cycle {}/{}: using stored PHY calibration",
            cycle, CONFIG.recalibration_interval
        );
    }
    force
}

// esp_phy_erase_cal_data_in_nvs は esp-idf-sys の bindgen 生成対象外のため extern "C" で宣言する。
// (xiao_esp32s3_sense での esp_idf_sys 直接呼び出しパターンと同様の手法)
// esp_phy_init.h: esp_err_t esp_phy_erase_cal_data_in_nvs(void)
#[cfg(feature = "wifi")]
extern "C" {
    fn esp_phy_erase_cal_data_in_nvs() -> i32;
}

/// PHY キャリブレーションデータを NVS から消去する
///
/// 次回 WiFi 初期化時にフルキャリブレーションが実行され NVS に保存される。
#[cfg(feature = "wifi")]
fn erase_phy_calibration() {
    let ret = unsafe { esp_phy_erase_cal_data_in_nvs() };
    if ret == esp_idf_svc::sys::ESP_OK {
        info!("PHY calibration data erased (full recalibration on next WiFi init)");
    } else {
        log::warn!("esp_phy_erase_cal_data_in_nvs failed: {}", ret);
    }
}

/// WiFi を STA モードで起動し ESP-NOW を初期化する
///
/// Deep Sleep モード時:
///   - 周期的 PHY 再キャリブレーション (should_force_recalibrate で判定)
///   - WiFi 初期化エラー時は PHY キャリブレーションを消去して次回起動でリカバリ
#[cfg(feature = "wifi")]
fn init_esp_now(
    modem: esp_idf_svc::hal::modem::Modem,
) -> Result<(esp_idf_svc::espnow::EspNow<'static>, [u8; 6])> {
    use esp_idf_svc::{
        espnow::EspNow,
        eventloop::EspSystemEventLoop,
        nvs::EspDefaultNvsPartition,
        wifi::{AuthMethod, BlockingWifi, ClientConfiguration, Configuration, EspWifi},
    };

    // Deep Sleep モード: 周期的 PHY 再キャリブレーション
    if CONFIG.use_deep_sleep && should_force_recalibrate() {
        erase_phy_calibration();
    }

    let peer_mac = parse_mac(CONFIG.receiver_mac)
        .ok_or_else(|| anyhow::anyhow!("cfg.toml の receiver_mac が不正 (形式: XX:XX:XX:XX:XX:XX)"))?;

    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // EspWifi 初期化: 失敗時は PHY キャリブレーションデータを消去して次回起動でリカバリ
    let esp_wifi = match EspWifi::new(modem, sysloop.clone(), Some(nvs)) {
        Ok(w) => w,
        Err(e) => {
            log::warn!("EspWifi init failed ({:?}), erasing PHY cal for recovery on next boot", e);
            erase_phy_calibration();
            return Err(anyhow::anyhow!("EspWifi::new failed: {:?}", e));
        }
    };

    // esp_wifi_set_storage は esp_wifi_start() 前に呼ぶ必要がある (ESP-IDF 推奨順序)。
    // start() 後に呼ぶと NVS から読み込んだチャンネル情報が破棄され、
    // deep sleep モードで毎サイクル再初期化する際に RF チャンネルが不定になる。
    // ゲートウェイ (usb_cdc_receiver/initialize_wifi) と同じ順序に統一する。
    unsafe {
        let st_ret = esp_idf_svc::sys::esp_wifi_set_storage(
            esp_idf_svc::sys::wifi_storage_t_WIFI_STORAGE_RAM,
        );
        if st_ret != esp_idf_svc::sys::ESP_OK {
            log::warn!("esp_wifi_set_storage failed: {}", st_ret);
        }
    }

    let mut wifi = BlockingWifi::wrap(esp_wifi, sysloop)?;

    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: "".try_into().unwrap(),
        password: "".try_into().unwrap(),
        auth_method: AuthMethod::None,
        ..Default::default()
    }))?;

    // WiFi STA 起動: 失敗時も PHY キャリブレーションを消去してリカバリ
    if let Err(e) = wifi.start() {
        log::warn!("WiFi start failed ({:?}), erasing PHY cal for recovery on next boot", e);
        erase_phy_calibration();
        return Err(e.into());
    }

    let mac = wifi.wifi().sta_netif().get_mac()?;
    info!(
        "Device MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );

    unsafe {
        let ps_ret =
            esp_idf_svc::sys::esp_wifi_set_ps(esp_idf_svc::sys::wifi_ps_type_t_WIFI_PS_NONE);
        if ps_ret != esp_idf_svc::sys::ESP_OK {
            log::warn!("esp_wifi_set_ps failed: {}", ps_ret);
        }
    }

    Box::leak(Box::new(wifi));

    let esp_now = EspNow::take()?;

    let peer_info = esp_idf_svc::espnow::PeerInfo {
        peer_addr: peer_mac,
        channel: CONFIG.wifi_channel, // cfg.toml で設定 (0 = 現在の STA チャンネル、1-13 = 固定)
        ifidx: esp_idf_svc::wifi::WifiDeviceId::Sta.into(),
        encrypt: false,
        lmk: [0u8; 16],
        priv_: std::ptr::null_mut(),
    };
    esp_now.add_peer(peer_info)?;

    let ch_str = if CONFIG.wifi_channel == 0 {
        "sta-channel".to_string()
    } else {
        CONFIG.wifi_channel.to_string()
    };
    info!(
        "ESP-NOW peer registered: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X} (channel: {})",
        peer_mac[0], peer_mac[1], peer_mac[2], peer_mac[3], peer_mac[4], peer_mac[5], ch_str
    );

    Ok((esp_now, peer_mac))
}

/// 温度を ESP-NOW で送信する
///
/// usb_cdc_receiver (ESP32-C3 ゲートウェイ) の detect_frame_type は
/// ESP-NOW ペイロードの先頭テキストでフレーム種別を判定するため、
/// バイナリフレームに包まず生テキストを直接送信する。
#[cfg(feature = "wifi")]
fn send_temperature(
    esp_now: &esp_idf_svc::espnow::EspNow<'static>,
    peer_mac: [u8; 6],
    temp: f32,
) -> Result<()> {
    // VOLT:100 = 電圧センサなしのプレースホルダ
    // TDS_VOLT:-999.0 = TDS センサなしのセンチネル値 (サーバー側で None として扱われる)
    let hash_payload = format_hash_payload(temp);

    // Deep Sleep モード: 温度計測を WiFi 起動前に行うため、送信時点でのラジオ安定待ち時間がない。
    // non-deep-sleep モードでは温度計測 (~1.4s) がラジオ安定待ちを兼ねている。
    // esp_now.send() はパケットをキューに入れるだけ (非同期) であるため、
    // deep sleep 直前の esp_now_deinit() でキューが破棄されないよう十分な待機が必要。
    FreeRtos::delay_ms(300);

    info!("Sending HASH payload ({} bytes)", hash_payload.len());
    esp_now.send(peer_mac, hash_payload.as_bytes())?;

    FreeRtos::delay_ms(50);

    info!("Sending EOF");
    esp_now.send(peer_mac, EOF_MARKER)?;

    // esp_now がドロップされると esp_now_deinit() が呼ばれ未送信パケットが破棄される。
    // EOF の実際の送信 (~4ms) を完了させてからドロップ・deep sleep に進む。
    FreeRtos::delay_ms(100);

    info!("ESP-NOW send complete");
    Ok(())
}

