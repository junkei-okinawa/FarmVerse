/// Device Stream Manager for Multi-Device Streaming
/// 
/// 複数のデバイス（カメラ）からの同時ストリーミングを管理します。
/// 各デバイスごとに独立したストリームを維持し、並行処理を可能にします。

use super::{StreamingError, StreamingResult, StreamingStatistics};
use super::buffer::{StreamingBuffer, BufferedData};
use super::frame_processor::{FrameProcessor, ProcessingResult};
use log::{debug, info, warn};
use std::collections::HashMap;

/// デバイス固有のストリーム状態
#[derive(Debug)]
pub struct DeviceStream {
    /// デバイスのMACアドレス
    pub mac_address: [u8; 6],
    /// デバイス名（識別用）
    pub name: String,
    /// フレームプロセッサ
    frame_processor: FrameProcessor,
    /// ストリーミングバッファ
    buffer: StreamingBuffer,
    /// 統計情報
    statistics: StreamingStatistics,
    /// 最後のアクティビティ時刻
    last_activity: u64,
    /// ストリーム状態
    status: StreamStatus,
}

impl DeviceStream {
    /// 新しいデバイスストリームを作成
    pub fn new(mac_address: [u8; 6], name: String) -> Self {
        DeviceStream {
            mac_address,
            name,
            frame_processor: FrameProcessor::new(),
            buffer: StreamingBuffer::new(),
            statistics: StreamingStatistics::new(),
            last_activity: get_current_timestamp(),
            status: StreamStatus::Active,
        }
    }
    
    /// 受信データを処理
    pub fn process_received_data(&mut self, data: &[u8]) -> Vec<ProcessedFrame> {
        self.last_activity = get_current_timestamp();
        self.statistics.count_frame_received();
        
        debug!("DeviceStream[{}]: processing {} bytes", self.name, data.len());
        
        // フレームプロセッサでデータを処理
        let processing_results = self.frame_processor.process_data(data);
        let mut processed_frames = Vec::new();
        
        for result in processing_results {
            match result {
                ProcessingResult::FrameProcessed { header, payload, total_bytes } => {
                    // 正常なフレーム処理
                    self.statistics.count_frame_processed(total_bytes);
                    
                    let processed_frame = ProcessedFrame {
                        mac_address: self.mac_address,
                        device_name: self.name.clone(),
                        sequence: header.sequence,
                        payload,
                        timestamp: self.last_activity,
                    };
                    
                    debug!("DeviceStream[{}]: frame processed - seq: {}, size: {} bytes",
                           self.name, header.sequence, processed_frame.payload.len());
                    
                    processed_frames.push(processed_frame);
                }
                ProcessingResult::IncompleteFrame { needed_bytes } => {
                    debug!("DeviceStream[{}]: incomplete frame, need {} more bytes",
                           self.name, needed_bytes);
                    // 不完全フレームは次のデータ到着を待つ
                }
                ProcessingResult::FrameError { error, consumed_bytes } => {
                    self.statistics.count_error(&error);
                    warn!("DeviceStream[{}]: frame error - {}, consumed {} bytes",
                          self.name, error, consumed_bytes);
                }
            }
        }
        
        processed_frames
    }
    
    /// バッファにデータを追加
    pub fn add_to_buffer(&mut self, data: &[u8]) -> StreamingResult<()> {
        let buffered_data = BufferedData::new(data, self.mac_address)?;
        self.buffer.push(buffered_data)
    }
    
    /// バッファからデータを取得
    pub fn get_from_buffer(&mut self) -> Option<BufferedData> {
        self.buffer.pop()
    }
    
    /// ストリームの統計情報を取得
    pub fn statistics(&self) -> &StreamingStatistics {
        &self.statistics
    }
    
    /// バッファの使用状況を取得
    pub fn buffer_usage(&self) -> (usize, usize) {
        self.buffer.usage()
    }
    
    /// 最後のアクティビティからの経過時間（ミリ秒）
    pub fn time_since_last_activity(&self) -> u64 {
        get_current_timestamp() - self.last_activity
    }
    
    /// ストリーム状態を取得
    pub fn status(&self) -> StreamStatus {
        self.status
    }
    
    /// ストリーム状態を設定
    pub fn set_status(&mut self, status: StreamStatus) {
        self.status = status;
        info!("DeviceStream[{}]: status changed to {:?}", self.name, status);
    }
    
    /// 古いデータをクリーンアップ
    pub fn cleanup(&mut self, timeout_ms: u64) -> usize {
        self.buffer.cleanup_old_data(timeout_ms)
    }
    
    /// ストリームをリセット
    pub fn reset(&mut self) {
        self.frame_processor.clear_buffer();
        self.buffer.clear();
        self.statistics.reset();
        self.last_activity = get_current_timestamp();
        self.status = StreamStatus::Active;
        info!("DeviceStream[{}]: reset completed", self.name);
    }
}

/// ストリームの状態
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StreamStatus {
    /// アクティブ（正常動作中）
    Active,
    /// アイドル（一定時間データなし）
    Idle,
    /// エラー状態
    Error,
    /// 停止中
    Stopped,
}

/// 処理済みフレーム
#[derive(Debug, Clone)]
pub struct ProcessedFrame {
    /// 送信元MACアドレス
    pub mac_address: [u8; 6],
    /// デバイス名
    pub device_name: String,
    /// シーケンス番号
    pub sequence: u16,
    /// ペイロードデータ
    pub payload: Vec<u8>,
    /// 処理時刻
    pub timestamp: u64,
}

impl ProcessedFrame {
    /// MACアドレスを文字列形式で取得
    pub fn mac_string(&self) -> String {
        format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                self.mac_address[0], self.mac_address[1], self.mac_address[2],
                self.mac_address[3], self.mac_address[4], self.mac_address[5])
    }
}

/// 複数デバイスのストリーム管理
pub struct DeviceStreamManager {
    /// デバイス別ストリーム（MAC address -> DeviceStream）
    streams: HashMap<[u8; 6], DeviceStream>,
    /// 管理設定
    config: StreamManagerConfig,
    /// 全体統計
    global_stats: StreamingStatistics,
}

impl DeviceStreamManager {
    /// 新しいデバイスストリーム管理者を作成
    pub fn new(config: StreamManagerConfig) -> Self {
        DeviceStreamManager {
            streams: HashMap::new(),
            config,
            global_stats: StreamingStatistics::new(),
        }
    }
    
    /// デバイスを登録
    pub fn register_device(&mut self, mac_address: [u8; 6], name: String) -> StreamingResult<()> {
        if self.streams.contains_key(&mac_address) {
            return Err(StreamingError::Other(
                format!("Device already registered: {:02X?}", mac_address)
            ));
        }
        
        let stream = DeviceStream::new(mac_address, name.clone());
        self.streams.insert(mac_address, stream);
        
        info!("DeviceStreamManager: registered device {} ({:02X?})", name, mac_address);
        Ok(())
    }
    
    /// デバイスを登録解除
    pub fn unregister_device(&mut self, mac_address: &[u8; 6]) -> StreamingResult<()> {
        if let Some(stream) = self.streams.remove(mac_address) {
            info!("DeviceStreamManager: unregistered device {} ({:02X?})", 
                  stream.name, mac_address);
            Ok(())
        } else {
            Err(StreamingError::DeviceNotFound(*mac_address))
        }
    }
    
    /// データを処理（メインエントリポイント）
    pub fn process_data(&mut self, mac_address: [u8; 6], data: &[u8]) -> StreamingResult<Vec<ProcessedFrame>> {
        // デバイスが登録されているか確認
        if !self.streams.contains_key(&mac_address) {
            // 未登録デバイスの場合、自動登録する
            let device_name = format!("Device_{:02X}{:02X}{:02X}", 
                                     mac_address[3], mac_address[4], mac_address[5]);
            self.register_device(mac_address, device_name)?;
        }
        
        // デバイスストリームでデータを処理
        let stream = self.streams.get_mut(&mac_address)
            .ok_or(StreamingError::DeviceNotFound(mac_address))?;
        
        let processed_frames = stream.process_received_data(data);
        
        // 全体統計を更新
        self.global_stats.count_frame_received();
        if !processed_frames.is_empty() {
            for frame in &processed_frames {
                self.global_stats.count_frame_processed(frame.payload.len());
            }
        }
        
        Ok(processed_frames)
    }
    
    /// 登録されているデバイス一覧を取得
    pub fn get_devices(&self) -> Vec<([u8; 6], String, StreamStatus)> {
        self.streams.iter()
            .map(|(mac, stream)| (*mac, stream.name.clone(), stream.status()))
            .collect()
    }
    
    /// 特定デバイスの統計情報を取得
    pub fn get_device_statistics(&self, mac_address: &[u8; 6]) -> StreamingResult<&StreamingStatistics> {
        let stream = self.streams.get(mac_address)
            .ok_or(StreamingError::DeviceNotFound(*mac_address))?;
        Ok(stream.statistics())
    }
    
    /// 全体統計情報を取得
    pub fn global_statistics(&self) -> &StreamingStatistics {
        &self.global_stats
    }
    
    /// 非アクティブなデバイスをクリーンアップ
    pub fn cleanup_inactive_devices(&mut self) -> usize {
        let mut removed_count = 0;
        let current_time = get_current_timestamp();
        
        let inactive_devices: Vec<[u8; 6]> = self.streams.iter()
            .filter(|(_, stream)| {
                current_time - stream.last_activity > self.config.device_timeout_ms
            })
            .map(|(mac, _)| *mac)
            .collect();
        
        for mac in inactive_devices {
            if let Some(stream) = self.streams.remove(&mac) {
                warn!("DeviceStreamManager: removed inactive device {} (timeout: {}ms)",
                      stream.name, self.config.device_timeout_ms);
                removed_count += 1;
            }
        }
        
        removed_count
    }
    
    /// 全デバイスのバッファクリーンアップ
    pub fn cleanup_all_buffers(&mut self) -> usize {
        let mut total_cleaned = 0;
        for stream in self.streams.values_mut() {
            total_cleaned += stream.cleanup(self.config.buffer_timeout_ms);
        }
        
        if total_cleaned > 0 {
            debug!("DeviceStreamManager: cleaned up {} old buffer items", total_cleaned);
        }
        
        total_cleaned
    }
    
    /// 特定デバイスのストリームをリセット
    pub fn reset_device_stream(&mut self, mac_address: &[u8; 6]) -> StreamingResult<()> {
        let stream = self.streams.get_mut(mac_address)
            .ok_or(StreamingError::DeviceNotFound(*mac_address))?;
        stream.reset();
        Ok(())
    }
    
    /// 全統計をリセット
    pub fn reset_statistics(&mut self) {
        self.global_stats.reset();
        for stream in self.streams.values_mut() {
            stream.statistics.reset();
        }
        info!("DeviceStreamManager: all statistics reset");
    }
    
    /// 現在のデバイス数を取得
    pub fn device_count(&self) -> usize {
        self.streams.len()
    }
    
    /// 総バッファ使用量を取得
    pub fn total_buffer_usage(&self) -> (usize, usize) {
        let mut total_used = 0;
        let mut total_capacity = 0;
        
        for stream in self.streams.values() {
            let (used, capacity) = stream.buffer_usage();
            total_used += used;
            total_capacity += capacity;
        }
        
        (total_used, total_capacity)
    }
}

/// ストリーム管理設定
#[derive(Debug, Clone)]
pub struct StreamManagerConfig {
    /// デバイスタイムアウト時間（ミリ秒）
    pub device_timeout_ms: u64,
    /// バッファタイムアウト時間（ミリ秒）
    pub buffer_timeout_ms: u64,
    /// 最大デバイス数
    pub max_devices: usize,
}

impl Default for StreamManagerConfig {
    fn default() -> Self {
        StreamManagerConfig {
            device_timeout_ms: 300_000, // 5分
            buffer_timeout_ms: 30_000,  // 30秒
            max_devices: 10,            // 最大10デバイス
        }
    }
}

/// 現在のタイムスタンプを取得（ミリ秒）
fn get_current_timestamp() -> u64 {
    unsafe {
        let ticks = esp_idf_svc::sys::xTaskGetTickCount();
        let ms_per_tick = 1000 / esp_idf_svc::sys::configTICK_RATE_HZ;
        (ticks * ms_per_tick) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_stream_creation() {
        let mac = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
        let stream = DeviceStream::new(mac, "Test Device".to_string());
        
        assert_eq!(stream.mac_address, mac);
        assert_eq!(stream.name, "Test Device");
        assert_eq!(stream.status(), StreamStatus::Active);
        assert_eq!(stream.statistics().frames_received, 0);
    }

    #[test]
    fn test_processed_frame_mac_string() {
        let mac = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
        let frame = ProcessedFrame {
            mac_address: mac,
            device_name: "Test".to_string(),
            sequence: 1,
            payload: vec![1, 2, 3],
            timestamp: 0,
        };
        
        assert_eq!(frame.mac_string(), "12:34:56:78:9A:BC");
    }

    #[test]
    fn test_device_stream_manager_basic() {
        let config = StreamManagerConfig::default();
        let mut manager = DeviceStreamManager::new(config);
        
        assert_eq!(manager.device_count(), 0);
        
        // デバイス登録
        let mac = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
        manager.register_device(mac, "Test Device".to_string()).unwrap();
        assert_eq!(manager.device_count(), 1);
        
        // 重複登録エラー
        let result = manager.register_device(mac, "Duplicate".to_string());
        assert!(result.is_err());
        
        // デバイス登録解除
        manager.unregister_device(&mac).unwrap();
        assert_eq!(manager.device_count(), 0);
    }

    #[test]
    fn test_device_stream_manager_auto_registration() {
        let config = StreamManagerConfig::default();
        let mut manager = DeviceStreamManager::new(config);
        
        let mac = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
        let data = vec![1, 2, 3, 4, 5];
        
        // 未登録デバイスからのデータ処理（自動登録）
        let result = manager.process_data(mac, &data);
        assert!(result.is_ok());
        assert_eq!(manager.device_count(), 1);
        
        let devices = manager.get_devices();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].0, mac);
        assert!(devices[0].1.contains("Device_"));
    }

    #[test]
    fn test_stream_manager_config_default() {
        let config = StreamManagerConfig::default();
        assert_eq!(config.device_timeout_ms, 300_000);
        assert_eq!(config.buffer_timeout_ms, 30_000);
        assert_eq!(config.max_devices, 10);
    }
}
