use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    nvs::EspDefaultNvsPartition,
    wifi::{BlockingWifi, EspWifi},
};
use esp_idf_svc::hal::modem::Modem;
use log::info;

use crate::core::config::AppConfig;
use crate::communication::esp_now::EspNowSender;

/// WiFiとESP-NOWの初期化を管理するモジュール
pub struct NetworkManager;

impl NetworkManager {
    /// WiFiをESP-NOW用に初期化し、ESP-NOWセンダーを作成
    /// WiFi接続を適切に管理してESP-NOW通信を安定化
    pub fn initialize_esp_now(
        modem: Modem,
        sysloop: &EspSystemEventLoop,
        nvs_partition: &EspDefaultNvsPartition,
        config: &AppConfig,
    ) -> anyhow::Result<(EspNowSender, BlockingWifi<EspWifi<'static>>)> {
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

        // WiFi Power Saveを無効化
        unsafe {
            esp_idf_svc::sys::esp_wifi_set_ps(esp_idf_svc::sys::wifi_ps_type_t_WIFI_PS_NONE);
        }
        info!("Wi-Fi Power Save を無効化しました (ESP-NOW用)");

        // ESP-NOWセンダーを初期化
        info!("ESP-NOWセンダーを初期化中...");
        let esp_now_sender = EspNowSender::new().map_err(|e| {
            log::error!("ESP-NOW初期化に失敗: {:?}", e);
            anyhow::anyhow!("ESP-NOW初期化に失敗: {:?}", e)
        })?;
        info!("ESP-NOWセンダーの初期化が完了しました");
        
        info!("受信機ピアを追加中: {}", config.receiver_mac);
        esp_now_sender.add_peer(&config.receiver_mac).map_err(|e| {
            log::error!("ESP-NOWピア追加に失敗: {:?}", e);
            anyhow::anyhow!("ESP-NOWピア追加に失敗: {:?}", e)
        })?;
        info!("ESP-NOW sender initialized and peer added. Receiver MAC: {}", config.receiver_mac);

        Ok((esp_now_sender, wifi))
    }
}
