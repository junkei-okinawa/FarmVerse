use crate::mac_address::MacAddress;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::espnow::EspNow;
use log::{debug, error, info, warn};
use std::sync::{Arc, Mutex};

// ESP-NOW関連定数
/// ESP-NOWメモリ不足エラーコード
const ESP_ERR_ESPNOW_NO_MEM: i32 = 12391;

/// ESP-NOW送信エラー
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum EspNowError {
    #[error("ESP-IDFエラー: {0}")]
    EspError(esp_idf_sys::EspError),

    #[error("ESP-NOWピア追加エラー: {0}")]
    AddPeerFailed(esp_idf_sys::EspError),

    #[error("ESP-NOW送信エラー: {0}")]
    SendFailed(esp_idf_sys::EspError),

    #[error("送信タイムアウトエラー")]
    SendTimeout,
}

/// ESP-NOW送信機
pub struct EspNowSender {
    esp_now: Arc<Mutex<EspNow<'static>>>,
    peer_mac: MacAddress,
}

impl std::fmt::Debug for EspNowSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EspNowSender")
            .field("peer_mac", &self.peer_mac)
            .finish()
    }
}

impl EspNowSender {
    /// 新しいESP-NOW送信機を初期化します
    pub fn new(esp_now: Arc<Mutex<EspNow<'static>>>, peer_mac: MacAddress) -> Result<Self, EspNowError> {
        let sender = Self { esp_now, peer_mac };
        sender.add_peer(&sender.peer_mac)?;
        Ok(sender)
    }

    /// ピアを追加します
    fn add_peer(&self, peer_mac: &MacAddress) -> Result<(), EspNowError> {
        info!("ESP-NOWピア追加: MAC={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", 
              peer_mac.0[0], peer_mac.0[1], peer_mac.0[2], 
              peer_mac.0[3], peer_mac.0[4], peer_mac.0[5]);

        let peer_info = esp_idf_svc::espnow::PeerInfo {
            peer_addr: peer_mac.0,
            channel: 0,
            ifidx: esp_idf_svc::wifi::WifiDeviceId::Sta.into(),
            encrypt: false,
            lmk: [0u8; 16],  // 16バイトの配列
            priv_: std::ptr::null_mut(),  // void ポインタ
        };

        {
            let esp_now_guard = self.esp_now.lock().unwrap();
            esp_now_guard.add_peer(peer_info)
                .map_err(|e| {
                    error!("ESP-NOWピア追加失敗: {:?}", e);
                    EspNowError::AddPeerFailed(e)
                })?;
        }

        info!("ESP-NOWピア追加成功");
        Ok(())
    }

    /// データを送信
    pub fn send(&self, data: &[u8], _timeout_ms: u32) -> Result<(), EspNowError> {
        // データサイズの事前チェック
        if data.len() > 250 {
            error!("ESP-NOWデータサイズ制限超過: {}バイト (最大250バイト)", data.len());
            return Err(EspNowError::SendFailed(esp_idf_sys::EspError::from(esp_idf_sys::ESP_ERR_INVALID_ARG).unwrap()));
        }
        
        {
            let esp_now_guard = self.esp_now.lock().unwrap();
            match esp_now_guard.send(self.peer_mac.0, data) {
                Ok(()) => {
                    // 正常送信時は詳細ログを出力しない（スパム防止）
                    Ok(())
                }
                Err(e) => {
                    error!("ESP-NOW送信失敗: {:?} (データ長: {}バイト)", e, data.len());
                    error!("ESP-NOWエラーコード: {}, ピアMAC: {:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", 
                           e.code(), 
                           self.peer_mac.0[0], self.peer_mac.0[1], self.peer_mac.0[2], 
                           self.peer_mac.0[3], self.peer_mac.0[4], self.peer_mac.0[5]);
                    Err(EspNowError::SendFailed(e))
                }
            }
        }
    }

    /// リトライ機能付きのデータ送信（メモリ不足対策強化版）
    pub fn send_with_retry(
        &self,
        data: &[u8],
        timeout_ms: u32,
        max_retries: u8,
    ) -> Result<(), EspNowError> {
        let mut last_error = EspNowError::SendTimeout;
        
        for attempt in 1..=max_retries {
            match self.send(data, timeout_ms) {
                Ok(()) => {
                    // 成功時は最初の試行以外でログ出力
                    if attempt > 1 {
                        info!("ESP-NOW送信成功 (試行 {})", attempt);
                    }
                    return Ok(());
                }
                Err(EspNowError::SendFailed(esp_err)) => {
                    if esp_err.code() == ESP_ERR_ESPNOW_NO_MEM { // ESP_ERR_ESPNOW_NO_MEM
                        error!("ESP-NOWメモリ不足 (試行 {}/{}): {}", attempt, max_retries, esp_err);
                        last_error = EspNowError::SendFailed(esp_err);
                        
                        if attempt < max_retries {
                            // メモリ不足時は段階的に長い待機時間（バッファクリア待ち）
                            let memory_delay = 800 + (attempt as u32 * 400); // 800ms, 1200ms, 1600ms...
                            info!("メモリ不足回復待機: {}ms後にリトライします...", memory_delay);
                            FreeRtos::delay_ms(memory_delay);
                        }
                    } else {
                        error!("ESP-NOW送信失敗 (試行 {}/{}): {:?}", attempt, max_retries, esp_err);
                        last_error = EspNowError::SendFailed(esp_err);
                        
                        if attempt < max_retries {
                            // 通常エラー時の待機時間
                            let delay_ms = 300 * attempt as u32; // 段階的に延長
                            info!("{}ms後にリトライします...", delay_ms);
                            FreeRtos::delay_ms(delay_ms);
                        }
                    }
                }
                Err(e) => {
                    error!("ESP-NOW送信失敗 (試行 {}/{}): {:?}", attempt, max_retries, e);
                    last_error = e;
                    
                    if attempt < max_retries {
                        let delay_ms = 300 * attempt as u32;
                        info!("{}ms後にリトライします...", delay_ms);
                        FreeRtos::delay_ms(delay_ms);
                    }
                }
            }
        }
        
        error!("ESP-NOW送信: 全ての試行が失敗しました ({}回試行)", max_retries);
        Err(last_error)
    }

    /// 画像データをチャンクに分割して送信する（アダプティブ実装・sensor_data_receiver準拠）
    pub fn send_image_chunks(
        &self,
        data: Vec<u8>,
        initial_chunk_size: usize,
        delay_between_chunks_ms: u32,
    ) -> Result<(), EspNowError> {
        // フレームヘッダーサイズを計算
        const FRAME_OVERHEAD: usize = 4 + 6 + 1 + 4 + 4 + 4 + 4; // START_MARKER + MAC + TYPE + SEQ + LEN + CHECKSUM + END_MARKER = 27バイト
        const ESP_NOW_MAX_SIZE: usize = 250; // ESP-NOWの最大サイズ
        
        // 有効なペイロードサイズを計算（フレームヘッダーを除く）
        let max_payload_size = ESP_NOW_MAX_SIZE - FRAME_OVERHEAD; // 223バイト
        let safe_initial_payload = if initial_chunk_size > max_payload_size {
            max_payload_size
        } else {
            initial_chunk_size
        };
        
        // 段階的にペイロードサイズを小さくして試行
        let payload_sizes = [safe_initial_payload, 150, 100, 50, 30];
        
        for &payload_size in &payload_sizes {
            // フレーム全体のサイズを確認
            let total_frame_size = FRAME_OVERHEAD + payload_size;
            if total_frame_size > ESP_NOW_MAX_SIZE {
                continue; // スキップして次の小さなサイズを試行
            }
            
            info!("画像データを{}バイトのペイロードに分割して送信開始（フレーム全体:{}バイト）", payload_size, total_frame_size);
            info!("総データサイズ: {}バイト", data.len());
            let total_chunks = (data.len() + payload_size - 1) / payload_size;
            
            let mut success = true;
            
            for (i, chunk) in data.chunks(payload_size).enumerate() {
                if i % 20 == 0 { // 20チャンクごとに進捗表示
                    info!("チャンク送信進捗: {}/{}", i + 1, total_chunks);
                }
                
                // 最初のチャンクの詳細を出力
                if i == 0 {
                    info!("最初のチャンク詳細: サイズ={}バイト, プレビュー={:02X?}", chunk.len(), &chunk[..std::cmp::min(10, chunk.len())]);
                }
                
                // sensor_data_receiver準拠のフレーム構造で送信
                let frame = match self.create_sensor_data_frame(2, chunk) { // FRAME_TYPE_DATA = 2
                    Ok(f) => f,
                    Err(e) => {
                        error!("チャンク{} フレーム作成失敗: {:?}", i + 1, e);
                        success = false;
                        break;
                    }
                };
                
                // 重要なチャンク（最初の3チャンク）は重複送信で信頼性向上
                let retry_count = if i < 3 { 
                    warn!("重要チャンク{}: 信頼性向上のため複数回送信", i + 1);
                    2 // 重要チャンクは2回送信
                } else { 
                    1 // 通常チャンクは1回
                };
                
                let mut chunk_success = false;
                for attempt in 1..=retry_count {
                    match self.send_with_retry(&frame, 1000, 3) {
                        Ok(()) => {
                            if retry_count > 1 {
                                info!("重要チャンク{} 送信成功 (試行{}/{})", i + 1, attempt, retry_count);
                            }
                            chunk_success = true;
                            break;
                        }
                        Err(e) => {
                            if attempt == retry_count {
                                error!("チャンク{} 送信失敗 (ペイロードサイズ{}バイト): {:?}", i + 1, payload_size, e);
                            } else {
                                warn!("重要チャンク{} 送信失敗 (試行{}/{}), 再送します", i + 1, attempt, retry_count);
                                FreeRtos::delay_ms(100); // 重要チャンク再送間隔
                            }
                        }
                    }
                }
                
                if !chunk_success {
                    success = false;
                    break;
                }
                
                // チャンク間の遅延
                FreeRtos::delay_ms(delay_between_chunks_ms);
            }
            
            if success {
                info!("画像データ送信完了: {}チャンク送信 (ペイロードサイズ: {}バイト)", total_chunks, payload_size);
                return Ok(());
            } else {
                warn!("ペイロードサイズ{}バイトで送信失敗、より小さなサイズで再試行します", payload_size);
                FreeRtos::delay_ms(1000); // 再試行前の待機
            }
        }
        
        error!("全てのペイロードサイズで送信失敗");
        Err(EspNowError::SendTimeout)
    }

    /// メタデータを含むハッシュフレームを送信（sensor_data_receiver準拠フレーム形式）
    pub fn send_hash_frame(
        &self,
        hash: &str,
        voltage_percentage: u8,
        temperature_celsius: Option<f32>,
        tds_voltage: Option<f32>,
        timestamp: &str,
    ) -> Result<(), EspNowError> {
        // 温度データがない場合はダミー値-999.0を使用
        let temp_data = temperature_celsius.unwrap_or(-999.0);
        // TDS電圧データがない場合はダミー値-999.0を使用
        let tds_data = tds_voltage.unwrap_or(-999.0);
        let hash_data = format!("HASH:{},VOLT:{},TEMP:{:.1},TDS_VOLT:{:.1},{}", hash, voltage_percentage, temp_data, tds_data, timestamp);
        info!("ハッシュフレーム送信（sensor_data_receiver準拠）: {}", hash_data);
        
        // sensor_data_receiver準拠のフレーム構造で送信
        let frame = self.create_sensor_data_frame(1, hash_data.as_bytes())?; // FRAME_TYPE_HASH = 1
        
        self.send_with_retry(&frame, 1000, 3)?;
        Ok(())
    }

    /// 画像送信終了マーカーを送信（sensor_data_receiver準拠フレーム形式）
    pub fn send_eof_marker(&self) -> Result<(), EspNowError> {
        info!("EOF フレーム送信開始（sensor_data_receiver準拠）");
        
        // sensor_data_receiver準拠のフレーム構造で送信
        let frame = self.create_sensor_data_frame(3, b"EOF")?; // FRAME_TYPE_EOF = 3
        
        // 複数回送信で信頼性を向上
        for attempt in 1..=3 {
            info!("EOF フレーム送信試行 {}/3", attempt);
            
            match self.send_with_retry(&frame, 1000, 3) {
                Ok(()) => {
                    info!("EOF フレーム送信成功 (試行 {})", attempt);
                    FreeRtos::delay_ms(200);
                    break;
                }
                Err(e) => {
                    error!("EOF フレーム送信失敗 (試行 {}): {:?}", attempt, e);
                    if attempt == 3 {
                        return Err(e);
                    }
                    FreeRtos::delay_ms(500);
                }
            }
        }
        
        info!("EOF フレーム送信完了");
        Ok(())
    }
    
    /// sensor_data_receiver準拠のフレーム形式でデータを作成
    /// 
    /// フレーム構造: [START_MARKER][MAC][TYPE][SEQ][LEN][DATA][CHECKSUM][END_MARKER]
    /// - START_MARKER: [0xFA, 0xCE, 0xAA, 0xBB] (4 bytes)
    /// - MAC: 送信元MACアドレス (6 bytes)  
    /// - TYPE: フレームタイプ (1 byte) - 1=HASH, 2=DATA, 3=EOF
    /// - SEQ: シーケンス番号 (4 bytes, little-endian)
    /// - LEN: データ長 (4 bytes, little-endian)
    /// - DATA: ペイロードデータ (可変長)
    /// - CHECKSUM: チェックサム (4 bytes, little-endian)
    /// - END_MARKER: [0xCD, 0xEF, 0x56, 0x78] (4 bytes)
    fn create_sensor_data_frame(&self, frame_type: u8, data: &[u8]) -> Result<Vec<u8>, EspNowError> {
        // フレームマーカー定数（sensor_data_receiver準拠）
        const START_MARKER: [u8; 4] = [0xFA, 0xCE, 0xAA, 0xBB];
        const END_MARKER: [u8; 4] = [0xCD, 0xEF, 0x56, 0x78];
        
        let mut frame = Vec::new();
        
        // 1. START_MARKER
        frame.extend_from_slice(&START_MARKER);
        
        // 2. MAC アドレス (6 bytes) - 実際のMAC取得
        let mac_address = self.get_local_mac_address();
        frame.extend_from_slice(&mac_address);
        
        // 3. フレームタイプ (1 byte)
        frame.push(frame_type);
        
        // 4. シーケンス番号 (4 bytes, little-endian)
        let sequence = self.get_next_sequence_number();
        frame.extend_from_slice(&sequence.to_le_bytes());
        
        // 5. データ長 (4 bytes, little-endian)
        let data_len = data.len() as u32;
        frame.extend_from_slice(&data_len.to_le_bytes());
        
        // 6. データ本体
        frame.extend_from_slice(data);
        
        // 7. チェックサム計算・追加 (4 bytes, little-endian)
        let checksum = self.calculate_xor_checksum(data);
        frame.extend_from_slice(&checksum.to_le_bytes());
        
        // 8. END_MARKER
        frame.extend_from_slice(&END_MARKER);
        
        debug!("sensor_data_receiver準拠フレーム作成: type={}, data_len={}, checksum=0x{:08X}, total_frame_len={}", 
               frame_type, data_len, checksum, frame.len());
        
        Ok(frame)
    }
    
    /// ローカルMACアドレスを取得
    fn get_local_mac_address(&self) -> [u8; 6] {
        // ESP32のWiFi MACアドレスを取得
        let mut mac = [0u8; 6];
        unsafe {
            let result = esp_idf_svc::sys::esp_wifi_get_mac(
                esp_idf_svc::sys::wifi_interface_t_WIFI_IF_STA, 
                mac.as_mut_ptr()
            );
            if result != 0 {
                warn!("MACアドレス取得失敗、デフォルト値を使用: {:?}", result);
                // デフォルトMAC（テスト用）
                mac = [0x24, 0x6F, 0x28, 0x12, 0x34, 0x56];
            }
        }
        mac
    }
    
    /// シーケンス番号を取得・インクリメント
    fn get_next_sequence_number(&self) -> u32 {
        // TODO: 実際のシーケンス番号管理実装
        // 現在は簡単な固定値
        0x00000001
    }
    
    /// XORベースのチェックサム計算（sensor_data_receiver準拠）
    fn calculate_xor_checksum(&self, data: &[u8]) -> u32 {
        let mut checksum: u32 = 0;
        for chunk in data.chunks(4) {
            let mut val: u32 = 0;
            for (i, &b) in chunk.iter().enumerate() {
                val |= (b as u32) << (i * 8);
            }
            checksum ^= val;
        }
        checksum
    }
}
