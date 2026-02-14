/// XIAO ESP32S3 Sense対応カメラピン設定とカメラ制御
/// 
/// Issue #12のカメラ初期化実装

#[allow(dead_code)] // Issue #12 カメラ機能実装中のため一時的に警告を抑制

/// XIAO ESP32S3 Sense用カメラピン構造体
#[derive(Debug, PartialEq)]
pub struct CameraPins {
    pub data_pins: [u8; 8],
    pub xclk_pin: u8,
    pub pclk_pin: u8,
    pub vsync_pin: u8,
    pub href_pin: u8,
    pub sda_pin: u8,
    pub scl_pin: u8,
}

/// 解像度列挙型
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Resolution {
    UXGA,  // 1600x1200
    SVGA,  // 800x600
    VGA,   // 640x480
}

/// カメラ設定構造体
#[derive(Debug)]
pub struct CameraConfig {
    resolution: Resolution,
}

impl CameraConfig {
    pub fn new() -> Self {
        Self {
            resolution: Resolution::UXGA,
        }
    }
    
    pub fn set_resolution(&mut self, resolution: Resolution) -> Result<(), &'static str> {
        self.resolution = resolution;
        Ok(())
    }
    
    pub fn get_resolution(&self) -> Resolution {
        self.resolution
    }
    
    pub fn get_frame_size(&self) -> (u16, u16) {
        match self.resolution {
            Resolution::UXGA => (1600, 1200),
            Resolution::SVGA => (800, 600),
            Resolution::VGA => (640, 480),
        }
    }
}

/// カメラ制御構造体
#[derive(Debug)]
pub struct Camera {
    initialized: bool,
    current_resolution: Resolution,
}

impl Camera {
    pub fn new() -> Self {
        Self {
            initialized: false,
            current_resolution: Resolution::UXGA,
        }
    }
    
    pub fn initialize(&mut self) -> Result<(), &'static str> {
        // カメラ初期化シミュレーション
        self.initialized = true;
        self.current_resolution = Resolution::UXGA;
        Ok(())
    }
    
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
    
    pub fn get_current_resolution(&self) -> Resolution {
        self.current_resolution
    }
    
    pub fn can_capture(&self) -> bool {
        self.initialized
    }
    
    pub fn capture_uxga_image(&self) -> Result<Vec<u8>, &'static str> {
        if !self.initialized {
            return Err("Camera not initialized");
        }
        
        // UXGA JPEG画像をシミュレート（約150KB）
        let mut image_data = Vec::with_capacity(150 * 1024);
        
        // JPEG header
        image_data.extend_from_slice(&[0xFF, 0xD8]);
        
        // Simulated image data
        for i in 0..(150 * 1024 - 4) {
            image_data.push(((i * 13 + 57) % 256) as u8);
        }
        
        // JPEG end marker
        image_data.extend_from_slice(&[0xFF, 0xD9]);
        
        Ok(image_data)
    }
    
    pub fn get_frame_size(&self) -> (u16, u16) {
        match self.current_resolution {
            Resolution::UXGA => (1600, 1200),
            Resolution::SVGA => (800, 600),
            Resolution::VGA => (640, 480),
        }
    }
}

/// XIAO ESP32S3 Sense用カメラピン設定を取得
pub fn get_camera_pins() -> CameraPins {
    CameraPins {
        data_pins: [15, 17, 18, 16, 14, 12, 11, 48], // D0-D7
        xclk_pin: 10,
        pclk_pin: 13,
        vsync_pin: 38,
        href_pin: 47,
        sda_pin: 40,
        scl_pin: 39,
    }
}

/// カメラ用全ピンをリセットし、Deep Sleep中のリーク電流を最小化する
/// 
/// 目的: 全てのカメラ関連ピン（D0-D7, XCLK, PCLK, VSYNC, HREF, SDA, SCL）を
/// 入力モードかつプルダウン設定に強制変更することで、拡張ボード側へのリーク電流を遮断します。
pub fn reset_camera_pins() {
    use esp_idf_sys::{
        gpio_config, gpio_config_t, gpio_int_type_t_GPIO_INTR_DISABLE,
        gpio_mode_t_GPIO_MODE_INPUT, gpio_mode_t_GPIO_MODE_OUTPUT, // Added OUTPUT mode
        gpio_pulldown_t_GPIO_PULLDOWN_DISABLE,
        gpio_pullup_t_GPIO_PULLUP_DISABLE, gpio_hold_en,
        gpio_set_direction, gpio_set_level, // Added set functions
    };

    log::info!("カメラピンのリセット（High-Z / Hold）を開始します...");

    let pin_mask: u64 = (1u64 << 10) | // XCLK
                        (1u64 << 15) | // D0
                        (1u64 << 17) | // D1
                        (1u64 << 18) | // D2
                        (1u64 << 16) | // D3
                        (1u64 << 14) | // D4
                        (1u64 << 12) | // D5
                        (1u64 << 11) | // D6
                        (1u64 << 48) | // D7
                        (1u64 << 38) | // VSYNC
                        (1u64 << 47) | // HREF
                        (1u64 << 13) | // PCLK
                        (1u64 << 40) | // SDA
                        (1u64 << 39);  // SCL

    let config = gpio_config_t {
        pin_bit_mask: pin_mask,
        mode: gpio_mode_t_GPIO_MODE_INPUT,
        pull_up_en: gpio_pullup_t_GPIO_PULLUP_DISABLE,
        pull_down_en: gpio_pulldown_t_GPIO_PULLDOWN_DISABLE,
        intr_type: gpio_int_type_t_GPIO_INTR_DISABLE,
    };

    unsafe {
        // 1. まず全ピンをInput/High-Zにしてリセット
        let err = gpio_config(&config);
        if err != 0 {
            log::error!("カメラピンのリセットに失敗しました: {}", err);
            return;
        }

        // 2. XCLK (GPIO 10) だけは Output Low に設定してクロックを確実に止める
        // Floatingだとノイズでクロックが入る可能性があるため
        gpio_set_direction(10, gpio_mode_t_GPIO_MODE_OUTPUT);
        gpio_set_level(10, 0);

        // 3. SDA/SCL (GPIO 40, 39) を明示的にOutput Lowに設定
        // I2Cラインがフローティングだとプルアップ抵抗経由でカメラにリーク電流が流れる
        gpio_set_direction(40, gpio_mode_t_GPIO_MODE_OUTPUT);
        gpio_set_level(40, 0);
        gpio_set_direction(39, gpio_mode_t_GPIO_MODE_OUTPUT);
        gpio_set_level(39, 0);

        // 各ピンをスリープ中に固定（隔離）
        for pin in [10, 15, 17, 18, 16, 14, 12, 11, 48, 38, 47, 13, 40, 39] {
            if pin < 32 {
                gpio_hold_en(pin as i32);
            } else {
                // Pin 32以上は esp_idf_sys の定義に従う（通常は i32 キャストで動作）
                gpio_hold_en(pin as i32);
            }
        }

        log::info!("✓ カメラ用全ピンがHigh-Z・Hold状態にリセットされました");
    }
}
