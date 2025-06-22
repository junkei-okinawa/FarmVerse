use core::mem::MaybeUninit;
use heapless::spsc::{Consumer, Producer, Queue};
use log::{debug, error, warn};
use std::sync::Mutex;
use super::{QueueError, QueueResult, ReceivedData};

/// キューの容量定数
pub const QUEUE_CAPACITY: usize = 512 + 1; // 512データ要素 + 余裕

/// 受信データのグローバルプロデューサー
static RECEIVED_DATA_PRODUCER: Mutex<Option<Producer<'static, ReceivedData, QUEUE_CAPACITY>>> =
    Mutex::new(None);

/// 受信データのグローバルコンシューマー
static RECEIVED_DATA_CONSUMER: Mutex<Option<Consumer<'static, ReceivedData, QUEUE_CAPACITY>>> =
    Mutex::new(None);

/// キュー自体のための静的バッファ（MaybeUninitで初期化）
static mut Q_BUFFER: MaybeUninit<Queue<ReceivedData, QUEUE_CAPACITY>> = MaybeUninit::uninit();

/// データキューを初期化します
///
/// # 安全性
///
/// この関数は、メインスレッドの起動時に一度だけ呼び出す必要があります。
/// 複数回の呼び出しや並行実行は未定義の動作を引き起こす可能性があります。
pub fn initialize_data_queue() -> bool {
    unsafe {
        // 静的バッファ内にキューを初期化
        Q_BUFFER.write(Queue::new());
        
        // 初期化されたキューへの可変参照を取得し、分割
        let (p, c) = Q_BUFFER.assume_init_mut().split();
        
        // グローバル変数に格納
        *RECEIVED_DATA_PRODUCER.lock().unwrap() = Some(p);
        *RECEIVED_DATA_CONSUMER.lock().unwrap() = Some(c);
    }
    
    debug!("Data queue initialized with capacity: {}", QUEUE_CAPACITY);
    true
}

/// キューにデータを追加します
///
/// # 引数
///
/// * `data` - キューに追加するデータ
///
/// # 戻り値
///
/// * `QueueResult<()>` - 成功した場合は`Ok(())`、失敗した場合は`Err(QueueError)`
pub fn enqueue(data: ReceivedData) -> QueueResult<()> {
    // プロデューサーのロックを取得
    let mut producer_guard = RECEIVED_DATA_PRODUCER
        .lock()
        .map_err(|_| QueueError::LockError)?;
    
    // プロデューサーの参照を取得
    let producer = producer_guard
        .as_mut()
        .ok_or(QueueError::Other("Queue not initialized"))?;
    
    // データをキューに追加
    producer
        .enqueue(data)
        .map_err(|_| QueueError::Full)
}

/// キューからデータを取り出します
///
/// # 戻り値
///
/// * `QueueResult<ReceivedData>` - データがある場合は`Ok(ReceivedData)`、ない場合は`Err(QueueError)`
pub fn dequeue() -> QueueResult<ReceivedData> {
    // コンシューマーのロックを取得
    let mut consumer_guard = RECEIVED_DATA_CONSUMER
        .lock()
        .map_err(|_| QueueError::LockError)?;
    
    // コンシューマーの参照を取得
    let consumer = consumer_guard
        .as_mut()
        .ok_or(QueueError::Other("Queue not initialized"))?;
    
    // キューからデータを取り出す
    consumer
        .dequeue()
        .ok_or(QueueError::Empty)
}

/// ESP-NOW受信コールバックからデータをキューに追加するためのヘルパー関数
///
/// # 引数
///
/// * `data` - キューに追加するデータ
///
/// # 戻り値
///
/// * `bool` - 成功した場合は`true`、失敗した場合は`false`
pub fn try_enqueue_from_callback(data: ReceivedData) -> bool {
    match enqueue(data) {
        Ok(_) => true,
        Err(QueueError::Full) => {
            warn!("Data queue full in ESP-NOW callback!");
            false
        }
        Err(e) => {
            error!("Failed to enqueue data in ESP-NOW callback: {}", e);
            false
        }
    }
}

/// キューの現在のサイズを取得します（デバッグ用）
pub fn get_queue_usage() -> QueueResult<(usize, usize)> {
    let consumer_guard = RECEIVED_DATA_CONSUMER
        .lock()
        .map_err(|_| QueueError::LockError)?;
    
    let consumer = consumer_guard
        .as_ref()
        .ok_or(QueueError::Other("Queue not initialized"))?;
    
    // heaplessのQueueは直接サイズを確認する方法を提供していないため、
    // 実際の実装ではコンシューマーから得られる情報に基づいて推定することになります。
    // ここでは例として単純な値を返します。
    Ok((consumer.len(), QUEUE_CAPACITY))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // 注: テスト用のキューを使用するため、テストを逐次実行する必要があります
    
    #[test]
    fn test_queue_operations() {
        // テスト用にキューを初期化
        initialize_data_queue();
        
        // テストデータ
        let test_mac = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc];
        let test_data = vec![1, 2, 3, 4, 5];
        
        // データをエンキュー
        let data = ReceivedData {
            mac: test_mac,
            data: test_data.clone(),
        };
        
        assert!(try_enqueue_from_callback(data));
        
        // データをデキュー
        let result = dequeue();
        assert!(result.is_ok());
        
        let received = result.unwrap();
        assert_eq!(received.mac, test_mac);
        assert_eq!(received.data, test_data);
        
        // キューが空になったことを確認
        assert!(dequeue().is_err());
    }
}
