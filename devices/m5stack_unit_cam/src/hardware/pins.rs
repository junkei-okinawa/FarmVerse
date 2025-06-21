use esp_idf_svc::hal::gpio::*;

/// カメラピン設定構造体
pub struct CameraPins {
    pub clock: Gpio27,
    pub d0: Gpio32,
    pub d1: Gpio35,
    pub d2: Gpio34,
    pub d3: Gpio5,
    pub d4: Gpio39,
    pub d5: Gpio18,
    pub d6: Gpio36,
    pub d7: Gpio19,
    pub vsync: Gpio22,
    pub href: Gpio26,
    pub pclk: Gpio21,
    pub sda: Gpio25,
    pub scl: Gpio23,
}

impl CameraPins {
    /// 個別のピンから作成
    pub fn new(
        clock: Gpio27, d0: Gpio32, d1: Gpio35, d2: Gpio34,
        d3: Gpio5, d4: Gpio39, d5: Gpio18, d6: Gpio36,
        d7: Gpio19, vsync: Gpio22, href: Gpio26, pclk: Gpio21,
        sda: Gpio25, scl: Gpio23,
    ) -> Self {
        Self {
            clock, d0, d1, d2, d3, d4, d5, d6, d7,
            vsync, href, pclk, sda, scl,
        }
    }
}
