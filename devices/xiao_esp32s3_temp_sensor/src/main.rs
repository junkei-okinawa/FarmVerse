use anyhow::Result;
use esp_idf_svc::hal::{delay::FreeRtos, peripherals::Peripherals};
use log::info;
use simple_ds18b20_temp_sensor::TempSensor;

// cfg.toml から読み込む設定 (存在しない場合はデフォルト値を使用)
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
}

// GPIO アサイン (XIAO ESP32-S3)
//   D1 = GPIO2 : DS18B20 電源制御
//   D3 = GPIO4 : DS18B20 1-Wire データ (4.7kΩ プルアップ必要)
const POWER_PIN: i32 = 2;
const DATA_PIN: i32 = 4;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    info!(
        "XIAO ESP32-S3 DS18B20 starting \
         (power=GPIO{}, data=GPIO{}, interval={}s, deep_sleep={})",
        POWER_PIN, DATA_PIN, CONFIG.measure_interval_s, CONFIG.use_deep_sleep
    );

    let peripherals = Peripherals::take().unwrap();
    // 必要なフィールドを先に分解して所有権を確定させる
    let rmt_channel0 = peripherals.rmt.channel0;
    #[cfg(feature = "wifi")]
    let modem = peripherals.modem;

    // Phase 2: WiFi + ESP-NOW 初期化 (wifi feature のみコンパイル)
    #[cfg(feature = "wifi")]
    let (esp_now, peer_mac) = init_esp_now(modem)?;

    let mut sensor = TempSensor::new(POWER_PIN, DATA_PIN, rmt_channel0)?;

    loop {
        match sensor.read_temperature() {
            Ok(temp) => {
                info!("Temperature: {:.2}°C", temp);

                // Phase 2: ESP-NOW 送信 (wifi feature のみコンパイル)
                #[cfg(feature = "wifi")]
                if let Err(e) = send_temperature(&esp_now, peer_mac, temp) {
                    log::warn!("ESP-NOW send failed: {:?}", e);
                }
            }
            Err(e) => log::error!("Failed to read temperature: {:?}", e),
        }

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
        // Deep sleep 中は USB 接続が切断される。
        // ウェイクアップ後はリセットされ main() から再実行される。
        unsafe {
            esp_idf_svc::sys::esp_deep_sleep((interval_s as u64) * 1_000_000);
        }
        // esp_deep_sleep() は戻らない
    } else {
        FreeRtos::delay_ms(interval_s.saturating_mul(1_000));
    }
}

// =============================================================================
// Phase 2: WiFi + ESP-NOW (wifi feature のみコンパイル)
// =============================================================================

/// WiFi を STA モードで起動し ESP-NOW を初期化する
///
/// devices/xiao_esp32s3_sense/src/communication/network_manager.rs と同パターン。
/// BlockingWifi は Box::leak で 'static 昇格させ、ESP-NOW の生存期間を保証する。
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

    let peer_mac = parse_mac(CONFIG.receiver_mac)
        .ok_or_else(|| anyhow::anyhow!("cfg.toml の receiver_mac が不正 (形式: XX:XX:XX:XX:XX:XX)"))?;

    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(modem, sysloop.clone(), Some(nvs))?,
        sysloop,
    )?;

    // ESP-NOW 用に空 SSID で STA モード起動 (AP へは接続しない)
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: "".try_into().unwrap(),
        password: "".try_into().unwrap(),
        auth_method: AuthMethod::None,
        ..Default::default()
    }))?;
    wifi.start()?;

    // 自デバイスの MAC アドレスをログ出力
    // → この値を送信先デバイス側の cfg.toml に設定する
    let mac = wifi.wifi().sta_netif().get_mac()?;
    info!(
        "Device MAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );

    unsafe {
        // ESP-NOW の安定性向上のため WiFi Power Save を無効化
        let ps_ret = esp_idf_svc::sys::esp_wifi_set_ps(esp_idf_svc::sys::wifi_ps_type_t_WIFI_PS_NONE);
        if ps_ret != esp_idf_svc::sys::ESP_OK {
            log::warn!("esp_wifi_set_ps failed: {}", ps_ret);
        }
        // ESP-NOW のバッファを RAM に設定 (Flash 書き込みを回避)
        let st_ret = esp_idf_svc::sys::esp_wifi_set_storage(
            esp_idf_svc::sys::wifi_storage_t_WIFI_STORAGE_RAM,
        );
        if st_ret != esp_idf_svc::sys::ESP_OK {
            log::warn!("esp_wifi_set_storage failed: {}", st_ret);
        }
    }

    // wifi を Box::leak で 'static 昇格させ EspNow の生存期間を保証
    Box::leak(Box::new(wifi));

    let esp_now = EspNow::take()?;

    // ピア登録
    let peer_info = esp_idf_svc::espnow::PeerInfo {
        peer_addr: peer_mac,
        channel: 0,
        ifidx: esp_idf_svc::wifi::WifiDeviceId::Sta.into(),
        encrypt: false,
        lmk: [0u8; 16],
        priv_: std::ptr::null_mut(),
    };
    esp_now.add_peer(peer_info)?;
    info!(
        "ESP-NOW peer registered: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        peer_mac[0], peer_mac[1], peer_mac[2], peer_mac[3], peer_mac[4], peer_mac[5]
    );

    Ok((esp_now, peer_mac))
}

/// 温度を ESP-NOW で送信する
///
/// sensor_data_reciver のフレームプロトコルに準拠:
///   HASH frame (type=1) → EOF frame (type=3)
///
/// HASH ペイロード形式 (sender.rs send_hash_frame と同一):
///   HASH:{64桁ゼロ},VOLT:100,TEMP:{temp:.1},TDS_VOLT:-999.0,2000/01/01 00:00:00.000
///   ※ timestamp は固定プレースホルダー (リアルタイムクロック非搭載のため)
#[cfg(feature = "wifi")]
fn send_temperature(
    esp_now: &esp_idf_svc::espnow::EspNow<'static>,
    peer_mac: [u8; 6],
    temp: f32,
) -> Result<()> {
    const DUMMY_HASH: &str =
        "0000000000000000000000000000000000000000000000000000000000000000";

    // 送信元 MAC を取得してフレームヘッダに埋め込む
    let mut sender_mac = [0u8; 6];
    let ret = unsafe {
        esp_idf_svc::sys::esp_wifi_get_mac(
            esp_idf_svc::sys::wifi_interface_t_WIFI_IF_STA,
            sender_mac.as_mut_ptr(),
        )
    };
    if ret != esp_idf_svc::sys::ESP_OK {
        return Err(anyhow::anyhow!("esp_wifi_get_mac failed: {}", ret));
    }

    // VOLT:100 = 電圧センサなしのプレースホルダ
    // TDS_VOLT:-999.0 = TDS センサなしのセンチネル値 (サーバー側で None として扱われ InfluxDB には書き込まれない)
    let hash_payload = format!(
        "HASH:{},VOLT:100,TEMP:{:.1},TDS_VOLT:-999.0,2000/01/01 00:00:00.000",
        DUMMY_HASH, temp
    );

    let hash_frame = build_frame(&sender_mac, 1, 0, hash_payload.as_bytes());
    let eof_frame = build_frame(&sender_mac, 3, 1, b"EOF");

    info!("Sending HASH frame ({} bytes)", hash_frame.len());
    esp_now.send(peer_mac, &hash_frame)?;

    FreeRtos::delay_ms(50);

    info!("Sending EOF frame ({} bytes)", eof_frame.len());
    esp_now.send(peer_mac, &eof_frame)?;

    info!("ESP-NOW send complete");
    Ok(())
}

/// sensor_data_reciver 準拠のバイナリフレームを構築する
///
/// フレーム構造 (sender.rs create_sensor_data_frame と同一):
///   [START(4B)][MAC(6B)][TYPE(1B)][SEQ(4B LE)][LEN(4B LE)][DATA][CHECKSUM(4B LE)][END(4B)]
#[cfg(feature = "wifi")]
fn build_frame(sender_mac: &[u8; 6], frame_type: u8, seq: u32, data: &[u8]) -> Vec<u8> {
    const START: [u8; 4] = [0xFA, 0xCE, 0xAA, 0xBB];
    const END: [u8; 4] = [0xCD, 0xEF, 0x56, 0x78];

    let mut f = Vec::with_capacity(27 + data.len());
    f.extend_from_slice(&START);
    f.extend_from_slice(sender_mac);
    f.push(frame_type);
    f.extend_from_slice(&seq.to_le_bytes());
    f.extend_from_slice(&(data.len() as u32).to_le_bytes());
    f.extend_from_slice(data);
    f.extend_from_slice(&xor_checksum(data).to_le_bytes());
    f.extend_from_slice(&END);
    f
}

/// XOR チェックサム (sender.rs calculate_xor_checksum と同一アルゴリズム)
///
/// DATA を 4 バイト単位のリトルエンディアン u32 としてXOR
#[cfg(feature = "wifi")]
fn xor_checksum(data: &[u8]) -> u32 {
    let mut cs: u32 = 0;
    for chunk in data.chunks(4) {
        let mut val = 0u32;
        for (i, &b) in chunk.iter().enumerate() {
            val |= (b as u32) << (i * 8);
        }
        cs ^= val;
    }
    cs
}

/// "XX:XX:XX:XX:XX:XX" 形式の文字列を [u8; 6] に変換
#[cfg(feature = "wifi")]
fn parse_mac(s: &str) -> Option<[u8; 6]> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 6 {
        return None;
    }
    let mut mac = [0u8; 6];
    for (i, p) in parts.iter().enumerate() {
        mac[i] = u8::from_str_radix(p, 16).ok()?;
    }
    Some(mac)
}
