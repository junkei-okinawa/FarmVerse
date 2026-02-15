pub const LOW_VOLTAGE_THRESHOLD_PERCENT: u8 = 8;
pub const INVALID_VOLTAGE_PERCENT: u8 = 255;

pub fn should_capture_image(voltage_percent: u8) -> bool {
    voltage_percent > LOW_VOLTAGE_THRESHOLD_PERCENT && voltage_percent < INVALID_VOLTAGE_PERCENT
}

pub fn should_capture_image_with_overrides(
    voltage_percent: u8,
    force_camera_test: bool,
    bypass_voltage_threshold: bool,
) -> bool {
    if force_camera_test {
        return true;
    }
    if voltage_percent >= INVALID_VOLTAGE_PERCENT {
        return false;
    }
    bypass_voltage_threshold || should_capture_image(voltage_percent)
}
