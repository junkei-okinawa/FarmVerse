#[derive(Debug, Clone)]
pub struct Sample {
    pub timestamp_ms: i64,
    pub bus_raw: u16,
    pub bus_voltage_v: f32,
    pub current_raw: i16,
    pub current_ma: f32,
    pub power_raw: u16,
    pub power_mw: f32,
    pub target: String,
    pub sensor_online: bool,
    pub quality: String,
    pub status_message: String,
}

impl Sample {
    pub fn empty(target: String) -> Self {
        Self {
            timestamp_ms: 0,
            bus_raw: 0,
            bus_voltage_v: 0.0,
            current_raw: 0,
            current_ma: 0.0,
            power_raw: 0,
            power_mw: 0.0,
            target,
            sensor_online: false,
            quality: "offline".to_string(),
            status_message: "Initializing".to_string(),
        }
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\"timestamp_ms\":{},\"bus_raw\":{},\"bus_voltage_v\":{:.6},\"current_raw\":{},\"current_ma\":{:.3},\"power_raw\":{},\"power_mw\":{:.3},\"target\":\"{}\",\"sensor_online\":{},\"quality\":\"{}\",\"status_message\":\"{}\"}}",
            self.timestamp_ms,
            self.bus_raw,
            self.bus_voltage_v,
            self.current_raw,
            self.current_ma,
            self.power_raw,
            self.power_mw,
            escape_json(&self.target),
            if self.sensor_online { "true" } else { "false" },
            escape_json(&self.quality),
            escape_json(&self.status_message)
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GuardConfig {
    pub enabled: bool,
    pub bus_voltage_min_v: f32,
    pub bus_voltage_max_v: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SampleQuality {
    Ok,
    Invalid,
}

pub fn evaluate_quality(bus_voltage_v: f32, guard: GuardConfig) -> (SampleQuality, String) {
    if guard.enabled
        && (bus_voltage_v < guard.bus_voltage_min_v || bus_voltage_v > guard.bus_voltage_max_v)
    {
        return (
            SampleQuality::Invalid,
            format!(
                "Bus voltage out of range: {:.3}V (expected {:.3}..{:.3}V)",
                bus_voltage_v, guard.bus_voltage_min_v, guard.bus_voltage_max_v
            ),
        );
    }
    (SampleQuality::Ok, "OK".to_string())
}

pub fn quality_label(q: &SampleQuality) -> &'static str {
    match q {
        SampleQuality::Ok => "ok",
        SampleQuality::Invalid => "invalid",
    }
}

pub fn should_store(sensor_online: bool, quality: &str) -> bool {
    sensor_online && quality == "ok"
}

pub fn resolve_ina226_address(detected: &[u8], preferred: u8) -> Result<u8, String> {
    if detected.contains(&preferred) {
        return Ok(preferred);
    }

    if let Some(addr) = detected
        .iter()
        .copied()
        .find(|addr| (0x40..=0x4F).contains(addr))
    {
        return Ok(addr);
    }

    Err(format!(
        "no INA226-like address found. Check wiring/SDA-SCL/power. detected=[{}]",
        format_addrs(detected)
    ))
}

pub fn format_addrs(addrs: &[u8]) -> String {
    addrs
        .iter()
        .map(|a| format!("0x{a:02X}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn escape_json(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::{
        evaluate_quality, format_addrs, quality_label, resolve_ina226_address, should_store,
        GuardConfig, Sample, SampleQuality,
    };

    #[test]
    fn to_json_includes_status_fields() {
        let sample = Sample {
            timestamp_ms: 123,
            bus_raw: 4190,
            bus_voltage_v: 5.2375,
            current_raw: 237,
            current_ma: 23.7,
            power_raw: 49,
            power_mw: 122.5,
            target: "M5StackUnitCam".to_string(),
            sensor_online: true,
            quality: "ok".to_string(),
            status_message: "OK".to_string(),
        };

        let json = sample.to_json();
        assert!(json.contains("\"sensor_online\":true"));
        assert!(json.contains("\"quality\":\"ok\""));
        assert!(json.contains("\"status_message\":\"OK\""));
    }

    #[test]
    fn to_json_escapes_quotes_and_backslashes() {
        let sample = Sample {
            timestamp_ms: 1,
            bus_raw: 0,
            bus_voltage_v: 0.0,
            current_raw: 0,
            current_ma: 0.0,
            power_raw: 0,
            power_mw: 0.0,
            target: "A\\B\"C".to_string(),
            sensor_online: false,
            quality: "offline".to_string(),
            status_message: "Sensor \"Offline\"".to_string(),
        };

        let json = sample.to_json();
        assert!(json.contains("\"target\":\"A\\\\B\\\"C\""));
        assert!(json.contains("\"status_message\":\"Sensor \\\"Offline\\\"\""));
    }

    #[test]
    fn evaluate_quality_returns_invalid_when_bus_out_of_range() {
        let guard = GuardConfig {
            enabled: true,
            bus_voltage_min_v: 4.8,
            bus_voltage_max_v: 5.4,
        };
        let (q, msg) = evaluate_quality(0.0, guard);
        assert_eq!(q, SampleQuality::Invalid);
        assert!(msg.contains("out of range"));
    }

    #[test]
    fn evaluate_quality_returns_ok_when_guard_disabled() {
        let guard = GuardConfig {
            enabled: false,
            bus_voltage_min_v: 4.8,
            bus_voltage_max_v: 5.4,
        };
        let (q, msg) = evaluate_quality(0.0, guard);
        assert_eq!(q, SampleQuality::Ok);
        assert_eq!(msg, "OK");
    }

    #[test]
    fn should_store_matches_expected_rule() {
        assert!(should_store(true, "ok"));
        assert!(!should_store(false, "ok"));
        assert!(!should_store(true, "invalid"));
        assert!(!should_store(true, "offline"));
    }

    #[test]
    fn quality_label_maps_enum() {
        assert_eq!(quality_label(&SampleQuality::Ok), "ok");
        assert_eq!(quality_label(&SampleQuality::Invalid), "invalid");
    }

    #[test]
    fn format_addrs_works() {
        let got = format_addrs(&[0x40, 0x41, 0x4F]);
        assert_eq!(got, "0x40,0x41,0x4F");
    }

    #[test]
    fn resolve_prefers_explicit_address() {
        let got = resolve_ina226_address(&[0x40, 0x41], 0x41).expect("must resolve");
        assert_eq!(got, 0x41);
    }

    #[test]
    fn resolve_falls_back_to_ina_range() {
        let got = resolve_ina226_address(&[0x20, 0x44], 0x40).expect("must fallback");
        assert_eq!(got, 0x44);
    }

    #[test]
    fn resolve_errors_when_no_candidate() {
        let err = resolve_ina226_address(&[0x20, 0x21], 0x40).expect_err("must error");
        assert!(err.contains("no INA226-like address found"));
    }
}
