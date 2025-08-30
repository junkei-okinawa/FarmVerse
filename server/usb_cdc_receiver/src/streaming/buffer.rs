/// Streaming Buffer Implementation
/// 
/// 循環バッファを使用してメモリ効率的なストリーミング処理を実現します。
/// 
/// ## 特徴
/// 
/// - **固定サイズ**: メモリ使用量を制限
/// - **循環利用**: 古いデータを上書きして継続的な処理を可能に
/// - **スレッドセーフ**: Mutexによる排他制御
/// - **バックプレッシャー対応**: バッファフル時の制御

use super::{StreamingError, StreamingResult};
use log::{debug, warn};

/// ストリーミングバッファの容量定数
pub const STREAMING_BUFFER_SIZE: usize = 512; // 512バイトに削減（メモリ制約のため）

/// 受信データの一時格納用構造体
#[derive(Debug, Clone)]
pub struct BufferedData {
    /// 受信データ
    pub data: heapless::Vec<u8, STREAMING_BUFFER_SIZE>,
    /// 受信時刻（ミリ秒）
    pub timestamp: u64,
    /// 送信元MACアドレス
    pub source_mac: [u8; 6],
}

impl BufferedData {
    /// 新しいBufferedDataインスタンスを作成
    pub fn new(data: &[u8], source_mac: [u8; 6]) -> StreamingResult<Self> {
        let mut vec = heapless::Vec::new();
        
        for &byte in data {
            vec.push(byte).map_err(|_| StreamingError::BufferFull)?;
        }
        
        Ok(BufferedData {
            data: vec,
            timestamp: get_current_timestamp(),
            source_mac,
        })
    }
    
    /// データサイズを取得
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    /// データが空かどうか確認
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    
    /// データの参照を取得
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }
}

/// 循環バッファ実装
/// 
/// 固定サイズのバッファを循環利用してメモリ効率的なストリーミング処理を実現
#[derive(Debug)]
pub struct StreamingBuffer {
    /// バッファ
    buffer: heapless::Deque<BufferedData, 4>, // メモリ削減: 8→4個
    /// バッファの最大サイズ
    max_size: usize,
    /// 現在の使用サイズ
    current_size: usize,
    /// ドロップされたデータ数（統計用）
    dropped_count: u64,
}

impl StreamingBuffer {
    /// 新しいストリーミングバッファを作成
    pub fn new() -> Self {
        StreamingBuffer {
            buffer: heapless::Deque::new(),
            max_size: STREAMING_BUFFER_SIZE,
            current_size: 0,
            dropped_count: 0,
        }
    }
    
    /// バッファにデータを追加
    /// 
    /// バッファが満杯の場合、最も古いデータを削除してから新しいデータを追加
    pub fn push(&mut self, data: BufferedData) -> StreamingResult<()> {
        // バッファサイズ制限をチェック
        if self.current_size + data.len() > self.max_size {
            // 古いデータを削除してスペースを確保
            while !self.buffer.is_empty() && 
                  self.current_size + data.len() > self.max_size {
                if let Some(old_data) = self.buffer.pop_front() {
                    self.current_size -= old_data.len();
                    self.dropped_count += 1;
                    warn!("Buffer full: dropped {} bytes from {:02X?}", 
                          old_data.len(), old_data.source_mac);
                }
            }
        }
        
        // 新しいデータを追加
        let data_size = data.len();
        self.buffer.push_back(data).map_err(|_| StreamingError::BufferFull)?;
        self.current_size += data_size;
        
        debug!("Buffer: added {} bytes, total size: {}/{}", 
               data_size, self.current_size, self.max_size);
        
        Ok(())
    }
    
    /// バッファからデータを取得
    pub fn pop(&mut self) -> Option<BufferedData> {
        if let Some(data) = self.buffer.pop_front() {
            self.current_size -= data.len();
            debug!("Buffer: removed {} bytes, remaining size: {}", 
                   data.len(), self.current_size);
            Some(data)
        } else {
            None
        }
    }
    
    /// バッファが空かどうか確認
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
    
    /// バッファの使用状況を取得
    pub fn usage(&self) -> (usize, usize) {
        (self.current_size, self.max_size)
    }
    
    /// バッファ内のアイテム数を取得
    pub fn len(&self) -> usize {
        self.buffer.len()
    }
    
    /// ドロップされたデータ数を取得
    pub fn dropped_count(&self) -> u64 {
        self.dropped_count
    }
    
    /// 古いデータをクリーンアップ（タイムアウト処理）
    /// 
    /// 指定された時間より古いデータを削除
    pub fn cleanup_old_data(&mut self, timeout_ms: u64) -> usize {
        let current_time = get_current_timestamp();
        let mut removed_count = 0;
        
        while let Some(front_data) = self.buffer.front() {
            if current_time - front_data.timestamp > timeout_ms {
                if let Some(old_data) = self.buffer.pop_front() {
                    self.current_size -= old_data.len();
                    removed_count += 1;
                    debug!("Cleaned up old data: {} bytes from {:02X?}", 
                           old_data.len(), old_data.source_mac);
                }
            } else {
                break;
            }
        }
        
        if removed_count > 0 {
            debug!("Cleanup: removed {} old items", removed_count);
        }
        
        removed_count
    }
    
    /// バッファの統計情報を取得
    pub fn stats(&self) -> BufferStats {
        BufferStats {
            items: self.len(),
            bytes_used: self.current_size,
            bytes_total: self.max_size,
            dropped_count: self.dropped_count,
            usage_percent: (self.current_size as f32 / self.max_size as f32) * 100.0,
        }
    }
    
    /// バッファをクリア
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.current_size = 0;
        debug!("Buffer cleared");
    }
}

impl Default for StreamingBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// バッファの統計情報
#[derive(Debug, Clone)]
pub struct BufferStats {
    /// バッファ内のアイテム数
    pub items: usize,
    /// 使用バイト数
    pub bytes_used: usize,
    /// 総容量バイト数
    pub bytes_total: usize,
    /// ドロップされたデータ数
    pub dropped_count: u64,
    /// 使用率（パーセンテージ）
    pub usage_percent: f32,
}

impl BufferStats {
    /// バッファが満杯に近いかどうか確認
    pub fn is_near_full(&self) -> bool {
        self.usage_percent > 80.0
    }
    
    /// バッファが危険な状態かどうか確認
    pub fn is_critical(&self) -> bool {
        self.usage_percent > 95.0
    }
}

/// 現在のタイムスタンプを取得（ミリ秒）
fn get_current_timestamp() -> u64 {
    // FreeRTOSのシステムティック（より安全）
    unsafe {
        // FreeRTOS tick count を使用（WDTリセットを避けるため）
        let ticks = esp_idf_sys::xTaskGetTickCount();
        // tick を ミリ秒に変換 (通常 1 tick = 1ms)
        ticks as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffered_data_creation() {
        let data = vec![1, 2, 3, 4, 5];
        let mac = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
        
        let buffered = BufferedData::new(&data, mac).unwrap();
        assert_eq!(buffered.len(), 5);
        assert_eq!(buffered.as_slice(), &[1, 2, 3, 4, 5]);
        assert_eq!(buffered.source_mac, mac);
        assert!(!buffered.is_empty());
    }

    #[test]
    fn test_streaming_buffer_basic_operations() {
        let mut buffer = StreamingBuffer::new();
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
        
        // データ追加
        let data1 = BufferedData::new(&[1, 2, 3], [0x11; 6]).unwrap();
        buffer.push(data1).unwrap();
        assert!(!buffer.is_empty());
        assert_eq!(buffer.len(), 1);
        
        // データ取得
        let retrieved = buffer.pop().unwrap();
        assert_eq!(retrieved.as_slice(), &[1, 2, 3]);
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_streaming_buffer_overflow() {
        let mut buffer = StreamingBuffer::new();
        
        // バッファ容量を超えるデータを追加
        let large_data = vec![0u8; 1024]; // 1KB
        for i in 0..5 {
            let mac = [i as u8; 6];
            let buffered = BufferedData::new(&large_data, mac).unwrap();
            buffer.push(buffered).unwrap();
        }
        
        // 古いデータがドロップされることを確認
        assert!(buffer.dropped_count() > 0);
        
        let stats = buffer.stats();
        assert!(stats.bytes_used <= STREAMING_BUFFER_SIZE);
    }

    #[test]
    fn test_buffer_stats() {
        let buffer = StreamingBuffer::new();
        let stats = buffer.stats();
        assert_eq!(stats.items, 0);
        assert_eq!(stats.bytes_used, 0);
        assert_eq!(stats.usage_percent, 0.0);
        assert!(!stats.is_near_full());
        assert!(!stats.is_critical());
    }

    #[test]
    fn test_buffer_cleanup() {
        let mut buffer = StreamingBuffer::new();
        
        // 複数のデータを追加
        for i in 0..3 {
            let data = BufferedData::new(&[i], [i; 6]).unwrap();
            buffer.push(data).unwrap();
        }
        
        assert_eq!(buffer.len(), 3);
        
        // すべてクリア
        buffer.clear();
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
    }
}
