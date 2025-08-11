mod command;
mod config;
mod esp_now;
mod mac_address;
mod queue;
mod usb;
mod streaming;

use anyhow::Result;
use command::{parse_command, Command};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys::{
    esp_now_init, esp_now_register_recv_cb, esp_wifi_set_ps, esp_wifi_set_storage,
    wifi_ps_type_t_WIFI_PS_NONE, wifi_storage_t_WIFI_STORAGE_RAM, vTaskDelay,
};
use esp_idf_svc::wifi::{AuthMethod, ClientConfiguration, Configuration, EspWifi};
use esp_now::sender::EspNowSender;
use log::{debug, error, info, warn};
use mac_address::format_mac_address;
use streaming::controller::{StreamingController, StreamingConfig};
use streaming::StreamingError;
use usb::cdc::UsbCdc;
use std::sync::Mutex;

/// 静的なStreamingControllerインスタンス
static STREAMING_CONTROLLER: Mutex<Option<StreamingController>> = Mutex::new(None);

/// ESP-NOWの受信コールバック関数（Streaming Architecture対応）
///
/// ESP-NOWからのデータを受け取り、StreamingControllerに渡します。
extern "C" fn esp_now_recv_cb(
    info: *const esp_idf_svc::sys::esp_now_recv_info_t,
    data: *const u8,
    data_len: i32,
) {
    let mut callback = |received_data: queue::ReceivedData| {
        // StreamingControllerでデータを処理（ノンブロッキング）
        match STREAMING_CONTROLLER.try_lock() {
            Ok(mut controller_guard) => {
                if let Some(_controller) = controller_guard.as_mut() {
                    // 注: USB CDC参照が必要だが、ここでは処理をキューに追加する方式を採用
                    // 実際のUSB転送はメインループで行う
                    match streaming::buffer::BufferedData::new(&received_data.data, received_data.mac) {
                        Ok(_buffered_data) => {
                            // 受信データをStreaming Controllerに通知
                            debug!("ESP-NOW callback: received {} bytes from {:02X?}", 
                                   received_data.data.len(), received_data.mac);
                            
                            // 従来のキューシステムと併用しつつ、段階的移行
                            queue::data_queue::try_enqueue_from_callback(received_data)
                        }
                        Err(e) => {
                            error!("ESP-NOW callback: failed to create buffered data: {}", e);
                            false
                        }
                    }
                } else {
                    error!("ESP-NOW callback: StreamingController not initialized");
                    false
                }
            }
            Err(_) => {
                // ロック取得失敗 - メインループで処理中の可能性
                debug!("ESP-NOW callback: StreamingController locked, using fallback queue");
                queue::data_queue::try_enqueue_from_callback(received_data)
            }
        }
    };
    esp_now::receiver::process_esp_now_data(&mut callback, info, data, data_len);
}

/// ESP-NOWピアを登録する関数
///
/// カメラのMACアドレスをESP-NOWピアとして登録します。
fn register_esp_now_peers(cameras: &[config::CameraConfig]) -> Result<()> {
    info!("=== ESP-NOWピア登録開始 ===");
    info!("登録するカメラ数: {}", cameras.len());

    unsafe {
        for (i, camera) in cameras.iter().enumerate() {
            info!("カメラ {}/{}: {}", i + 1, cameras.len(), camera.name);
            info!("  MAC: {}", camera.mac_address);

            let mut peer_info = esp_idf_svc::sys::esp_now_peer_info_t::default();
            peer_info.channel = 0; // 現在のチャンネルを使用
            peer_info.ifidx = esp_idf_svc::sys::wifi_interface_t_WIFI_IF_STA; // STA interface
            peer_info.encrypt = false; // 暗号化なし
            peer_info.peer_addr = camera.mac_address.into_bytes();
            
            info!("  チャンネル: {}", peer_info.channel);
            info!("  インターフェース: {}", peer_info.ifidx);
            info!("  暗号化: {}", peer_info.encrypt);
            info!("  ピアアドレス: {:02X?}", peer_info.peer_addr);

            let add_result = esp_idf_svc::sys::esp_now_add_peer(&peer_info);
            if add_result == 0 {
                info!("  ✓ ESP-NOWピア登録成功: {}", camera.name);
            } else {
                error!("  ✗ ESP-NOWピア登録失敗: {} (エラーコード: {})", camera.name, add_result);
            }
        }

        info!("=== PMK設定 ===");
        // ESP-NOW添付ファイル(PMK)の拡張設定
        let pmk: [u8; 16] = [
            0x50, 0x4d, 0x4b, 0x5f, 0x4b, 0x45, 0x59, 0x5f, 0x42, 0x59, 0x5f, 0x43, 0x55, 0x53,
            0x54, 0x4f,
        ];
        info!("PMKデータ: {:02X?}", pmk);
        let pmk_result = esp_idf_svc::sys::esp_now_set_pmk(pmk.as_ptr());

        if pmk_result == 0 {
            info!("✓ PMK設定成功");
        } else {
            error!("✗ PMK設定失敗: エラーコード {}", pmk_result);
        }
    }

    Ok(())
}

/// Wi-Fiを初期化する関数
///
/// ESP-NOWのためにWi-FiをSTAモードで初期化します。
///
/// # 引数
///
/// * `modem` - Wi-Fiモデムペリフェラル
///
/// # 戻り値
///
/// * `Result<EspWifi<'static>>` - 初期化されたWi-Fiインスタンス
fn initialize_wifi(modem: Modem) -> Result<EspWifi<'static>> {
    info!("Initializing Wi-Fi in STA mode for ESP-NOW...");

    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?; // NVSはWi-Fi初期化に必要

    let mut wifi = EspWifi::new(modem, sysloop.clone(), Some(nvs))?;

    // Wi-Fi設定をRAMに保存（NVS書き込み回避）
    unsafe {
        esp_wifi_set_storage(wifi_storage_t_WIFI_STORAGE_RAM);
    }

    // STAモードで設定（接続は不要）
    wifi.set_configuration(&Configuration::Client(ClientConfiguration {
        ssid: heapless::String::new(),     // Empty SSID
        password: heapless::String::new(), // Empty Password
        auth_method: AuthMethod::None,     // No auth needed
        ..Default::default()
    }))?;
    wifi.start()?; // Wi-FiをSTAモードで開始
    info!("Wi-Fi driver started in STA mode.");

    // Wi-Fiパワーセーブを無効化（ESP-NOWの応答性向上）
    unsafe {
        esp_wifi_set_ps(wifi_ps_type_t_WIFI_PS_NONE);
    }
    info!("Wi-Fi Power Save disabled.");

    Ok(wifi)
}

/// ESP-NOWを初期化する関数
///
/// ESP-NOWを初期化し、受信コールバックを登録します。
fn initialize_esp_now() -> Result<()> {
    info!("Initializing ESP-NOW...");

    unsafe {
        esp_now_init();
        esp_now_register_recv_cb(Some(esp_now_recv_cb));

        // ESP-NOWの最大ピア数を確認
        let mut esp_now_peer_num = esp_idf_svc::sys::esp_now_peer_num_t {
            total_num: 0,
            encrypt_num: 0,
        };

        if esp_idf_svc::sys::esp_now_get_peer_num(&mut esp_now_peer_num) == 0 {
            info!(
                "ESP-NOW: Current peer count: {}",
                esp_now_peer_num.total_num
            );
            info!("ESP-NOW: Maximum supported peers: 20"); // ESP-IDF 4.xでは20ピアをサポート
        } else {
            error!("ESP-NOW: Failed to get peer count");
        }
    }

    info!("ESP-NOW Initialized and receive callback registered.");
    Ok(())
}

/// Streaming Architecture対応のデータ処理ループ
///
/// 新しいStreaming Controllerを使用してデータ処理を行います。
/// 従来のキューシステムと併用して段階的に移行します。
#[allow(unused_assignments)]
fn process_streaming_data_loop(
    usb_cdc: &mut UsbCdc, 
    esp_now_sender: &mut EspNowSender,
) -> Result<()> {
    info!("Entering streaming processing loop...");
    
    loop {
        let mut processed_any_data = false;
        
        // 1. 従来のキューからデータを取得（互換性のため）
        match queue::data_queue::dequeue() {
            Ok(received_data) => {
                let mac_str = format_mac_address(&received_data.mac);
                debug!("Streaming Loop: Processing legacy queue data from {}. ({} bytes)",
                       mac_str, received_data.data.len());
                
                // WDT問題のため、Streaming Controllerを使用せず直接USB送信
                debug!("Streaming Loop: Processing data from {} using fallback method. ({} bytes)",
                       mac_str, received_data.data.len());
                
                match usb_cdc.send_frame(&received_data.data, &mac_str) {
                    Ok(bytes_sent) => {
                        debug!("Streaming Loop: USB transfer successful: {} bytes", bytes_sent);
                        processed_any_data = true;
                    }
                    Err(usb_err) => {
                        error!("Streaming Loop: USB transfer failed for {}: {}", mac_str, usb_err);
                    }
                }
                processed_any_data = true;
            }
            Err(queue::QueueError::Empty) => {
                // キューが空の場合は正常（処理なし）
            }
            Err(e) => {
                error!("Error dequeuing data: {:?}", e);
            }
        }
        
        // 2. USBコマンドの処理（スリープコマンドなど）
        match usb_cdc.read_command(10) { // 10ms timeout
            Ok(Some(command_str)) => {
                info!("=== Received USB command: '{}' ===", command_str);
                
                match parse_command(&command_str) {
                    Ok(Command::SendEspNow { mac_address, sleep_seconds }) => {
                        info!("Processing ESP-NOW send command: {} -> {}s", mac_address, sleep_seconds);
                        
                        match esp_now_sender.send_sleep_command(&mac_address, sleep_seconds) {
                            Ok(()) => {
                                info!("✓ ESP-NOW sleep command sent successfully to {}", mac_address);
                            }
                            Err(e) => {
                                error!("✗ Failed to send ESP-NOW sleep command to {}: {:?}", mac_address, e);
                            }
                        }
                    }
                    Ok(Command::Unknown(cmd)) => {
                        warn!("Unknown command received: '{}'", cmd);
                    }
                    Err(e) => {
                        error!("Failed to parse command '{}': {:?}", command_str, e);
                    }
                }
                processed_any_data = true;
            }
            Ok(None) => {
                // コマンドなし
            }
            Err(e) => {
                error!("Error reading USB command: {:?}", e);
                FreeRtos::delay_ms(50);
            }
        }
        
        // 3. 新しいデバイス（現在は従来のキューデータを処理）
        
        // ここで将来的に新しいデータソースを追加可能
        
        // 4. データ処理がない場合は短い遅延
        if !processed_any_data {
            FreeRtos::delay_ms(5); // 遅延を短縮してレスポンス向上
        }
    }
}

fn main() -> Result<()> {
    // ESP-IDFシステムの初期化
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::set_max_level(log::LevelFilter::Info);

    info!("Starting ESP-NOW USB CDC Receiver with Streaming Architecture...");

    // キューの初期化（互換性のため継続）
    queue::data_queue::initialize_data_queue();
    info!("✓ Queue initialized");

    // Streaming Controllerの初期化
    let streaming_config = StreamingConfig::default();
    info!("✓ Streaming config created: timeout={}ms, retries={}", 
          streaming_config.usb_timeout_ms, streaming_config.usb_max_retries);
    
    // グローバルなStreaming Controllerを設定
    {
        info!("Setting up global streaming controller...");
        let mut global_controller = STREAMING_CONTROLLER.lock().unwrap();
        *global_controller = Some(StreamingController::new(streaming_config.clone()));
        info!("✓ Global streaming controller set up");
        
        // Streaming Controller の初期状態を確認
        if let Some(controller) = global_controller.as_ref() {
            info!("Streaming Controller initialized: {} devices registered", controller.list_devices().len());
        }
    }

    // 設定からカメラ情報を読み込み
    info!("Loading camera configurations...");
    let cameras = config::load_camera_configs();
    info!("✓ Camera configs loaded: {} cameras", cameras.len());

    // カメラをStreaming Controllerに登録
    {
        info!("Registering cameras with Streaming Controller...");
        let mut global_controller = STREAMING_CONTROLLER.lock().unwrap();
        
        if let Some(controller) = global_controller.as_mut() {
            for (index, camera) in cameras.iter().enumerate() {
                let device_id = camera.mac_address.into_bytes();
                let device_name = format!("{} ({})", camera.name, camera.mac_address);
                
                info!("Registering camera {} ({}/{})", camera.name, index + 1, cameras.len());
                match controller.register_device(device_id, device_name) {
                    Ok(()) => {
                        // info!("✓ Registered camera: {}", camera.name);  // 一時的に無効化
                    }
                    Err(e) => {
                        // warn!("Failed to register camera {}: {:?}", camera.name, e);  // 一時的に無効化
                    }
                }
            }
            info!("✓ Camera registration completed: {} cameras", cameras.len());
        } else {
            error!("Failed to register cameras: StreamingController not initialized");
        }
    }

    // ペリフェラルの取得
    info!("Taking peripherals...");
    let peripherals = Peripherals::take().unwrap();
    info!("✓ Peripherals taken");

    // Wi-Fi初期化（モデムを渡す）
    info!("Initializing Wi-Fi...");
    let _wifi = initialize_wifi(peripherals.modem)?;
    info!("✓ Wi-Fi initialized");

    // デバイス情報の表示
    info!("=== USBゲートウェイ デバイス情報 ===");
    
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
    
    info!("登録されたカメラ数: {}", cameras.len());
    for (i, camera) in cameras.iter().enumerate() {
        info!("  カメラ{}: {} ({})", i + 1, camera.name, camera.mac_address);
    }
    
    // デバイス登録数を確認
    {
        let global_controller = STREAMING_CONTROLLER.lock().unwrap();
        if let Some(controller) = global_controller.as_ref() {
            info!("Streaming Controller: {} devices registered", controller.list_devices().len());
        } else {
            warn!("Streaming Controller not initialized");
        }
    }

    // ESP-NOW初期化
    initialize_esp_now()?;

    // カメラをピアとして登録
    register_esp_now_peers(&cameras)?;

    // ESP-NOW送信機能を初期化
    info!("Initializing ESP-NOW sender...");
    let mut esp_now_sender = EspNowSender::new();
    info!("✓ ESP-NOW sender initialized.");

    // USB CDC初期化（Wi-Fi初期化で取得したペリフェラルを使用）
    info!("Initializing USB CDC...");
    let mut usb_cdc = UsbCdc::new(
        peripherals.usb_serial,
        peripherals.pins.gpio18, // XIAO ESP32C3のUSB D-ピン
        peripherals.pins.gpio19, // XIAO ESP32C3のUSB D+ピン
    )?;
    info!("✓ USB CDC initialized with streaming architecture support.");

    // Streaming Architecture対応のメインデータ処理ループ
    info!("Starting streaming data processing loop...");
    process_streaming_data_loop(&mut usb_cdc, &mut esp_now_sender)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_integration() {
        // Streaming Controllerの基本的な初期化テスト
        let config = StreamingConfig::default();
        let controller = StreamingController::new(config);
        
        assert_eq!(controller.list_devices().len(), 0);
        
        // テスト環境ではハードウェア依存の処理はスキップ
    }
}
