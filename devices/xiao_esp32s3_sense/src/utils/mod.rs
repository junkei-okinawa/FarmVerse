/// ユーティリティモジュール
/// ハードウェア非依存の純粋関数を提供

pub mod voltage_calc;

// 便利な再エクスポート
pub use voltage_calc::calculate_voltage_percentage;
