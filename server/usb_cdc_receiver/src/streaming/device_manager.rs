use std::collections::HashMap;
use super::{StreamingResult, StreamingError, StreamingStatistics};
use crate::esp_now::frame::{Frame, FrameParseError};

#[derive(Debug, Clone)]
pub struct StreamManagerConfig {
    pub buffer_timeout_ms: u64,
}

impl Default for StreamManagerConfig {
    fn default() -> Self {
        Self {
            buffer_timeout_ms: 5000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessedFrame {
    pub sequence: u32,
    /// Contains the full raw ESP-NOW frame bytes (including header/checksum)
    /// ready to be forwarded over USB.
    pub full_frame: Vec<u8>,
    pub mac: [u8; 6],
}

impl ProcessedFrame {
    pub fn mac_string(&self) -> String {
        format!("{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                self.mac[0], self.mac[1], self.mac[2],
                self.mac[3], self.mac[4], self.mac[5])
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamStatus {
    Active,
    Inactive,
}

#[derive(Debug, Clone, Default)]
pub struct GlobalStatistics {
    pub frames_received: u64,
    pub frames_processed: u64,
    pub frames_error: u64,
    pub checksum_error_count: u64,
    pub sequence_error_count: u64,
    pub buffer_full_count: u64,
}

impl GlobalStatistics {
    pub fn success_rate(&self) -> f32 {
        if self.frames_received > 0 {
            (self.frames_processed as f32 / self.frames_received as f32) * 100.0
        } else {
            0.0
        }
    }
}

pub struct DeviceStreamManager {
    config: StreamManagerConfig,
    devices: HashMap<[u8; 6], String>, // Mac -> Name
    stats: GlobalStatistics,
    device_stats: HashMap<[u8; 6], StreamingStatistics>,
}

impl DeviceStreamManager {
    pub fn new(config: StreamManagerConfig) -> Self {
        Self {
            config,
            devices: HashMap::new(),
            stats: GlobalStatistics::default(),
            device_stats: HashMap::new(),
        }
    }

    pub fn process_data(&mut self, mac_address: [u8; 6], data: &[u8]) -> StreamingResult<Vec<ProcessedFrame>> {
        self.stats.frames_received += 1;
        
        // Register device if not exists (auto-discovery) or just track stats
        // In a real app we might want explicit registration or auto-discovery logic.
        // Here we just ensure stats entry exists.
        let dev_stats = self.device_stats.entry(mac_address).or_insert_with(StreamingStatistics::default);
        dev_stats.count_frame_received();

        // ESP-NOWフレームとしてパースを試みる
        // これによりシーケンス番号の抽出とチェックサム検証を行う
        match Frame::from_bytes(data) {
            Ok((frame, _size)) => {
                // パース成功
                let sequence = frame.sequence_number();
                // 統計更新のみにデータサイズを使用するため、コピーを避ける
                let frame_len = _size;
                
                // 統計更新
                dev_stats.count_frame_processed(frame_len);
                self.stats.frames_processed += 1;

                let processed_frame = ProcessedFrame {
                    sequence,
                    full_frame: data[.._size].to_vec(), // Use full frame bytes for USB forwarding
                    mac: mac_address,
                };
                
                Ok(vec![processed_frame])
            },
            Err(e) => {
                // パース失敗（チェックサムエラー、フォーマットエラーなど）
                self.stats.frames_error += 1;
                
                // エラータイプに応じて詳細なカウンタを更新
                match e {
                    FrameParseError::InvalidChecksum { .. } => self.stats.checksum_error_count += 1,
                    _ => {}, // その他のエラーも frames_error には含まれる
                }

                // エラーが発生してもストリームを停止させないため、空のベクタを返す
                // これにより上位のループは継続できる
                Ok(vec![])
            }
        }
    }

    pub fn cleanup_all_buffers(&mut self) -> usize {
        // 現在はバッファの自動クリーンアップは未実装
        // 将来的にはタイムアウトしたバッファを削除するロジックを実装予定
        0 
    }

    pub fn cleanup_inactive_devices(&mut self) -> usize {
        // 現在は非アクティブデバイスの自動削除は未実装
        0
    }

    pub fn device_count(&self) -> usize {
        self.device_stats.len()
    }

    /// Returns total buffer usage as (used_bytes, capacity_bytes) if available.
    ///
    /// Currently, `DeviceManager` does not track any concrete buffers, so this
    /// returns `None` to avoid reporting misleading utilization figures.
    pub fn total_buffer_usage(&self) -> Option<(usize, usize)> {
        None
    }

    pub fn global_statistics(&self) -> &GlobalStatistics {
        &self.stats
    }

    pub fn register_device(&mut self, mac_address: [u8; 6], name: String) -> StreamingResult<()> {
        self.devices.insert(mac_address, name);
        Ok(())
    }

    pub fn unregister_device(&mut self, mac_address: &[u8; 6]) -> StreamingResult<()> {
        self.devices.remove(mac_address);
        self.device_stats.remove(mac_address);
        Ok(())
    }

    pub fn get_devices(&self) -> Vec<([u8; 6], String, StreamStatus)> {
        self.devices.iter().map(|(mac, name)| {
            (*mac, name.clone(), StreamStatus::Active)
        }).collect()
    }

    pub fn get_device_statistics(&self, mac_address: &[u8; 6]) -> StreamingResult<&StreamingStatistics> {
         self.device_stats.get(mac_address).ok_or(StreamingError::InvalidData)
    }

    pub fn reset_device_stream(&mut self, _mac_address: &[u8; 6]) -> StreamingResult<()> {
        Ok(())
    }

    pub fn reset_statistics(&mut self) {
        self.stats = GlobalStatistics::default();
        for stats in self.device_stats.values_mut() {
            *stats = StreamingStatistics::default();
        }
    }
}
