pub fn voltage_to_percentage(voltage_mv: f32, min_mv: f32, max_mv: f32) -> u8 {
    let range_mv = max_mv - min_mv;
    let percentage = if range_mv <= 0.0 {
        0.0
    } else {
        ((voltage_mv - min_mv) / range_mv * 100.0)
            .max(0.0)
            .min(100.0)
    };
    percentage.round() as u8
}

pub fn resolve_sleep_duration_seconds(received_seconds: Option<u32>, default_seconds: u64) -> u64 {
    match received_seconds {
        Some(seconds) if seconds > 0 => seconds as u64,
        _ => default_seconds,
    }
}
