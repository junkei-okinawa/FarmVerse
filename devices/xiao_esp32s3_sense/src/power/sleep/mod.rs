use log::info;
pub mod deep_sleep;
pub mod light_sleep;

pub use deep_sleep::*;
pub use light_sleep::*;

/// スリープの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepType {
    /// ディープスリープ (再起動が必要、待機電力最小)
    Deep,
    /// ライトスリープ (RAMと周辺機器の状態を保持、復帰が高速)
    Light,
}

/// ハイブリッド制御を含むスリープマネージャー
pub struct SleepManager<D: DeepSleepPlatform, L: LightSleepPlatform> {
    deep_platform: D,
    light_platform: L,
    /// Light Sleepを選択する最大秒数 (これを超えるとDeep Sleep)
    light_sleep_threshold_sec: u64,
}

impl<D: DeepSleepPlatform, L: LightSleepPlatform> SleepManager<D, L> {
    pub fn new(deep_platform: D, light_platform: L, threshold: u64) -> Self {
        Self {
            deep_platform,
            light_platform,
            light_sleep_threshold_sec: threshold,
        }
    }

    /// 指定された秒数に最適なスリープモードを選択して実行します
    pub fn sleep_optimized(&self, duration_sec: u64) -> anyhow::Result<SleepType> {
        if duration_sec < self.light_sleep_threshold_sec {
            info!("選択されたスリープモード: LIGHT SLEEP ({}秒 < {}秒閾値)", duration_sec, self.light_sleep_threshold_sec);
            self.light_platform.light_sleep(duration_sec * 1_000_000);
            Ok(SleepType::Light)
        } else {
            info!("選択されたスリープモード: DEEP SLEEP ({}秒 >= {}秒閾値)", duration_sec, self.light_sleep_threshold_sec);
            self.deep_platform.deep_sleep(duration_sec * 1_000_000);
            // 通常ここには戻らない
            Ok(SleepType::Deep)
        }
    }
}
