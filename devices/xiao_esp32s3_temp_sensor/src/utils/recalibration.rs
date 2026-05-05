/// deep sleep サイクルカウンタと再キャリブレーション周期から、今サイクルで PHY 再キャリブレーションが必要か判定する
///
/// - interval = 0: 常に false (無効化)
/// - interval != 0: cycle % interval == 0 のとき true
pub fn needs_recalibration(cycle: u32, interval: u32) -> bool {
    interval != 0 && cycle % interval == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recal_first_cycle_triggers() {
        // cycle=0 は初回起動。0 % N == 0 なので常に true
        assert!(needs_recalibration(0, 100));
    }

    #[test]
    fn test_recal_mid_cycle_does_not_trigger() {
        assert!(!needs_recalibration(50, 100));
        assert!(!needs_recalibration(99, 100));
    }

    #[test]
    fn test_recal_exact_interval_triggers() {
        assert!(needs_recalibration(100, 100));
        assert!(needs_recalibration(200, 100));
    }

    #[test]
    fn test_recal_interval_zero_always_false() {
        // interval=0 は無効化; いかなる cycle でも false
        assert!(!needs_recalibration(0, 0));
        assert!(!needs_recalibration(100, 0));
        assert!(!needs_recalibration(u32::MAX, 0));
    }

    #[test]
    fn test_recal_wrapping_max_u32() {
        // u32::MAX % 100 == 95 → false
        assert!(!needs_recalibration(u32::MAX, 100));
    }

    #[test]
    fn test_recal_interval_one_always_true() {
        // interval=1 は毎サイクル再キャリブレーション
        assert!(needs_recalibration(0, 1));
        assert!(needs_recalibration(1, 1));
        assert!(needs_recalibration(999, 1));
    }
}
