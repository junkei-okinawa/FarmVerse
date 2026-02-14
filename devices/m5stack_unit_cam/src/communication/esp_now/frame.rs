use sha2::{Digest, Sha256};

/// 画像データのフレーム処理に関するエラー
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("データが空です")]
    EmptyData,
}

/// 画像フレームを処理するためのユーティリティ
pub struct ImageFrame;

impl ImageFrame {
    /// 画像データのSHA256ハッシュを計算します
    ///
    /// # 引数
    ///
    /// * `data` - ハッシュを計算する画像データ
    ///
    /// # 戻り値
    ///
    /// 16進数形式のハッシュ文字列
    ///
    /// # エラー
    ///
    /// データが空の場合にエラーを返します
    pub fn calculate_hash(data: &[u8]) -> Result<String, FrameError> {
        if data.is_empty() {
            return Err(FrameError::EmptyData);
        }

        let mut hasher = Sha256::new();
        hasher.update(data);
        let hash_result = hasher.finalize();
        let hash_hex = format!("{:x}", hash_result);

        Ok(hash_hex)
    }

    /// 画像データからフレームを作成
    pub fn from_image_data(data: Vec<u8>) -> Result<Self, FrameError> {
        if data.is_empty() {
            return Err(FrameError::EmptyData);
        }
        Ok(ImageFrame)
    }

    /// ダミーフレームを作成
    pub fn dummy(_hash: &str) -> Self {
        ImageFrame
    }
}
