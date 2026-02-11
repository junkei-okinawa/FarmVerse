/// Camera module for XIAO ESP32S3 Sense
/// 
/// This module provides camera functionality including:
/// - XIAO ESP32S3 Sense specific pin configuration
/// - OV2640 camera control 
/// - UXGA image capture capability
/// - Frame size configuration

pub mod config;
pub mod controller;
pub mod ov2640;
pub mod xiao_esp32s3;

// Re-export main types
pub use xiao_esp32s3::{Camera, CameraPins, Resolution, reset_camera_pins, get_camera_pins}; // テストで使用
pub use config::*;
pub use controller::*;

// Convenience function for capturing UXGA images
pub fn capture_uxga_image() -> Result<Vec<u8>, &'static str> {
    let mut camera = Camera::new();
    camera.initialize()?;
    camera.capture_uxga_image()
}
