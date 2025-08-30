use esp_idf_svc::hal::gpio::*;

/// カメラピン設定構造体
pub struct CameraPins {
    pub clock: Gpio10,
    pub d0: Gpio15,
    pub d1: Gpio17,
    pub d2: Gpio18,
    pub d3: Gpio16,
    pub d4: Gpio14,
    pub d5: Gpio12,
    pub d6: Gpio11,
    pub d7: Gpio48,
    pub vsync: Gpio38,
    pub href: Gpio47,
    pub pclk: Gpio13,
    pub sda: Gpio40,
    pub scl: Gpio39,
}

impl CameraPins {
    /// 個別のピンから作成
    pub fn new(
        clock: Gpio10,
        d0: Gpio15,
        d1: Gpio17,
        d2: Gpio18,
        d3: Gpio16,
        d4: Gpio14,
        d5: Gpio12,
        d6: Gpio11,
        d7: Gpio48,
        vsync: Gpio38,
        href: Gpio47,
        pclk: Gpio13,
        sda: Gpio40,
        scl: Gpio39,
    ) -> Self {
        Self {
            clock, d0, d1, d2, d3, d4, d5, d6, d7,
            vsync, href, pclk, sda, scl,
        }
    }
}
