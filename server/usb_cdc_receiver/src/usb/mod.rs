#[cfg(feature = "esp")]
pub mod cdc;

// Mock実装（テストとnon-espビルドで使用可能）
#[cfg(not(feature = "esp"))]
pub mod mock;

/// USB通信での結果の型
pub type UsbResult<T> = Result<T, UsbError>;

/// USBコマンド読み取り用のバッファサイズ
pub const COMMAND_BUFFER_SIZE: usize = 256;

/// USB通信のエラーを表す列挙型
#[derive(Debug, Clone, PartialEq)]
pub enum UsbError {
    /// 初期化エラー
    InitError(String),
    /// 書き込みエラー
    WriteError(String),
    /// タイムアウトエラー
    Timeout,
    /// その他のエラー
    Other(String),
}

impl std::fmt::Display for UsbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UsbError::InitError(msg) => write!(f, "USB initialization error: {}", msg),
            UsbError::WriteError(msg) => write!(f, "USB write error: {}", msg),
            UsbError::Timeout => write!(f, "USB operation timed out"),
            UsbError::Other(msg) => write!(f, "USB error: {}", msg),
        }
    }
}

impl std::error::Error for UsbError {}

#[cfg(feature = "esp")]
impl From<esp_idf_svc::sys::EspError> for UsbError {
    fn from(error: esp_idf_svc::sys::EspError) -> Self {
        if error.code() == esp_idf_svc::sys::ESP_ERR_TIMEOUT {
            UsbError::Timeout
        } else {
            UsbError::Other(format!("ESP-IDF error: {}", error))
        }
    }
}

/// USB通信インターフェースのトレイト
/// 
/// このトレイトを実装することで、実機用とテスト用(Mock)の
/// 実装を切り替えることができます。
pub trait UsbInterface {
    /// データをUSB経由で書き込む
    fn write(&mut self, data: &[u8], timeout_ms: u32) -> UsbResult<usize>;

    /// USB経由でデータを読み取る
    fn read(&mut self, buffer: &mut [u8], timeout_ms: u32) -> UsbResult<usize>;

    /// USBからコマンドを読み取り、解析する
    fn read_command(&mut self, timeout_ms: u32) -> UsbResult<Option<String>>;

    /// フレームデータをUSB経由で送信する
    fn send_frame(&mut self, data: &[u8], mac_str: &str) -> UsbResult<usize>;
}
