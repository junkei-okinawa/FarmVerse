use log::info;

/// Platform-agnostic light-sleep abstraction.
pub trait LightSleepPlatform {
    /// Enter light sleep for the specified duration in microseconds.
    fn light_sleep(&self, duration_us: u64);
}

/// ESP-IDF specific light sleep implementation.
pub struct EspIdfLightSleep;

impl LightSleepPlatform for EspIdfLightSleep {
    fn light_sleep(&self, duration_us: u64) {
        info!("Entering light sleep for {} microseconds", duration_us);
        unsafe {
            // タイマーウェイクアップを設定
            esp_idf_sys::esp_sleep_enable_timer_wakeup(duration_us);
            
            // [IMPORTANT] ESP32-S3でPSRAMを使用している場合、VDD_SDIOドメインをONに保持する必要がある
            esp_idf_sys::esp_sleep_pd_config(
                esp_idf_sys::esp_sleep_pd_domain_t_ESP_PD_DOMAIN_VDDSDIO,
                esp_idf_sys::esp_sleep_pd_option_t_ESP_PD_OPTION_ON,
            );
            
            // WiFiモデムのPower DomainをOFFに設定（deinit済みのため不要）
            esp_idf_sys::esp_sleep_pd_config(
                esp_idf_sys::esp_sleep_pd_domain_t_ESP_PD_DOMAIN_MODEM,
                esp_idf_sys::esp_sleep_pd_option_t_ESP_PD_OPTION_OFF,
            );
            
            // RTC周辺機器ドメインをOFFに設定（GPIO HoldはRTC_PERIPHなしでも保持される）
            esp_idf_sys::esp_sleep_pd_config(
                esp_idf_sys::esp_sleep_pd_domain_t_ESP_PD_DOMAIN_RTC_PERIPH,
                esp_idf_sys::esp_sleep_pd_option_t_ESP_PD_OPTION_OFF,
            );
            
            // ライトスリープを開始（CPU実行を一時停止し、復帰後はこの次から再開される）
            info!("---[ENTERING LIGHT SLEEP]---");
            // ログのフラッシュを促すため、ごくわずかに待機
            esp_idf_sys::vTaskDelay(10); 

            let err = esp_idf_sys::esp_light_sleep_start();
            
            // 復帰確認用: LEDを消灯
            esp_idf_sys::gpio_set_level(21, 1);

            // 復帰直後、ホスト側のUSBシリアルスタックの再認識を待つため、長めに待機
            // (USB-Serial-JTAGが切断されるため再接続の時間が必要)
            esp_idf_sys::vTaskDelay(200); 
            
            // 復帰直後：まずは全ピンのHoldを解除（これをしないとUART出力すら出ない場合がある）
            // Xiao ESP32S3 Senseで使用しているピンのHoldを解除
            for pin in [2, 5, 21, 10, 15, 17, 18, 16, 14, 12, 11, 48, 38, 47, 13, 40, 39] {
                esp_idf_sys::gpio_hold_dis(pin as i32);
            }

            if err != 0 {
                // エラー時はログを試みるが、通常は成功するはず
                // log::error!("!!! LIGHT SLEEP FAILURE: error code {} !!!", err);
            }
            
            // LED信号（高速3回点滅）で物理的に復帰を通知
            
            // LED信号（高速3回点滅）で物理的に復帰を通知
            // インスタンスを使わず raw GPIO (GPIO 21) で操作
            for _ in 0..3 {
                esp_idf_sys::gpio_set_level(21, 0); // ON
                esp_idf_sys::vTaskDelay(5);
                esp_idf_sys::gpio_set_level(21, 1); // OFF
                esp_idf_sys::vTaskDelay(5);
            }

            info!("**********************************************");
            info!("*** WAKING UP FROM LIGHT SLEEP (SUCCESS) ***");
            info!("**********************************************");
        }
    }
}
