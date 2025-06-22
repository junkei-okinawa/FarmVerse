use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    nvs::EspDefaultNvsPartition,
    wifi::{BlockingWifi, EspWifi},
};
use esp_idf_svc::hal::modem::Modem;
use log::info;

/// WiFiとESP-NOWの初期化を管理するモジュール
pub struct NetworkManager;

impl NetworkManager {
    /// WiFiをESP-NOW用に初期化（ESP-NOW初期化は呼び出し側で行う）
    pub fn initialize_wifi_for_esp_now(
        modem: Modem,
        sysloop: &EspSystemEventLoop,
        nvs_partition: &EspDefaultNvsPartition,
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
        
        wifi.start()?;
        info!("WiFiがESP-NOW用にSTAモードで起動しました。");

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
}
