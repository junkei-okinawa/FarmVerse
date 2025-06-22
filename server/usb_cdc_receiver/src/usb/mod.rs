pub mod cdc;

/// USB通信での結果の型
pub type UsbResult<T> = Result<T, UsbError>;

/// USB通信のエラーを表す列挙型
#[derive(Debug)]
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

impl From<esp_idf_svc::sys::EspError> for UsbError {
    fn from(error: esp_idf_svc::sys::EspError) -> Self {
        if error.code() == esp_idf_svc::sys::ESP_ERR_TIMEOUT {
            UsbError::Timeout
        } else {
            UsbError::Other(format!("ESP-IDF error: {}", error))
        }
    }
}
