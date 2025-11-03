/// ユーティリティモジュール
/// ハードウェア非依存の純粋関数を提供

pub mod voltage_calc;
pub mod tds_calc;
pub mod streaming_protocol;

// 便利な再エクスポート
pub use voltage_calc::calculate_voltage_percentage;
pub use tds_calc::{calculate_tds_from_ec, compensate_ec_temperature, calculate_ec_from_adc};
pub use streaming_protocol::{MessageType, StreamingHeader, StreamingMessage, DeserializeError};
