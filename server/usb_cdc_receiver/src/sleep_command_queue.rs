/// スリープコマンド送信のキューシステム
/// 
/// ESP-NOWの競合を回避するため、スリープコマンドを順序化して送信します。

use esp_idf_svc::hal::delay::FreeRtos;
use heapless::Deque;
use log::{info, warn, error};
use crate::esp_now::sender::EspNowSender;

/// スリープコマンドキューの最大サイズ
const SLEEP_COMMAND_QUEUE_SIZE: usize = 10;

/// スリープコマンド送信間隔（ミリ秒）
const SLEEP_COMMAND_INTERVAL_MS: u32 = 500;

/// スリープコマンド情報
#[derive(Debug, Clone)]
pub struct SleepCommand {
    pub mac_address: String,
    pub sleep_seconds: u32,
    pub retry_count: u32,
}

impl SleepCommand {
    pub fn new(mac_address: String, sleep_seconds: u32) -> Self {
        Self {
            mac_address,
            sleep_seconds,
            retry_count: 0,
        }
    }
}

/// スリープコマンドキューシステム
pub struct SleepCommandQueue {
    queue: Deque<SleepCommand, SLEEP_COMMAND_QUEUE_SIZE>,
    last_send_time: u64,
}

impl SleepCommandQueue {
    /// 新しいキューを作成
    pub fn new() -> Self {
        Self {
            queue: Deque::new(),
            last_send_time: 0,
        }
    }

    /// スリープコマンドをキューに追加
    pub fn enqueue(&mut self, mac_address: String, sleep_seconds: u32) -> Result<(), &'static str> {
        let command = SleepCommand::new(mac_address, sleep_seconds);
        
        // 同じMACアドレスの重複コマンドをチェック
        if self.queue.iter().any(|cmd| cmd.mac_address == command.mac_address) {
            warn!("Sleep command for {} already queued, skipping duplicate", command.mac_address);
            return Ok(());
        }
        
        match self.queue.push_back(command.clone()) {
            Ok(()) => {
                info!("Sleep command queued: {} -> {}s (queue size: {})", 
                      command.mac_address, command.sleep_seconds, self.queue.len());
                Ok(())
            }
            Err(_) => {
                error!("Sleep command queue is full, dropping command for {}", command.mac_address);
                Err("Queue full")
            }
        }
    }

    /// キューからスリープコマンドを処理
    pub fn process_queue(&mut self, esp_now_sender: &EspNowSender) -> bool {
        let current_time = self.get_current_time_ms();
        
        // 送信間隔チェック
        if current_time - self.last_send_time < SLEEP_COMMAND_INTERVAL_MS as u64 {
            return false; // まだ間隔が足りない
        }

        if let Some(mut command) = self.queue.pop_front() {
            info!("Processing sleep command: {} -> {}s (attempt {})", 
                  command.mac_address, command.sleep_seconds, command.retry_count + 1);
            
            match esp_now_sender.send_sleep_command(&command.mac_address, command.sleep_seconds) {
                Ok(()) => {
                    info!("✓ Sleep command sent successfully: {} -> {}s", 
                          command.mac_address, command.sleep_seconds);
                    self.last_send_time = current_time;
                    true
                }
                Err(e) => {
                    error!("✗ Sleep command send failed: {} -> {}s, error: {:?}", 
                           command.mac_address, command.sleep_seconds, e);
                    
                    command.retry_count += 1;
                    const MAX_RETRIES: u32 = 2;
                    
                    if command.retry_count < MAX_RETRIES {
                        // リトライのためキューの先頭に戻す
                        warn!("Retrying sleep command: {} (attempt {}/{})", 
                              command.mac_address, command.retry_count + 1, MAX_RETRIES + 1);
                        
                        if let Err(_) = self.queue.push_front(command) {
                            error!("Failed to requeue sleep command for retry");
                        }
                    } else {
                        error!("Sleep command failed after {} attempts: {}", 
                               MAX_RETRIES + 1, command.mac_address);
                    }
                    
                    self.last_send_time = current_time;
                    false
                }
            }
        } else {
            false // キューが空
        }
    }

    /// キューが空かどうか確認
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// キューのサイズを取得
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    /// 現在時刻を取得（ミリ秒）
    fn get_current_time_ms(&self) -> u64 {
        unsafe {
            esp_idf_svc::sys::xTaskGetTickCount() as u64 * 1000 / esp_idf_svc::sys::configTICK_RATE_HZ as u64
        }
    }
}

/// グローバルスリープコマンドキュー
static mut SLEEP_QUEUE: Option<SleepCommandQueue> = None;

/// グローバルキューを初期化
pub fn init_sleep_command_queue() {
    unsafe {
        SLEEP_QUEUE = Some(SleepCommandQueue::new());
    }
    info!("Sleep command queue initialized");
}

/// スリープコマンドをグローバルキューに追加
pub fn enqueue_sleep_command(mac_address: String, sleep_seconds: u32) -> Result<(), &'static str> {
    unsafe {
        if let Some(queue) = &mut SLEEP_QUEUE {
            queue.enqueue(mac_address, sleep_seconds)
        } else {
            error!("Sleep command queue not initialized");
            Err("Queue not initialized")
        }
    }
}

/// グローバルキューを処理
pub fn process_sleep_command_queue(esp_now_sender: &EspNowSender) -> bool {
    unsafe {
        if let Some(queue) = &mut SLEEP_QUEUE {
            queue.process_queue(esp_now_sender)
        } else {
            false
        }
    }
}

/// キューが空かどうか確認
pub fn is_sleep_command_queue_empty() -> bool {
    unsafe {
        if let Some(queue) = &SLEEP_QUEUE {
            queue.is_empty()
        } else {
            true
        }
    }
}

/// キューのサイズを取得
pub fn get_sleep_command_queue_len() -> usize {
    unsafe {
        if let Some(queue) = &SLEEP_QUEUE {
            queue.len()
        } else {
            0
        }
    }
}
