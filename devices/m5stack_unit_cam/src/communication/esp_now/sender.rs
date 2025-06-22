use crate::mac_address::MacAddress;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::espnow::EspNow;
use log::{error, info};
use std::sync::{Arc, Mutex};

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
            let mut esp_now_guard = self.esp_now.lock().unwrap();
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
        {
            let esp_now_guard = self.esp_now.lock().unwrap();
            esp_now_guard.send(self.peer_mac.0, data)
                .map_err(|e| {
                    error!("ESP-NOW送信失敗: {:?}", e);
                    EspNowError::SendFailed(e)
                })?;
        }
        Ok(())
    }

    /// リトライ機能付きのデータ送信
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
                    return Ok(());
                }
                Err(e) => {
                    error!("ESP-NOW送信失敗 (試行 {}): {:?}", attempt, e);
                    last_error = e;
                    
                    if attempt < max_retries {
                        // リトライ前に少し待機
                        FreeRtos::delay_ms(200 * attempt as u32);
                    }
                }
            }
        }
        
        error!("ESP-NOW送信: 全ての試行が失敗しました");
        Err(last_error)
    }

    /// 画像データをチャンクに分割して送信する
    pub fn send_image_chunks(
        &self,
        data: Vec<u8>,
        chunk_size: usize,
        delay_between_chunks_ms: u32,
    ) -> Result<(), EspNowError> {
        info!("画像データを{}バイトのチャンクに分割して送信開始", chunk_size);
        let total_chunks = (data.len() + chunk_size - 1) / chunk_size;
        
        for (i, chunk) in data.chunks(chunk_size).enumerate() {
            if i % 10 == 0 { // 10チャンクごとに進捗表示
                info!("チャンク送信進捗: {}/{}", i + 1, total_chunks);
            }
            
            // メモリ不足対策：リトライ回数を減らして1回のみ試行
            self.send_with_retry(chunk, 1000, 1)?;
            
            // チャンク間のディレイ（メモリクリア時間を確保）
            FreeRtos::delay_ms(delay_between_chunks_ms);
        }
        
        info!("画像データ送信完了: {}チャンク送信", total_chunks);
        Ok(())
    }

    /// メタデータを含むハッシュフレームを送信
    pub fn send_hash_frame(
        &self,
        hash: &str,
        voltage_percentage: u8,
        timestamp: &str,
    ) -> Result<(), EspNowError> {
        // M5Stack Unit Camには温度センサーがないため、ダミー値を使用
        let temp_celsius = 25.0; // ダミー値（室温想定）
        let hash_data = format!("HASH:{},VOLT:{},TEMP:{:.1},{}", hash, voltage_percentage, temp_celsius, timestamp);
        info!("ハッシュフレーム送信: {}", hash_data);
        
        self.send_with_retry(hash_data.as_bytes(), 1000, 3)?;
        Ok(())
    }

    /// 画像送信終了マーカーを送信
    pub fn send_eof_marker(&self) -> Result<(), EspNowError> {
        let eof_marker = b"EOF";
        info!("EOF マーカー送信");
        
        self.send_with_retry(eof_marker, 1000, 3)?;
        Ok(())
    }
}
