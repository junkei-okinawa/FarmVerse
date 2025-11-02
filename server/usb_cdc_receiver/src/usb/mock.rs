use super::{UsbError, UsbInterface, UsbResult};
use std::sync::{Arc, Mutex};

/// テスト用のUSB CDCモック実装
/// 
/// 実際のUSBハードウェアを使わずにUSB CDC通信をシミュレートします。
/// 送信されたデータと受信コマンドを記録し、テストで検証できます。
#[derive(Debug, Clone)]
pub struct MockUsbCdc {
    /// 送信されたデータの記録
    pub sent_data: Arc<Mutex<Vec<Vec<u8>>>>,
    /// 読み取り用のコマンドキュー（先頭から取り出される）
    pub command_queue: Arc<Mutex<Vec<String>>>,
    /// 読み取り用のデータキュー（先頭から取り出される）
    pub read_data_queue: Arc<Mutex<Vec<Vec<u8>>>>,
    /// エラーシミュレーション用のフラグ
    pub simulate_write_error: Arc<Mutex<bool>>,
    pub simulate_read_error: Arc<Mutex<bool>>,
    pub simulate_timeout: Arc<Mutex<bool>>,
}

impl Default for MockUsbCdc {
    fn default() -> Self {
        Self::new()
    }
}

impl MockUsbCdc {
    /// 新しいMockUsbCdcインスタンスを作成します
    pub fn new() -> Self {
        Self {
            sent_data: Arc::new(Mutex::new(Vec::new())),
            command_queue: Arc::new(Mutex::new(Vec::new())),
            read_data_queue: Arc::new(Mutex::new(Vec::new())),
            simulate_write_error: Arc::new(Mutex::new(false)),
            simulate_read_error: Arc::new(Mutex::new(false)),
            simulate_timeout: Arc::new(Mutex::new(false)),
        }
    }

    /// テスト用: 読み取り用コマンドをキューに追加
    pub fn queue_command(&self, command: String) {
        self.command_queue.lock().unwrap().push(command);
    }

    /// テスト用: 読み取り用データをキューに追加
    pub fn queue_read_data(&self, data: Vec<u8>) {
        self.read_data_queue.lock().unwrap().push(data);
    }

    /// テスト用: 送信されたデータを取得
    pub fn get_sent_data(&self) -> Vec<Vec<u8>> {
        self.sent_data.lock().unwrap().clone()
    }

    /// テスト用: 送信データをクリア
    pub fn clear_sent_data(&self) {
        self.sent_data.lock().unwrap().clear();
    }

    /// テスト用: 書き込みエラーをシミュレート
    pub fn set_write_error(&self, enable: bool) {
        *self.simulate_write_error.lock().unwrap() = enable;
    }

    /// テスト用: 読み取りエラーをシミュレート
    pub fn set_read_error(&self, enable: bool) {
        *self.simulate_read_error.lock().unwrap() = enable;
    }

    /// テスト用: タイムアウトをシミュレート
    pub fn set_timeout(&self, enable: bool) {
        *self.simulate_timeout.lock().unwrap() = enable;
    }
}

impl UsbInterface for MockUsbCdc {
    fn write(&mut self, data: &[u8], _timeout_ms: u32) -> UsbResult<usize> {
        // エラーシミュレーション
        if *self.simulate_timeout.lock().unwrap() {
            return Err(UsbError::Timeout);
        }
        if *self.simulate_write_error.lock().unwrap() {
            return Err(UsbError::WriteError("Simulated write error".to_string()));
        }

        // データを記録
        self.sent_data.lock().unwrap().push(data.to_vec());
        Ok(data.len())
    }

    fn read(&mut self, buffer: &mut [u8], _timeout_ms: u32) -> UsbResult<usize> {
        // エラーシミュレーション
        if *self.simulate_timeout.lock().unwrap() {
            return Err(UsbError::Timeout);
        }
        if *self.simulate_read_error.lock().unwrap() {
            return Err(UsbError::Other("Simulated read error".to_string()));
        }

        // キューからデータを取り出す
        let mut queue = self.read_data_queue.lock().unwrap();
        if let Some(data) = queue.first() {
            let len = data.len().min(buffer.len());
            buffer[..len].copy_from_slice(&data[..len]);
            queue.remove(0); // 取り出したデータを削除
            Ok(len)
        } else {
            // データがない場合はタイムアウト
            Err(UsbError::Timeout)
        }
    }

    fn read_command(&mut self, timeout_ms: u32) -> UsbResult<Option<String>> {
        // コマンドキューから取り出す
        {
            let mut queue = self.command_queue.lock().unwrap();
            if let Some(cmd) = queue.first() {
                let result = cmd.clone();
                queue.remove(0);
                return Ok(Some(result));
            }
        }
        
        // キューが空の場合、readを呼び出してタイムアウト処理を行う
        let mut buffer = [0u8; 256];
        match self.read(&mut buffer, timeout_ms) {
            Ok(bytes_read) if bytes_read > 0 => {
                let command_str = String::from_utf8_lossy(&buffer[..bytes_read])
                    .trim()
                    .to_string();
                if !command_str.is_empty() {
                    Ok(Some(command_str))
                } else {
                    Ok(None)
                }
            }
            Ok(_) => Ok(None),
            Err(UsbError::Timeout) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn send_frame(&mut self, data: &[u8], _mac_str: &str) -> UsbResult<usize> {
        // Mockでは簡略化: チャンキングなしで全データを送信
        self.write(data, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_write() {
        let mut mock = MockUsbCdc::new();
        let test_data = b"Hello, USB!";

        let result = mock.write(test_data, 100);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), test_data.len());

        let sent = mock.get_sent_data();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0], test_data);
    }

    #[test]
    fn test_mock_write_error() {
        let mut mock = MockUsbCdc::new();
        mock.set_write_error(true);

        let result = mock.write(b"test", 100);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), UsbError::WriteError(_)));
    }

    #[test]
    fn test_mock_read() {
        let mut mock = MockUsbCdc::new();
        let test_data = b"Test data";
        mock.queue_read_data(test_data.to_vec());

        let mut buffer = [0u8; 128];
        let result = mock.read(&mut buffer, 100);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), test_data.len());
        assert_eq!(&buffer[..test_data.len()], test_data);
    }

    #[test]
    fn test_mock_read_timeout() {
        let mut mock = MockUsbCdc::new();
        // キューが空の場合はタイムアウト

        let mut buffer = [0u8; 128];
        let result = mock.read(&mut buffer, 100);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), UsbError::Timeout));
    }

    #[test]
    fn test_mock_read_command() {
        let mut mock = MockUsbCdc::new();
        mock.queue_command("SLEEP 300".to_string());

        let result = mock.read_command(100);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("SLEEP 300".to_string()));
    }

    #[test]
    fn test_mock_send_frame() {
        let mut mock = MockUsbCdc::new();
        let test_frame = vec![0xAA, 0xBB, 0xCC, 0xDD, 0x01, 0x02, 0x03];

        let result = mock.send_frame(&test_frame, "00:11:22:33:44:55");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), test_frame.len());

        let sent = mock.get_sent_data();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0], test_frame);
    }

    #[test]
    fn test_mock_multiple_operations() {
        let mut mock = MockUsbCdc::new();

        // 複数の書き込み
        mock.write(b"data1", 100).unwrap();
        mock.write(b"data2", 100).unwrap();
        mock.write(b"data3", 100).unwrap();

        let sent = mock.get_sent_data();
        assert_eq!(sent.len(), 3);
        assert_eq!(sent[0], b"data1");
        assert_eq!(sent[1], b"data2");
        assert_eq!(sent[2], b"data3");

        // クリア
        mock.clear_sent_data();
        assert_eq!(mock.get_sent_data().len(), 0);
    }
}
