pub const LOW_VOLTAGE_THRESHOLD_PERCENT: u8 = 8;
pub const INVALID_VOLTAGE_PERCENT: u8 = 255;

pub fn should_capture_image(voltage_percent: u8) -> bool {
    voltage_percent > LOW_VOLTAGE_THRESHOLD_PERCENT && voltage_percent < INVALID_VOLTAGE_PERCENT
}
