/// ユーティリティモジュール
/// ハードウェア非依存の純粋関数を提供

pub mod mac_utils;
pub mod payload;
pub mod recalibration;

pub use mac_utils::parse_mac;
pub use payload::{format_hash_payload, EOF_MARKER};
pub use recalibration::needs_recalibration;
