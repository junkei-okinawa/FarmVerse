/// Streaming Controller - Central coordinator for streaming architecture
/// 
/// ESP-NOWからの受信データを即座にUSB CDCに転送する
/// ストリーミングアーキテクチャの中央制御を行います。
/// 
/// ## 主要機能
/// 
/// - デバイス別ストリーム管理との連携
/// - USB CDC即座転送制御
/// - エラーハンドリングと復旧
/// - 統計・監視機能

use super::{StreamingError, StreamingResult, StreamingStatistics};
use super::device_manager::{DeviceStreamManager, ProcessedFrame, StreamManagerConfig};
use crate::usb::cdc::UsbCdc;
use crate::esp_now::sender::EspNowSender;
use crate::esp_now::{AckMessage, MessageType, AckStatus};
use log::{debug, info, warn, error};

/// ストリーミング設定
#[derive(Debug, Clone)]
pub struct StreamingConfig {
    /// USB転送のタイムアウト時間（ミリ秒）
    pub usb_timeout_ms: u32,
    /// USB転送の最大リトライ回数
    pub usb_max_retries: u32,
    /// バッファクリーンアップ間隔（ミリ秒）
    pub cleanup_interval_ms: u64,
    /// 統計レポート間隔（ミリ秒）
    pub stats_report_interval_ms: u64,
    /// デバイス管理設定
    pub device_manager_config: StreamManagerConfig,
}

impl Default for StreamingConfig {
    fn default() -> Self {
        StreamingConfig {
            usb_timeout_ms: 100,              // 100ms timeout
            usb_max_retries: 3,               // 最大3回リトライ
            cleanup_interval_ms: 10_000,      // 10秒ごとにクリーンアップ
            stats_report_interval_ms: 30_000, // 30秒ごとに統計レポート
            device_manager_config: StreamManagerConfig::default(),
        }
    }
}

/// ストリーミング統計（拡張版）
#[derive(Debug, Clone, Default)]
pub struct StreamingStats {
    /// 基本統計
    pub basic: StreamingStatistics,
    /// USB転送統計
    pub usb_transfers: u64,
    pub usb_transfer_errors: u64,
    pub usb_retries: u64,
    /// バッファ統計
    pub buffer_cleanups: u64,
    /// 処理時間統計
    pub total_processing_time_ms: u64,
    pub max_processing_time_ms: u64,
    /// ACK送信統計
    pub acks_sent: u64,
    pub ack_errors: u64,
    /// スリープコマンド統計
    pub sleep_commands_sent: u64,
    pub sleep_command_errors: u64,
    /// 最後の統計リセット時刻
    pub last_reset: u64,
}

impl StreamingStats {
    /// 新しい統計インスタンスを作成
    pub fn new() -> Self {
        Self {
            last_reset: get_current_timestamp(),
            ..Default::default()
        }
    }
    
    /// USB転送成功をカウント
    pub fn count_usb_transfer(&mut self, bytes: usize) {
        self.usb_transfers += 1;
        self.basic.bytes_transferred += bytes as u64;
    }
    
    /// USB転送エラーをカウント
    pub fn count_usb_error(&mut self) {
        self.usb_transfer_errors += 1;
    }
    
    /// USB転送リトライをカウント
    pub fn count_usb_retry(&mut self) {
        self.usb_retries += 1;
    }
    
    /// バッファクリーンアップをカウント
    pub fn count_buffer_cleanup(&mut self, items: usize) {
        self.buffer_cleanups += items as u64;
    }
    
    /// 処理時間を記録
    pub fn record_processing_time(&mut self, time_ms: u64) {
        self.total_processing_time_ms += time_ms;
        if time_ms > self.max_processing_time_ms {
            self.max_processing_time_ms = time_ms;
        }
    }
    
    /// ACK送信成功をカウント
    pub fn count_ack_sent(&mut self) {
        self.acks_sent += 1;
    }
    
    /// ACK送信エラーをカウント
    pub fn count_ack_error(&mut self) {
        self.ack_errors += 1;
    }
    
    /// スリープコマンド送信成功をカウント
    pub fn count_sleep_command_sent(&mut self) {
        self.sleep_commands_sent += 1;
    }
    
    /// スリープコマンド送信エラーをカウント
    pub fn count_sleep_command_error(&mut self) {
        self.sleep_command_errors += 1;
    }
    
    /// 平均処理時間を計算
    pub fn average_processing_time_ms(&self) -> f64 {
        if self.basic.frames_processed > 0 {
            self.total_processing_time_ms as f64 / self.basic.frames_processed as f64
        } else {
            0.0
        }
    }
    
    /// USB転送成功率を計算
    pub fn usb_success_rate(&self) -> f32 {
        if self.usb_transfers + self.usb_transfer_errors > 0 {
            (self.usb_transfers as f32 / (self.usb_transfers + self.usb_transfer_errors) as f32) * 100.0
        } else {
            0.0
        }
    }
    
    /// ACK送信成功率を計算
    pub fn ack_success_rate(&self) -> f32 {
        if self.acks_sent + self.ack_errors > 0 {
            (self.acks_sent as f32 / (self.acks_sent + self.ack_errors) as f32) * 100.0
        } else {
            0.0
        }
    }
    
    /// スリープコマンド送信成功率を計算
    pub fn sleep_command_success_rate(&self) -> f32 {
        if self.sleep_commands_sent + self.sleep_command_errors > 0 {
            (self.sleep_commands_sent as f32 / (self.sleep_commands_sent + self.sleep_command_errors) as f32) * 100.0
        } else {
            0.0
        }
    }
    
    /// 統計をリセット
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

/// ストリーミングコントローラー
pub struct StreamingController {
    /// デバイスストリーム管理者
    device_manager: DeviceStreamManager,
    /// ESP-NOW送信機能
    esp_now_sender: EspNowSender,
    /// 設定
    config: StreamingConfig,
    /// 統計情報
    stats: StreamingStats,
    /// 最後のクリーンアップ時刻
    last_cleanup: u64,
    /// 最後の統計レポート時刻
    last_stats_report: u64,
}

impl StreamingController {
    /// 新しいストリーミングコントローラーを作成
    pub fn new(config: StreamingConfig) -> Self {
        let device_manager = DeviceStreamManager::new(config.device_manager_config.clone());
        let esp_now_sender = EspNowSender::new();
        let current_time = get_current_timestamp();
        
        StreamingController {
            device_manager,
            esp_now_sender,
            config,
            stats: StreamingStats::new(),
            last_cleanup: current_time,
            last_stats_report: current_time,
        }
    }
    
    /// ESP-NOWから受信したデータを処理（ACK返信付き）
    pub fn process_esp_now_data(
        &mut self,
        mac_address: [u8; 6],
        data: &[u8],
        usb_cdc: &mut UsbCdc,
    ) -> StreamingResult<usize> {
        let start_time = get_current_timestamp();
        let mut total_transferred = 0;
        
        debug!("StreamingController: processing {} bytes from {:02X?}", data.len(), mac_address);
        
        // デバイスストリーム管理者でデータを処理
        let processed_frames = self.device_manager.process_data(mac_address, data)?;
        
        // 処理されたフレームを即座にUSB CDCに転送
        for frame in &processed_frames {
            match self.transfer_frame_to_usb(&frame, usb_cdc) {
                Ok(bytes_sent) => {
                    total_transferred += bytes_sent;
                    self.stats.count_usb_transfer(bytes_sent);
                    debug!("StreamingController: transferred {} bytes for frame seq {}", 
                           bytes_sent, frame.sequence);
                    
                    // フレーム処理成功後にACKを送信
                    self.send_ack_for_frame(&frame, mac_address, AckStatus::Success);
                }
                Err(e) => {
                    self.stats.count_usb_error();
                    error!("StreamingController: USB transfer failed for frame seq {}: {}", 
                           frame.sequence, e);
                    
                    // USB転送失敗時もACKを送信（エラーステータス付き）
                    self.send_ack_for_frame(&frame, mac_address, AckStatus::BufferOverflow);
                    // エラーが発生しても他のフレーム処理は継続
                }
            }
        }
        
        // 処理時間を記録
        let processing_time = get_current_timestamp() - start_time;
        self.stats.record_processing_time(processing_time);
        
        // 定期的なメンテナンス処理
        self.periodic_maintenance();
        
        Ok(total_transferred)
    }
    
    /// フレーム処理結果に対してACKを送信
    fn send_ack_for_frame(&mut self, frame: &ProcessedFrame, mac_address: [u8; 6], status: AckStatus) {
        // データフレームタイプを決定（実際のフレームタイプに基づく）
        let acked_message_type = MessageType::DataFrame; // 現在はすべてデータフレームとして扱う
        
        let ack = AckMessage::new(frame.sequence, acked_message_type, status);
        let ack_data = ack.serialize();
        
        match self.esp_now_sender.send_data(mac_address, &ack_data) {
            Ok(()) => {
                info!("✓ ACK sent successfully for frame seq {} to {:02X?} (status: {:?})", 
                      frame.sequence, mac_address, status);
                self.stats.count_ack_sent();
            }
            Err(e) => {
                warn!("✗ Failed to send ACK for frame seq {} to {:02X?}: {:?}", 
                      frame.sequence, mac_address, e);
                self.stats.count_ack_error();
            }
        }
    }
    
    /// PythonサーバーからのスリープコマンドをESP-NOWで転送
    pub fn forward_sleep_command(&mut self, mac_address: [u8; 6], sleep_seconds: u32) -> StreamingResult<()> {
        info!("Forwarding sleep command: {} seconds to {:02X?}", sleep_seconds, mac_address);
        
        match self.esp_now_sender.send_sleep_command(
            &format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                     mac_address[0], mac_address[1], mac_address[2],
                     mac_address[3], mac_address[4], mac_address[5]),
            sleep_seconds
        ) {
            Ok(()) => {
                info!("✓ Sleep command forwarded successfully");
                self.stats.count_sleep_command_sent();
                Ok(())
            }
            Err(e) => {
                error!("✗ Failed to forward sleep command: {:?}", e);
                self.stats.count_sleep_command_error();
                Err(StreamingError::EspNowSendError(format!("Sleep command forward failed: {:?}", e)))
            }
        }
    }
    
    /// フレームをUSB CDCに転送
    fn transfer_frame_to_usb(
        &mut self,
        frame: &ProcessedFrame,
        usb_cdc: &mut UsbCdc,
    ) -> StreamingResult<usize> {
        let mac_str = frame.mac_string();
        let mut retry_count = 0;
        
        loop {
            match usb_cdc.send_frame(&frame.payload, &mac_str) {
                Ok(bytes_sent) => {
                    if retry_count > 0 {
                        self.stats.count_usb_retry();
                        debug!("StreamingController: USB transfer succeeded after {} retries", retry_count);
                    }
                    return Ok(bytes_sent);
                }
                Err(e) => {
                    retry_count += 1;
                    if retry_count > self.config.usb_max_retries {
                        return Err(StreamingError::UsbTransferError(
                            format!("Max retries exceeded: {}", e)
                        ));
                    }
                    
                    warn!("StreamingController: USB transfer retry {}/{} for {}: {}", 
                          retry_count, self.config.usb_max_retries, mac_str, e);
                    
                    // 短い遅延後にリトライ
                    esp_idf_svc::hal::delay::FreeRtos::delay_ms(10);
                }
            }
        }
    }
    
    /// 定期的なメンテナンス処理
    fn periodic_maintenance(&mut self) {
        let current_time = get_current_timestamp();
        
        // バッファクリーンアップ
        if current_time - self.last_cleanup > self.config.cleanup_interval_ms {
            let cleaned_items = self.device_manager.cleanup_all_buffers();
            if cleaned_items > 0 {
                self.stats.count_buffer_cleanup(cleaned_items);
            }
            
            let removed_devices = self.device_manager.cleanup_inactive_devices();
            if removed_devices > 0 {
                info!("StreamingController: removed {} inactive devices", removed_devices);
            }
            
            self.last_cleanup = current_time;
        }
        
        // 統計レポート
        if current_time - self.last_stats_report > self.config.stats_report_interval_ms {
            self.report_statistics();
            self.last_stats_report = current_time;
        }
    }
    
    /// 統計レポートを出力
    fn report_statistics(&self) {
        let device_count = self.device_manager.device_count();
        let (buffer_used, buffer_total) = self.device_manager.total_buffer_usage();
        let global_stats = self.device_manager.global_statistics();
        
        info!("=== Streaming Statistics ===");
        info!("Active devices: {}", device_count);
        info!("Frames received: {}", global_stats.frames_received);
        info!("Frames processed: {}", global_stats.frames_processed);
        info!("Frame success rate: {:.1}%", global_stats.success_rate());
        info!("USB transfers: {}", self.stats.usb_transfers);
        info!("USB success rate: {:.1}%", self.stats.usb_success_rate());
        info!("Total bytes transferred: {} bytes", self.stats.basic.bytes_transferred);
        info!("Buffer usage: {} / {} bytes ({:.1}%)", 
              buffer_used, buffer_total, 
              if buffer_total > 0 { (buffer_used as f32 / buffer_total as f32) * 100.0 } else { 0.0 });
        info!("Average processing time: {:.2}ms", self.stats.average_processing_time_ms());
        info!("Max processing time: {}ms", self.stats.max_processing_time_ms);
        
        if global_stats.frames_error > 0 {
            warn!("Frame errors: {} (Checksum: {}, Sequence: {}, Buffer full: {})",
                  global_stats.frames_error,
                  global_stats.checksum_error_count,
                  global_stats.sequence_error_count,
                  global_stats.buffer_full_count);
        }
    }
    
    /// デバイスを手動で登録
    pub fn register_device(&mut self, mac_address: [u8; 6], name: String) -> StreamingResult<()> {
        self.device_manager.register_device(mac_address, name)
    }
    
    /// デバイスを登録解除
    pub fn unregister_device(&mut self, mac_address: &[u8; 6]) -> StreamingResult<()> {
        self.device_manager.unregister_device(mac_address)
    }
    
    /// 登録デバイス一覧を取得
    pub fn list_devices(&self) -> Vec<([u8; 6], String, super::device_manager::StreamStatus)> {
        self.device_manager.get_devices()
    }
    
    /// 特定デバイスの統計を取得
    pub fn get_device_statistics(&self, mac_address: &[u8; 6]) -> StreamingResult<&StreamingStatistics> {
        self.device_manager.get_device_statistics(mac_address)
    }
    
    /// 全体統計を取得
    pub fn get_statistics(&self) -> &StreamingStats {
        &self.stats
    }
    
    /// 特定デバイスのストリームをリセット
    pub fn reset_device_stream(&mut self, mac_address: &[u8; 6]) -> StreamingResult<()> {
        self.device_manager.reset_device_stream(mac_address)
    }
    
    /// 全統計をリセット
    pub fn reset_all_statistics(&mut self) {
        self.device_manager.reset_statistics();
        self.stats.reset();
        info!("StreamingController: all statistics reset");
    }
    
    /// 手動クリーンアップを実行
    pub fn force_cleanup(&mut self) -> (usize, usize) {
        let buffer_items = self.device_manager.cleanup_all_buffers();
        let inactive_devices = self.device_manager.cleanup_inactive_devices();
        
        if buffer_items > 0 || inactive_devices > 0 {
            info!("StreamingController: force cleanup - {} buffer items, {} inactive devices",
                  buffer_items, inactive_devices);
        }
        
        (buffer_items, inactive_devices)
    }
    
    /// 設定を更新
    pub fn update_config(&mut self, config: StreamingConfig) {
        self.config = config;
        info!("StreamingController: configuration updated");
    }
    
    /// 現在の設定を取得
    pub fn get_config(&self) -> &StreamingConfig {
        &self.config
    }
}

impl Default for StreamingController {
    fn default() -> Self {
        Self::new(StreamingConfig::default())
    }
}

/// 現在のタイムスタンプを取得（ミリ秒）
fn get_current_timestamp() -> u64 {
    unsafe {
        let ticks = esp_idf_svc::sys::xTaskGetTickCount();
        let ms_per_tick = 1000 / esp_idf_svc::sys::configTICK_RATE_HZ;
        (ticks * ms_per_tick) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_config_default() {
        let config = StreamingConfig::default();
        assert_eq!(config.usb_timeout_ms, 100);
        assert_eq!(config.usb_max_retries, 3);
        assert_eq!(config.cleanup_interval_ms, 10_000);
        assert_eq!(config.stats_report_interval_ms, 30_000);
    }

    #[test]
    fn test_streaming_stats_basic() {
        let mut stats = StreamingStats::new();
        
        // USB転送統計
        stats.count_usb_transfer(100);
        assert_eq!(stats.usb_transfers, 1);
        assert_eq!(stats.basic.bytes_transferred, 100);
        
        stats.count_usb_error();
        assert_eq!(stats.usb_transfer_errors, 1);
        assert_eq!(stats.usb_success_rate(), 50.0);
        
        // 処理時間統計
        stats.record_processing_time(10);
        stats.record_processing_time(20);
        assert_eq!(stats.max_processing_time_ms, 20);
        
        // 基本統計との連携が必要な場合
        stats.basic.count_frame_processed(50);
        assert!(stats.average_processing_time_ms() > 0.0);
    }

    #[test]
    fn test_streaming_controller_creation() {
        let config = StreamingConfig::default();
        let controller = StreamingController::new(config);
        
        assert_eq!(controller.list_devices().len(), 0);
        assert_eq!(controller.get_statistics().usb_transfers, 0);
    }

    #[test]
    fn test_streaming_controller_device_management() {
        let config = StreamingConfig::default();
        let mut controller = StreamingController::new(config);
        
        let mac = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
        
        // デバイス登録
        controller.register_device(mac, "Test Device".to_string()).unwrap();
        assert_eq!(controller.list_devices().len(), 1);
        
        // デバイス登録解除
        controller.unregister_device(&mac).unwrap();
        assert_eq!(controller.list_devices().len(), 0);
    }
}
