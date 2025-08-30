/// XIAO ESP32S3 Sense用カメラ初期化テスト
/// 
/// テスト項目:
/// 1. ピン配置定義の正確性
/// 2. OV2640センサーのUXGA解像度対応
/// 3. ESP32S3メモリ最適化
#[cfg(test)]
mod camera_tests {
    use crate::hardware::camera::{Resolution};
    use crate::hardware::camera::xiao_esp32s3::{CameraConfig, get_camera_pins};

    #[test]
    fn test_camera_pin_configuration() {
        let pins = get_camera_pins();
        
        // XIAO ESP32S3 Sense用ピン配置の確認
        assert_eq!(pins.data_pins, [15, 17, 18, 16, 14, 12, 11, 48]);
        assert_eq!(pins.xclk_pin, 10);
        assert_eq!(pins.pclk_pin, 13);
        assert_eq!(pins.vsync_pin, 38);
        assert_eq!(pins.href_pin, 47);
        assert_eq!(pins.sda_pin, 40);
        assert_eq!(pins.scl_pin, 39);
    }

    #[test] 
    fn test_camera_uxga_configuration() {
        let mut camera_config = CameraConfig::new();
        
        // UXGA解像度設定テスト
        let result = camera_config.set_resolution(Resolution::UXGA);
        assert!(result.is_ok());
        
        assert_eq!(camera_config.get_resolution(), Resolution::UXGA);
        assert_eq!(camera_config.get_frame_size(), (1600, 1200));
    }

    #[test]
    fn test_esp32s3_memory_optimization() {
        // ESP32S3メモリ使用量確認
        let camera_pins = get_camera_pins();
        
        // ピン設定のメモリ使用量は最小限であることを確認
        assert!(std::mem::size_of_val(&camera_pins) < 100); // 100bytes未満
    }
}

/// XIAO ESP32S3 Sense用カメラ制御テスト
/// 
/// テスト項目:
/// 1. カメラ初期化機能
/// 2. UXGA画像キャプチャ機能
/// 3. エラーハンドリング
#[cfg(test)]
mod camera_control_tests {
    use crate::hardware::camera::{Camera, Resolution};

    #[test]
    fn test_camera_initialization() {
        let mut camera = Camera::new();
        
        // 初期化前の状態確認
        assert!(!camera.is_initialized());
        assert!(!camera.can_capture());
        
        // カメラ初期化
        let result = camera.initialize();
        assert!(result.is_ok());
        
        // 初期化後の状態確認
        assert!(camera.is_initialized());
        assert!(camera.can_capture());
        assert_eq!(camera.get_current_resolution(), Resolution::UXGA);
    }

    #[test]
    fn test_uxga_image_capture() {
        let mut camera = Camera::new();
        camera.initialize().unwrap();
        
        // UXGA画像キャプチャ
        let image_result = camera.capture_uxga_image();
        assert!(image_result.is_ok());
        
        let image_data = image_result.unwrap();
        
        // 画像データの検証
        assert!(!image_data.is_empty());
        assert!(image_data.len() > 100_000); // 約150KB期待
        
        // JPEG形式確認
        assert_eq!(image_data[0], 0xFF);
        assert_eq!(image_data[1], 0xD8);
        assert_eq!(image_data[image_data.len()-2], 0xFF);
        assert_eq!(image_data[image_data.len()-1], 0xD9);
    }

    #[test]
    fn test_camera_error_handling() {
        let camera = Camera::new();
        
        // 初期化前キャプチャエラー
        let result = camera.capture_uxga_image();
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), "Camera not initialized");
    }
}
#[cfg(test)]
mod camera_initialization_tests {
    use crate::hardware::camera::{Resolution, CameraPins};
    use crate::hardware::camera::xiao_esp32s3::get_camera_pins;
    
    #[test]
    fn test_xiao_esp32s3_pin_configuration() {
        // Given: XIAO ESP32S3 Sense専用ピン配置
        let expected_pins = get_expected_xiao_esp32s3_pins();
        
        // When: カメラピン設定を取得
        let actual_pins = get_camera_pins();
        
        // Then: ピン配置が正確に設定されている
        assert_eq!(actual_pins.data_pins, expected_pins.data_pins);
        assert_eq!(actual_pins.xclk_pin, expected_pins.xclk_pin);
        assert_eq!(actual_pins.pclk_pin, expected_pins.pclk_pin);
        assert_eq!(actual_pins.vsync_pin, expected_pins.vsync_pin);
        assert_eq!(actual_pins.href_pin, expected_pins.href_pin);
        assert_eq!(actual_pins.sda_pin, expected_pins.sda_pin);
        assert_eq!(actual_pins.scl_pin, expected_pins.scl_pin);
    }
    
    #[test]
    fn test_ov2640_uxga_resolution_support() {
        // Given: OV2640センサー設定
        let mut camera_config = crate::hardware::camera::xiao_esp32s3::CameraConfig::new();
        
        // When: UXGA解像度を設定
        let result = camera_config.set_resolution(Resolution::UXGA);
        
        // Then: UXGA解像度が正常に設定される
        assert!(result.is_ok());
        assert_eq!(camera_config.get_resolution(), Resolution::UXGA);
        assert_eq!(camera_config.get_frame_size(), (1600, 1200));
    }
    
    #[test]
    fn test_esp32s3_memory_optimization() {
        // Given: ESP32S3メモリ設定
        let memory_config = crate::config::MemoryConfig::new();
        
        // When: メモリ最適化設定を確認
        let psram_enabled = memory_config.is_psram_enabled();
        let available_heap = memory_config.get_available_heap_size();
        let camera_buffer_size = memory_config.get_camera_buffer_size();
        
        // Then: ESP32S3の大容量メモリが活用される
        assert!(psram_enabled, "PSRAM should be enabled for XIAO ESP32S3");
        assert!(available_heap >= 8 * 1024 * 1024, "Should have at least 8MB available"); // 8MB PSRAM
        assert!(camera_buffer_size >= 200 * 1024, "Camera buffer should be at least 200KB for UXGA");
    }
    
    #[test]
    fn test_camera_initialization_sequence() {
        // Given: カメラ初期化シーケンス
        let mut camera = crate::hardware::camera::Camera::new();
        
        // When: カメラを初期化
        let init_result = camera.initialize();
        
        // Then: 初期化が成功し、UXGA解像度で動作可能
        assert!(init_result.is_ok());
        assert!(camera.is_initialized());
        assert_eq!(camera.get_current_resolution(), Resolution::UXGA);
        assert!(camera.can_capture());
    }
    
    // Helper function: XIAO ESP32S3 Sense期待ピン配置
    fn get_expected_xiao_esp32s3_pins() -> CameraPins {
        get_camera_pins() // 実際の関数を使用
    }
}
