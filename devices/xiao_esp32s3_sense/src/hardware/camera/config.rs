/// Camera configuration types and utilities
/// 
/// This module provides configuration structures for different camera use cases:
/// - Streaming configuration for real-time image transmission
/// - Batch configuration for multiple image capture

#[allow(dead_code)] // 将来的にストリーミング機能で使用予定

use super::xiao_esp32s3::Resolution;

/// Streaming specific camera configuration
#[derive(Debug, Clone)]
pub struct StreamingCameraConfig {
    pub resolution: Resolution,
    pub chunk_size: usize,
    pub max_retries: u8,
    pub timeout_ms: u32,
}

impl Default for StreamingCameraConfig {
    fn default() -> Self {
        Self {
            resolution: Resolution::UXGA,
            chunk_size: 1024,  // 1KB chunks for streaming
            max_retries: 3,
            timeout_ms: 5000,
        }
    }
}

impl StreamingCameraConfig {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_resolution(mut self, resolution: Resolution) -> Self {
        self.resolution = resolution;
        self
    }
    
    pub fn with_chunk_size(mut self, chunk_size: usize) -> Self {
        self.chunk_size = chunk_size;
        self
    }
    
    pub fn with_max_retries(mut self, max_retries: u8) -> Self {
        self.max_retries = max_retries;
        self
    }
    
    pub fn with_timeout(mut self, timeout_ms: u32) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }
}

/// Batch capture camera configuration
#[derive(Debug, Clone)]
pub struct BatchCameraConfig {
    pub resolution: Resolution,
    pub capture_count: u8,
    pub interval_ms: u32,
}

impl Default for BatchCameraConfig {
    fn default() -> Self {
        Self {
            resolution: Resolution::UXGA,
            capture_count: 1,
            interval_ms: 1000,
        }
    }
}
