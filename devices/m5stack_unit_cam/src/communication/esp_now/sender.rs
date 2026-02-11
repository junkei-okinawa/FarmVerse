use crate::mac_address::MacAddress;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::espnow::EspNow;
use log::{error, info, warn};
use std::sync::{Arc, Mutex};

const START_MARKER: [u8; 4] = [0xFA, 0xCE, 0xAA, 0xBB];
const END_MARKER: [u8; 4] = [0xCD, 0xEF, 0x56, 0x78];

// ESP-NOW関連定数
/// ESP-NOWメモリ不足エラーコード
const ESP_ERR_ESPNOW_NO_MEM: i32 = 12391;

/// ESP-NOW送信エラー
#[derive(Debug, thiserror::Error)]
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
    sequence_number: Mutex<u32>,
}

impl EspNowSender {
    /// 新しいESP-NOW送信機を初期化します
    pub fn new(esp_now: Arc<Mutex<EspNow<'static>>>, peer_mac: MacAddress) -> Result<Self, EspNowError> {
        let sender = Self {
            esp_now,
            peer_mac,
            sequence_number: Mutex::new(1),
        };
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

    /// 画像データをチャンクに分割して送信する（アダプティブ実装）
    pub fn send_image_chunks(
        &self,
        data: Vec<u8>,
        initial_chunk_size: usize,
        delay_between_chunks_ms: u32,
    ) -> Result<(), EspNowError> {
        // フレームヘッダーサイズを計算
        const FRAME_OVERHEAD: usize = 4 + 6 + 1 + 4 + 4 + 4 + 4; // START + MAC + TYPE + SEQ + LEN + CHECKSUM + END = 27
        const ESP_NOW_MAX_SIZE: usize = 250;

        // 有効なペイロードサイズを計算
        let max_payload_size = ESP_NOW_MAX_SIZE - FRAME_OVERHEAD; // 223 bytes
        let safe_initial_payload = if initial_chunk_size > max_payload_size {
            max_payload_size
        } else {
            initial_chunk_size
        };

        // 段階的にペイロードサイズを小さくして試行
        let payload_sizes = [safe_initial_payload, 150, 100, 50, 30];

        for &payload_size in &payload_sizes {
            let total_frame_size = FRAME_OVERHEAD + payload_size;
            if total_frame_size > ESP_NOW_MAX_SIZE {
                continue;
            }

            info!(
                "画像データを{}バイトのペイロードに分割して送信開始（フレーム全体:{}バイト）",
                payload_size,
                total_frame_size
            );
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
        
        error!("全てのチャンクサイズで送信失敗");
        Err(EspNowError::SendTimeout)
    }

    /// メタデータを含むハッシュフレームを送信
    pub fn send_hash_frame(
        &self,
        hash: &str,
        voltage_percentage: u8,
        temperature_celsius: Option<f32>,
        tds_voltage: Option<f32>,
        timestamp: &str,
    ) -> Result<(), EspNowError> {
        let temp_data = temperature_celsius.unwrap_or(-999.0);
        let tds_data = tds_voltage.unwrap_or(-999.0);
        let hash_data = format!(
            "HASH:{},VOLT:{},TEMP:{:.1},TDS_VOLT:{:.1},{}",
            hash,
            voltage_percentage,
            temp_data,
            tds_data,
            timestamp
        );
        info!("ハッシュフレーム送信（sensor_data_receiver準拠）: {}", hash_data);

        let frame = self.create_sensor_data_frame(1, hash_data.as_bytes())?; // FRAME_TYPE_HASH = 1
        self.send_with_retry(&frame, 1000, 3)?;
        Ok(())
    }

    /// 画像送信終了マーカーを送信
    pub fn send_eof_marker(&self) -> Result<(), EspNowError> {
        info!("EOF フレーム送信開始（sensor_data_receiver準拠）");

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

    fn create_sensor_data_frame(&self, frame_type: u8, data: &[u8]) -> Result<Vec<u8>, EspNowError> {
        let mac_address = self.get_local_mac_address();
        let sequence = self.get_next_sequence_number();
        Ok(build_sensor_data_frame(frame_type, mac_address, sequence, data))
    }

    fn get_local_mac_address(&self) -> [u8; 6] {
        let mut mac = [0u8; 6];
        unsafe {
            let result = esp_idf_sys::esp_wifi_get_mac(
                esp_idf_sys::wifi_interface_t_WIFI_IF_STA,
                mac.as_mut_ptr(),
            );
            if result != 0 {
                warn!("MACアドレス取得失敗、デフォルト値を使用: {:?}", result);
                mac = [0x24, 0x6F, 0x28, 0x12, 0x34, 0x56];
            }
        }
        mac
    }

    fn get_next_sequence_number(&self) -> u32 {
        let mut guard = self.sequence_number.lock().unwrap();
        let current = *guard;
        *guard = guard.wrapping_add(1);
        current
    }

    fn calculate_xor_checksum(&self, data: &[u8]) -> u32 {
        calculate_xor_checksum(data)
    }
}

fn build_sensor_data_frame(
    frame_type: u8,
    mac_address: [u8; 6],
    sequence: u32,
    data: &[u8],
) -> Vec<u8> {
    let mut frame = Vec::new();

    frame.extend_from_slice(&START_MARKER);
    frame.extend_from_slice(&mac_address);
    frame.push(frame_type);
    frame.extend_from_slice(&sequence.to_le_bytes());

    let data_len = data.len() as u32;
    frame.extend_from_slice(&data_len.to_le_bytes());
    frame.extend_from_slice(data);

    let checksum = calculate_xor_checksum(data);
    frame.extend_from_slice(&checksum.to_le_bytes());
    frame.extend_from_slice(&END_MARKER);

    frame
}

fn calculate_xor_checksum(data: &[u8]) -> u32 {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_xor_checksum_little_endian_chunks() {
        // 0x04030201 ^ 0x08070605 = 0x0C040404
        let data = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08];
        let checksum = calculate_xor_checksum(&data);
        assert_eq!(checksum, 0x0C040404);
    }

    #[test]
    fn test_build_sensor_data_frame_structure() {
        let mac = [0x10, 0x11, 0x12, 0x13, 0x14, 0x15];
        let sequence = 0x01020304;
        let payload = [0xAA, 0xBB, 0xCC];
        let frame = build_sensor_data_frame(2, mac, sequence, &payload);

        let expected_len = 4 + 6 + 1 + 4 + 4 + payload.len() + 4 + 4;
        assert_eq!(frame.len(), expected_len);

        assert_eq!(&frame[0..4], &START_MARKER);
        assert_eq!(&frame[4..10], &mac);
        assert_eq!(frame[10], 2);
        assert_eq!(&frame[11..15], &sequence.to_le_bytes());
        assert_eq!(&frame[15..19], &(payload.len() as u32).to_le_bytes());
        assert_eq!(&frame[19..22], &payload);

        let checksum_offset = 19 + payload.len();
        let checksum = calculate_xor_checksum(&payload);
        assert_eq!(
            &frame[checksum_offset..checksum_offset + 4],
            &checksum.to_le_bytes()
        );
        assert_eq!(&frame[checksum_offset + 4..checksum_offset + 8], &END_MARKER);
    }
}
