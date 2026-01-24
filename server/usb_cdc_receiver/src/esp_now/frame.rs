#[cfg(target_os = "espidf")]
use log::debug;
#[cfg(not(target_os = "espidf"))]
macro_rules! debug {
    ($($arg:tt)*) => {};
}

use super::FrameType;

/// フレーム処理のための定数
pub const START_MARKER: u32 = 0xFACE_AABB; // フレーム開始マーカー
pub const END_MARKER: u32 = 0xCDEF_5678;   // フレーム終了マーカー

/// ESP-NOWフレームの構造
/// 
/// フレーム構造:
/// - 開始マーカー (4バイト): 0xFACE_AABB
/// - MACアドレス (6バイト): 送信元デバイスのMACアドレス
/// - フレームタイプ (1バイト): 1=HASH, 2=DATA, 3=EOF
/// - シーケンス番号 (4バイト): データの順序を保証するためのカウンター
/// - データ長 (4バイト): ペイロードの長さ
/// - データ本体 (可変長): 実際のペイロードデータ
/// - チェックサム (4バイト): データの整合性を検証するためのXORチェックサム
/// - 終了マーカー (4バイト): 0xCDEF_5678
pub struct Frame {
    mac_address: [u8; 6],
    frame_type: FrameType,
    sequence_number: u32,
    data: Vec<u8>,
}

/// フレーム解析エラー
#[derive(Debug, PartialEq)]
pub enum FrameParseError {
    TooShort,
    InvalidStartMarker(u32),
    InvalidEndMarker(u32),
    InvalidFrameType(u8),
    InvalidChecksum { expected: u32, actual: u32 },
    DataLengthExceedsBuffer { offset: usize, data_len: usize, buffer_len: usize },
}

impl Frame {
    /// 新しいフレームを作成します
    pub fn new(
        mac_address: [u8; 6],
        frame_type: FrameType,
        sequence_number: u32,
        data: Vec<u8>,
    ) -> Self {
        Self {
            mac_address,
            frame_type,
            sequence_number,
            data,
        }
    }

    /// フレームを生のバイトに変換します
    /// 
    /// フォーマット（xiao_esp32s3_sense送信側と互換）:
    /// - START_MARKER: 4 bytes (big-endian: 0xFACEAABB)
    /// - MAC: 6 bytes
    /// - FRAME_TYPE: 1 byte
    /// - SEQUENCE: 4 bytes (little-endian)
    /// - DATA_LEN: 4 bytes (little-endian)
    /// - DATA: variable length
    /// - CHECKSUM: 4 bytes (little-endian)
    /// - END_MARKER: 4 bytes (big-endian: 0xCDEF5678)
    pub fn to_bytes(&self) -> Vec<u8> {
        let start_marker_bytes = START_MARKER.to_be_bytes();
        let end_marker_bytes = END_MARKER.to_be_bytes();
        let data_len_bytes = (self.data.len() as u32).to_le_bytes(); // little-endian
        let seq_bytes = self.sequence_number.to_le_bytes(); // little-endian
        let checksum_bytes = calculate_checksum(&self.data).to_le_bytes(); // little-endian

        // フレームの合計長を計算
        let total_frame_len = start_marker_bytes.len() + // 開始マーカー: 4バイト
            self.mac_address.len() +      // MACアドレス: 6バイト
            1 +                    // フレームタイプ: 1バイト
            seq_bytes.len() +      // シーケンス番号: 4バイト
            data_len_bytes.len() + // データ長: 4バイト
            self.data.len() +     // 実データ: 可変長
            checksum_bytes.len() + // チェックサム: 4バイト
            end_marker_bytes.len(); // 終了マーカー: 4バイト

        let mut framed_data = Vec::with_capacity(total_frame_len);

        // フレームを構築
        framed_data.extend_from_slice(&start_marker_bytes); // 開始マーカー (big-endian)
        framed_data.extend_from_slice(&self.mac_address); // MACアドレス
        framed_data.push(self.frame_type.to_byte()); // フレームタイプ
        framed_data.extend_from_slice(&seq_bytes); // シーケンス番号 (little-endian)
        framed_data.extend_from_slice(&data_len_bytes); // データ長 (little-endian)
        framed_data.extend_from_slice(&self.data); // データ本体
        framed_data.extend_from_slice(&checksum_bytes); // チェックサム (little-endian)
        framed_data.extend_from_slice(&end_marker_bytes); // 終了マーカー (big-endian)

        framed_data
    }

    /// バイトデータからフレームを解析します
    /// 
    /// # 戻り値
    /// * `Result<(Self, usize), FrameParseError>` - 解析に成功した場合はフレームと使用したバイト数のタプル、失敗した場合はエラー
    pub fn from_bytes(data: &[u8]) -> Result<(Self, usize), FrameParseError> {
        // 最小フレームサイズをチェック
        const MIN_FRAME_SIZE: usize = 4 + 6 + 1 + 4 + 4 + 0 + 4 + 4; // 27バイト
        if data.len() < MIN_FRAME_SIZE {
            debug!("Frame data too short: {} bytes", data.len());
            return Err(FrameParseError::TooShort);
        }

        // 開始マーカーのチェック
        let mut offset = 0;
        let start_marker = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        if start_marker != START_MARKER {
            debug!("Invalid start marker: {:08x}", start_marker);
            return Err(FrameParseError::InvalidStartMarker(start_marker));
        }
        offset += 4;

        // MACアドレスの抽出
        let mut mac_address = [0u8; 6];
        mac_address.copy_from_slice(&data[offset..offset + 6]);
        offset += 6;

        // フレームタイプの解析
        let frame_type_byte = data[offset];
        let frame_type = match FrameType::from_byte(frame_type_byte) {
            Some(frame_type) => frame_type,
            None => {
                debug!("Invalid frame type: {}", frame_type_byte);
                return Err(FrameParseError::InvalidFrameType(frame_type_byte));
            }
        };
        offset += 1;

        // シーケンス番号の解析 (little-endian)
        let sequence_number = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        offset += 4;

        // データ長の解析 (little-endian)
        let data_len = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        // データ長のバリデーション
        if offset + data_len + 8 > data.len() {
            debug!(
                "Data length exceeds buffer: offset={}, data_len={}, buffer={}",
                offset,
                data_len,
                data.len()
            );
            return Err(FrameParseError::DataLengthExceedsBuffer {
                offset,
                data_len,
                buffer_len: data.len(),
            });
        }

        // データの抽出
        let payload_data = data[offset..offset + data_len].to_vec();
        offset += data_len;

        // チェックサムの検証 (little-endian)
        let expected_checksum = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        let actual_checksum = calculate_checksum(&payload_data);
        if expected_checksum != actual_checksum {
            debug!(
                "Checksum mismatch: expected={:08x}, actual={:08x}",
                expected_checksum, actual_checksum
            );
            return Err(FrameParseError::InvalidChecksum {
                expected: expected_checksum,
                actual: actual_checksum,
            });
        }
        offset += 4;

        // 終了マーカーの検証
        let end_marker = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]);
        if end_marker != END_MARKER {
            debug!("Invalid end marker: {:08x}", end_marker);
            return Err(FrameParseError::InvalidEndMarker(end_marker));
        }
        offset += 4;

        // フレームオブジェクトの作成
        let frame = Frame {
            mac_address,
            frame_type,
            sequence_number,
            data: payload_data,
        };

        Ok((frame, offset))
    }

    /// MACアドレスを取得
    pub fn mac_address(&self) -> &[u8; 6] {
        &self.mac_address
    }

    /// フレームタイプを取得
    pub fn frame_type(&self) -> FrameType {
        self.frame_type
    }

    /// シーケンス番号を取得
    pub fn sequence_number(&self) -> u32 {
        self.sequence_number
    }

    /// データを取得
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

/// データのチェックサムを計算します（XORベース）
/// 
/// # 引数
/// * `data` - チェックサムを計算するデータ
/// 
/// # 戻り値
/// * `u32` - 計算されたチェックサム値
pub fn calculate_checksum(data: &[u8]) -> u32 {
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

/// データのフレーム化を行います
/// 
/// # 引数
/// * `mac_address` - 送信元MACアドレス
/// * `data` - フレーム化するデータ
/// * `frame_type` - フレームのタイプ
/// * `sequence_number` - シーケンス番号
/// 
/// # 戻り値
/// * `Vec<u8>` - フレーム化されたデータ
pub fn create_frame(
    mac_address: [u8; 6],
    data: &[u8],
    frame_type: FrameType,
    sequence_number: u32,
) -> Vec<u8> {
    let frame = Frame::new(mac_address, frame_type, sequence_number, data.to_vec());
    frame.to_bytes()
}

/// データの特定のパターンに基づいてフレームタイプを判断します
pub fn detect_frame_type(data: &[u8]) -> FrameType {
    // EOF判定: "EOF!"の場合
    if data.len() == 4 && data == b"EOF!" {
        return FrameType::Eof;
    }
    
    // HASH判定: "HASH:"で始まる場合
    if data.len() > 5 && data.starts_with(b"HASH:") {
        return FrameType::Hash;
    }
    
    // それ以外はデータフレーム
    FrameType::Data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_checksum() {
        // 単純なケース
        assert_eq!(calculate_checksum(&[1, 0, 0, 0]), 1);
        assert_eq!(calculate_checksum(&[1, 2, 3, 4]), 0x04030201);
        
        // XORの性質をテスト
        assert_eq!(
            calculate_checksum(&[1, 2, 3, 4, 1, 2, 3, 4]),
            0x04030201 ^ 0x04030201
        );
    }

    #[test]
    fn test_frame_roundtrip() {
        let mac = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
        let data = vec![1, 2, 3, 4, 5];
        let seq = 42;
        
        // フレーム作成
        let frame = Frame::new(mac, FrameType::Data, seq, data.clone());
        let bytes = frame.to_bytes();
        
        // フレーム解析
        let (parsed_frame, size) = Frame::from_bytes(&bytes).unwrap();
        
        // 検証
        assert_eq!(size, bytes.len());
        assert_eq!(parsed_frame.mac_address, mac);
        assert_eq!(parsed_frame.frame_type, FrameType::Data);
        assert_eq!(parsed_frame.sequence_number, seq);
        assert_eq!(parsed_frame.data, data);
    }

    #[test]
    fn test_detect_frame_type() {
        assert_eq!(detect_frame_type(b"EOF!"), FrameType::Eof);
        assert_eq!(detect_frame_type(b"HASH:12345"), FrameType::Hash);
        assert_eq!(detect_frame_type(b"normal data"), FrameType::Data);
    }
}
