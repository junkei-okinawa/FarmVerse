/// Frame Processor for Streaming Architecture
/// 
/// 送信側からのストリーミングフレームを解析し、チェックサム検証、
/// シーケンス管理、エラーハンドリングを行います。
/// 
/// ## プロトコル仕様
/// 
/// 送信側のstreaming.rsと互換性のあるフレーム形式:
/// ```
/// [START_MARKER:4][SEQUENCE:2][DATA_LEN:2][CHECKSUM:4][DATA:N][END_MARKER:4]
/// ```

use super::{StreamingError, StreamingResult};
use log::{debug, warn};

/// フレームマーカー定数（送信側と同期）
pub const START_MARKER: [u8; 4] = [0xAA, 0xBB, 0xCC, 0xDD];
pub const END_MARKER: [u8; 4] = [0xDD, 0xCC, 0xBB, 0xAA];

/// フレームヘッダの最小サイズ
pub const FRAME_HEADER_SIZE: usize = 4 + 2 + 2 + 4; // START + SEQ + LEN + CHECKSUM
pub const FRAME_FOOTER_SIZE: usize = 4; // END_MARKER
pub const MIN_FRAME_SIZE: usize = FRAME_HEADER_SIZE + FRAME_FOOTER_SIZE;

/// フレームヘッダ情報
#[derive(Debug, Clone, PartialEq)]
pub struct FrameHeader {
    /// シーケンス番号
    pub sequence: u16,
    /// データ長
    pub data_len: u16,
    /// チェックサム
    pub checksum: u32,
}

impl FrameHeader {
    /// フレームヘッダを作成
    pub fn new(sequence: u16, data_len: u16, checksum: u32) -> Self {
        FrameHeader {
            sequence,
            data_len,
            checksum,
        }
    }
    
    /// バイト配列からフレームヘッダを解析
    pub fn from_bytes(data: &[u8]) -> StreamingResult<Self> {
        if data.len() < FRAME_HEADER_SIZE {
            return Err(StreamingError::InvalidFrame(
                format!("Header too short: {} bytes", data.len())
            ));
        }
        
        // START_MARKERの確認
        if &data[0..4] != START_MARKER {
            return Err(StreamingError::InvalidFrame(
                "Invalid start marker".to_string()
            ));
        }
        
        // ヘッダフィールドの抽出
        let sequence = u16::from_le_bytes([data[4], data[5]]);
        let data_len = u16::from_le_bytes([data[6], data[7]]);
        let checksum = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        
        Ok(FrameHeader {
            sequence,
            data_len,
            checksum,
        })
    }
}

/// フレーム処理結果
#[derive(Debug, Clone)]
pub enum ProcessingResult {
    /// 完全なフレームが処理された
    FrameProcessed {
        header: FrameHeader,
        payload: Vec<u8>,
        total_bytes: usize,
    },
    /// フレームが不完全（更多データが必要）
    IncompleteFrame {
        needed_bytes: usize,
    },
    /// フレームエラーが発生
    FrameError {
        error: StreamingError,
        consumed_bytes: usize,
    },
}

impl ProcessingResult {
    /// 処理が成功したかどうか確認
    pub fn is_success(&self) -> bool {
        matches!(self, ProcessingResult::FrameProcessed { .. })
    }
    
    /// 消費されたバイト数を取得
    pub fn consumed_bytes(&self) -> usize {
        match self {
            ProcessingResult::FrameProcessed { total_bytes, .. } => *total_bytes,
            ProcessingResult::IncompleteFrame { .. } => 0,
            ProcessingResult::FrameError { consumed_bytes, .. } => *consumed_bytes,
        }
    }
}

/// ストリーミングフレームプロセッサ
/// 
/// 受信したデータストリームからフレームを抽出し、検証します。
#[derive(Debug)]
pub struct FrameProcessor {
    /// 現在のバッファ
    buffer: Vec<u8>,
    /// 期待するシーケンス番号
    expected_sequence: u16,
    /// 処理統計
    frames_processed: u64,
    frames_error: u64,
    last_sequence_error: Option<u16>,
}

impl FrameProcessor {
    /// 新しいフレームプロセッサを作成
    pub fn new() -> Self {
        FrameProcessor {
            buffer: Vec::with_capacity(256), // メモリ使用量削減: 1024→256
            expected_sequence: 0,
            frames_processed: 0,
            frames_error: 0,
            last_sequence_error: None,
        }
    }
    
    /// バッファにデータを追加して処理
    pub fn process_data(&mut self, data: &[u8]) -> Vec<ProcessingResult> {
        self.buffer.extend_from_slice(data);
        debug!("FrameProcessor: added {} bytes, buffer size: {}", 
               data.len(), self.buffer.len());
        
        let mut results = Vec::new();
        
        // バッファから可能な限りフレームを抽出
        while !self.buffer.is_empty() {
            match self.try_extract_frame() {
                Some(result) => {
                    let consumed = result.consumed_bytes();
                    
                    // 成功した場合は統計を更新
                    if result.is_success() {
                        self.frames_processed += 1;
                        if let ProcessingResult::FrameProcessed { header, .. } = &result {
                            // シーケンス番号の検証と更新
                            if let Err(seq_error) = self.validate_sequence(header.sequence) {
                                // シーケンスエラーを記録するが、フレーム自体は有効として処理
                                warn!("Sequence validation failed: {}", seq_error);
                                self.frames_error += 1;
                                self.last_sequence_error = Some(header.sequence);
                            }
                            self.expected_sequence = header.sequence.wrapping_add(1);
                        }
                    } else {
                        self.frames_error += 1;
                    }
                    
                    // 消費されたバイトを削除
                    if consumed > 0 {
                        self.buffer.drain(0..consumed);
                    }
                    
                    results.push(result);
                }
                None => break, // これ以上フレームを抽出できない
            }
        }
        
        // バッファサイズ制限（メモリ保護）
        const MAX_BUFFER_SIZE: usize = 32768; // 32KB
        if self.buffer.len() > MAX_BUFFER_SIZE {
            warn!("Buffer size exceeded limit, clearing: {} bytes", self.buffer.len());
            self.buffer.clear();
            results.push(ProcessingResult::FrameError {
                error: StreamingError::Other("Buffer overflow".to_string()),
                consumed_bytes: 0,
            });
        }
        
        results
    }
    
    /// 1つのフレームを抽出試行
    fn try_extract_frame(&mut self) -> Option<ProcessingResult> {
        // 最小フレームサイズのチェック
        if self.buffer.len() < MIN_FRAME_SIZE {
            return Some(ProcessingResult::IncompleteFrame {
                needed_bytes: MIN_FRAME_SIZE - self.buffer.len(),
            });
        }
        
        // START_MARKERを検索
        let start_pos = self.find_start_marker()?;
        
        // START_MARKERより前のデータがあれば破棄
        if start_pos > 0 {
            warn!("Discarding {} bytes before start marker", start_pos);
            return Some(ProcessingResult::FrameError {
                error: StreamingError::InvalidFrame("No start marker found".to_string()),
                consumed_bytes: start_pos,
            });
        }
        
        // フレームヘッダの解析
        let header = match FrameHeader::from_bytes(&self.buffer[start_pos..]) {
            Ok(h) => h,
            Err(e) => {
                return Some(ProcessingResult::FrameError {
                    error: e,
                    consumed_bytes: 1, // 1バイト進めて再試行
                });
            }
        };
        
        // 完全なフレームサイズの計算
        let total_frame_size = FRAME_HEADER_SIZE + header.data_len as usize + FRAME_FOOTER_SIZE;
        
        // データが十分にあるかチェック
        if self.buffer.len() < start_pos + total_frame_size {
            return Some(ProcessingResult::IncompleteFrame {
                needed_bytes: (start_pos + total_frame_size) - self.buffer.len(),
            });
        }
        
        // フレームデータの抽出
        let frame_start = start_pos + FRAME_HEADER_SIZE;
        let frame_end = frame_start + header.data_len as usize;
        let payload = self.buffer[frame_start..frame_end].to_vec();
        
        // END_MARKERの確認
        let end_marker_start = frame_end;
        if &self.buffer[end_marker_start..end_marker_start + 4] != END_MARKER {
            return Some(ProcessingResult::FrameError {
                error: StreamingError::InvalidFrame("Invalid end marker".to_string()),
                consumed_bytes: start_pos + 1,
            });
        }
        
        // チェックサムの検証
        if let Err(checksum_error) = self.verify_checksum(&payload, header.checksum) {
            return Some(ProcessingResult::FrameError {
                error: checksum_error,
                consumed_bytes: start_pos + total_frame_size,
            });
        }
        
        debug!("FrameProcessor: extracted frame - seq: {}, len: {}, checksum: 0x{:08X}",
               header.sequence, header.data_len, header.checksum);
        
        Some(ProcessingResult::FrameProcessed {
            header,
            payload,
            total_bytes: start_pos + total_frame_size,
        })
    }
    
    /// START_MARKERの位置を検索
    fn find_start_marker(&self) -> Option<usize> {
        for i in 0..=(self.buffer.len().saturating_sub(4)) {
            if &self.buffer[i..i + 4] == START_MARKER {
                return Some(i);
            }
        }
        None
    }
    
    /// チェックサムを検証
    fn verify_checksum(&self, data: &[u8], expected: u32) -> StreamingResult<()> {
        let calculated = calculate_checksum(data);
        if calculated == expected {
            Ok(())
        } else {
            Err(StreamingError::ChecksumMismatch {
                expected,
                actual: calculated,
            })
        }
    }
    
    /// シーケンス番号を検証
    fn validate_sequence(&self, sequence: u16) -> StreamingResult<()> {
        if sequence == self.expected_sequence {
            Ok(())
        } else {
            Err(StreamingError::SequenceError {
                expected: self.expected_sequence,
                actual: sequence,
            })
        }
    }
    
    /// 期待するシーケンス番号をリセット
    pub fn reset_sequence(&mut self, sequence: u16) {
        self.expected_sequence = sequence;
        debug!("FrameProcessor: reset expected sequence to {}", sequence);
    }
    
    /// 処理統計を取得
    pub fn stats(&self) -> (u64, u64) {
        (self.frames_processed, self.frames_error)
    }
    
    /// バッファをクリア
    pub fn clear_buffer(&mut self) {
        self.buffer.clear();
        debug!("FrameProcessor: buffer cleared");
    }
    
    /// 現在のバッファサイズを取得
    pub fn buffer_size(&self) -> usize {
        self.buffer.len()
    }
}

impl Default for FrameProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// CRC32チェックサムを計算（送信側と同じアルゴリズム）
pub fn calculate_checksum(data: &[u8]) -> u32 {
    // 簡単なCRC32実装（送信側と同期）
    let mut crc = 0xFFFFFFFFu32;
    
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB88320;
            } else {
                crc >>= 1;
            }
        }
    }
    
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_header_parsing() {
        let mut data = Vec::new();
        data.extend_from_slice(&START_MARKER);
        data.extend_from_slice(&1234u16.to_le_bytes()); // sequence
        data.extend_from_slice(&56u16.to_le_bytes());   // data_len
        data.extend_from_slice(&0x12345678u32.to_le_bytes()); // checksum
        
        let header = FrameHeader::from_bytes(&data).unwrap();
        assert_eq!(header.sequence, 1234);
        assert_eq!(header.data_len, 56);
        assert_eq!(header.checksum, 0x12345678);
    }

    #[test]
    fn test_frame_header_invalid_marker() {
        let data = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B];
        let result = FrameHeader::from_bytes(&data);
        assert!(result.is_err());
        match result.unwrap_err() {
            StreamingError::InvalidFrame(msg) => assert!(msg.contains("Invalid start marker")),
            _ => panic!("Expected InvalidFrame error"),
        }
    }

    #[test]
    fn test_checksum_calculation() {
        let data = b"Hello, World!";
        let checksum1 = calculate_checksum(data);
        let checksum2 = calculate_checksum(data);
        assert_eq!(checksum1, checksum2); // 再現性の確認
        
        // 異なるデータでは異なるチェックサム
        let checksum3 = calculate_checksum(b"Different data");
        assert_ne!(checksum1, checksum3);
    }

    #[test]
    fn test_frame_processor_basic() {
        let mut processor = FrameProcessor::new();
        
        // 完全なフレームを作成
        let payload = b"test data";
        let checksum = calculate_checksum(payload);
        
        let mut frame = Vec::new();
        frame.extend_from_slice(&START_MARKER);
        frame.extend_from_slice(&0u16.to_le_bytes()); // sequence
        frame.extend_from_slice(&(payload.len() as u16).to_le_bytes()); // data_len
        frame.extend_from_slice(&checksum.to_le_bytes());
        frame.extend_from_slice(payload);
        frame.extend_from_slice(&END_MARKER);
        
        let results = processor.process_data(&frame);
        assert_eq!(results.len(), 1);
        
        match &results[0] {
            ProcessingResult::FrameProcessed { header, payload: p, .. } => {
                assert_eq!(header.sequence, 0);
                assert_eq!(p, payload);
            }
            _ => panic!("Expected FrameProcessed result"),
        }
    }

    #[test]
    fn test_frame_processor_incomplete() {
        let mut processor = FrameProcessor::new();
        
        // 不完全なフレーム（ヘッダの一部のみ）
        let partial_data = vec![0xAA, 0xBB, 0xCC]; // START_MARKERの一部
        
        let results = processor.process_data(&partial_data);
        assert_eq!(results.len(), 1);
        
        match &results[0] {
            ProcessingResult::IncompleteFrame { .. } => {
                // 期待された結果
            }
            _ => panic!("Expected IncompleteFrame result"),
        }
    }

    #[test]
    fn test_frame_processor_checksum_error() {
        let mut processor = FrameProcessor::new();
        
        let payload = b"test data";
        let wrong_checksum = 0x12345678u32; // 故意に間違ったチェックサム
        
        let mut frame = Vec::new();
        frame.extend_from_slice(&START_MARKER);
        frame.extend_from_slice(&0u16.to_le_bytes());
        frame.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        frame.extend_from_slice(&wrong_checksum.to_le_bytes());
        frame.extend_from_slice(payload);
        frame.extend_from_slice(&END_MARKER);
        
        let results = processor.process_data(&frame);
        assert_eq!(results.len(), 1);
        
        match &results[0] {
            ProcessingResult::FrameError { error, .. } => {
                match error {
                    StreamingError::ChecksumMismatch { .. } => {
                        // 期待された結果
                    }
                    _ => panic!("Expected ChecksumMismatch error"),
                }
            }
            _ => panic!("Expected FrameError result"),
        }
    }
}
