/// 画像データのフレーム処理に関するエラー
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("データが空です")]
    EmptyData,
}
