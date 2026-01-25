use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    nvs::EspDefaultNvsPartition,
    wifi::{BlockingWifi, EspWifi},
    espnow::EspNow,
};
use esp_idf_svc::hal::modem::Modem;
use log::info;
use std::sync::{Arc, Mutex};
use crate::communication::esp_now::EspNowReceiver;

/// WiFiとESP-NOWの初期化を管理するモジュール
pub struct NetworkManager;

impl NetworkManager {
    /// WiFiをESP-NOW用に初期化（ESP-NOW初期化は呼び出し側で行う）
    pub fn initialize_wifi_for_esp_now(
        modem: Modem,
        sysloop: &EspSystemEventLoop,
        nvs_partition: &EspDefaultNvsPartition,
        wifi_tx_power_dbm: i8,
        wifi_init_delay_ms: u64,
    ) -> anyhow::Result<BlockingWifi<EspWifi<'static>>> {
        info!("ESP-NOW用にWiFiをSTAモードで準備します。");
        
        let mut wifi = BlockingWifi::wrap(
            EspWifi::new(modem, sysloop.clone(), Some(nvs_partition.clone()))?,
            sysloop.clone(),
        )?;

        // 空のSSID/パスワードでWiFiを設定（ESP-NOW用）
        wifi.set_configuration(&esp_idf_svc::wifi::Configuration::Client(
            esp_idf_svc::wifi::ClientConfiguration {
                ssid: "".try_into().unwrap(),
                password: "".try_into().unwrap(),
                auth_method: esp_idf_svc::wifi::AuthMethod::None,
                ..Default::default()
            },
        ))?;
        
        info!("WiFi設定完了。起動待機({}ms)...", wifi_init_delay_ms);
        esp_idf_svc::hal::delay::FreeRtos::delay_ms(wifi_init_delay_ms as u32); // 突入電流分散待機 1
        
        wifi.start()?;
        info!("WiFiがESP-NOW用にSTAモードで起動しました。RF安定化待機({}ms)...", wifi_init_delay_ms);
        esp_idf_svc::hal::delay::FreeRtos::delay_ms(wifi_init_delay_ms as u32); // 突入電流分散待機 2

        // WiFi送信パワーを設定（省電力化）
        unsafe {
            // ESP-IDF API expects power in 0.25 dBm units (quarter-dBm).
            // Perform the scaling in a wider integer type to avoid i8 overflow,
            // then clamp into the valid i8 range before passing to the driver.
            let scaled: i16 = i16::from(wifi_tx_power_dbm) * 4;
            let min_i8 = i16::from(i8::MIN);
            let max_i8 = i16::from(i8::MAX);
            
            if scaled < min_i8 || scaled > max_i8 {
                log::warn!(
                    "WiFi送信パワー値が許容範囲外です ({} dBm, 四分の一dBm単位: {}). i8範囲にクランプします。",
                    wifi_tx_power_dbm,
                    scaled
                );
            }
            let power_quarter_dbm = scaled.clamp(min_i8, max_i8) as i8;

            let err = esp_idf_svc::sys::esp_wifi_set_max_tx_power(power_quarter_dbm);
            if err != esp_idf_svc::sys::ESP_OK {
                // 送信パワー設定失敗時は、デフォルト値で動作するが、システム自体は停止させない
                log::warn!("WiFi送信パワーの設定に失敗しました (エラーコード: {}) - デフォルトパワーで動作します", err);
            }
        }
        info!("WiFi送信パワーを {}dBm に設定しました。適用待機({}ms)...", wifi_tx_power_dbm, wifi_init_delay_ms);
        esp_idf_svc::hal::delay::FreeRtos::delay_ms(wifi_init_delay_ms as u32); // 突入電流分散待機 3
        
        // WiFi状態の詳細確認
        let wifi_status = wifi.is_started();
        info!("WiFi起動状態: {:?}", wifi_status);
        
        // WiFiのMACアドレスを取得して表示
        let mac_addr = wifi.wifi().sta_netif().get_mac()?;
        info!("デバイスMACアドレス: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", 
              mac_addr[0], mac_addr[1], mac_addr[2], 
              mac_addr[3], mac_addr[4], mac_addr[5]);

        // WiFi Power Saveを無効化
        unsafe {
            esp_idf_svc::sys::esp_wifi_set_ps(esp_idf_svc::sys::wifi_ps_type_t_WIFI_PS_NONE);
        }
        info!("Wi-Fi Power Save を無効化しました (ESP-NOW用)");

        Ok(wifi)
    }

    /// ESP-NOW初期化（送信＆受信機能付き）
    pub fn initialize_esp_now(
        _wifi: &BlockingWifi<EspWifi<'static>>,
    ) -> anyhow::Result<(Arc<Mutex<EspNow<'static>>>, EspNowReceiver)> {
        info!("ESP-NOWを初期化中（送信＆受信機能付き）...");
        
        // ESP-NOWのメモリ設定を最適化
        unsafe {
            // ESP-NOWの送信バッファサイズを調整（デフォルトより小さく）
            esp_idf_svc::sys::esp_wifi_set_storage(esp_idf_svc::sys::wifi_storage_t_WIFI_STORAGE_RAM);
        }
        
        let esp_now = EspNow::take()?;
        let esp_now_arc = Arc::new(Mutex::new(esp_now));
        
        // ESP-NOW受信機能を初期化
        let receiver = EspNowReceiver::new(Arc::clone(&esp_now_arc))?;
        
        info!("ESP-NOW初期化完了（送信＆受信機能）");
        Ok((esp_now_arc, receiver))
    }
}
