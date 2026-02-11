use crate::mac_address::MacAddress;

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    MissingReceiverMac,
    InvalidReceiverMac(String),
    InvalidCameraWarmupFrames(u8),
    InvalidTargetMinuteLastDigit(u8),
    InvalidTargetSecondLastDigit(u8),
    MissingWifiSsid,
}

pub fn parse_receiver_mac(receiver_mac: &str) -> Result<MacAddress, ValidationError> {
    if receiver_mac == "11:22:33:44:55:66" || receiver_mac.is_empty() {
        return Err(ValidationError::MissingReceiverMac);
    }

    MacAddress::from_str(receiver_mac)
        .map_err(|_| ValidationError::InvalidReceiverMac(receiver_mac.to_string()))
}

pub fn parse_camera_warmup_frames(value: u8) -> Result<Option<u8>, ValidationError> {
    if !(value <= 10 || value == 255) {
        return Err(ValidationError::InvalidCameraWarmupFrames(value));
    }

    if value == 255 {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

pub fn parse_target_minute_last_digit(value: u8) -> Result<Option<u8>, ValidationError> {
    if value <= 9 {
        Ok(Some(value))
    } else if value == 255 {
        Ok(None)
    } else {
        Err(ValidationError::InvalidTargetMinuteLastDigit(value))
    }
}

pub fn parse_target_second_tens_digit(value: u8) -> Result<Option<u8>, ValidationError> {
    if value <= 5 {
        Ok(Some(value))
    } else if value == 255 {
        Ok(None)
    } else {
        Err(ValidationError::InvalidTargetSecondLastDigit(value))
    }
}

pub fn validate_wifi_ssid(ssid: &str) -> Result<(), ValidationError> {
    if ssid.is_empty() {
        Err(ValidationError::MissingWifiSsid)
    } else {
        Ok(())
    }
}
