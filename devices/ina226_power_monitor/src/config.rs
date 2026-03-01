#[toml_cfg::toml_config]
pub struct Config {
    #[default("ina226-monitor")]
    ap_ssid: &'static str,

    #[default("")]
    ap_password: &'static str,

    #[default(1)]
    ap_channel: u8,

    #[default("M5StackUnitCam")]
    measurement_target: &'static str,

    #[default(0x40)]
    ina226_addr: u8,

    #[default(0.1)]
    shunt_resistor_ohm: f32,

    #[default(100_000)]
    i2c_frequency_hz: u32,

    #[default(200)]
    sample_interval_ms: u32,

    #[default(true)]
    invalid_guard_enabled: bool,

    #[default(0.1)]
    bus_voltage_min_v: f32,

    #[default(6.0)]
    bus_voltage_max_v: f32,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub ap_ssid: String,
    pub ap_password: String,
    pub ap_channel: u8,
    pub measurement_target: String,
    pub ina226_addr: u8,
    pub shunt_resistor_ohm: f32,
    pub i2c_frequency_hz: u32,
    pub sample_interval_ms: u32,
    pub invalid_guard_enabled: bool,
    pub bus_voltage_min_v: f32,
    pub bus_voltage_max_v: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("AP SSID is empty")]
    EmptyApSsid,
    #[error("AP password must be >= 8 chars or empty")]
    InvalidApPassword,
    #[error("I2C frequency out of range: {0}")]
    InvalidI2cFrequency(u32),
    #[error("sample_interval_ms must be > 0")]
    InvalidSampleInterval,
    #[error("shunt_resistor_ohm must be > 0")]
    InvalidShunt,
    #[error("bus voltage guard range is invalid: min={min}, max={max}")]
    InvalidBusVoltageGuard { min: f32, max: f32 },
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let cfg = CONFIG;

        let ap_ssid = cfg.ap_ssid.trim().to_string();
        if ap_ssid.is_empty() {
            return Err(ConfigError::EmptyApSsid);
        }

        let ap_password = cfg.ap_password.to_string();
        if !ap_password.is_empty() && ap_password.len() < 8 {
            return Err(ConfigError::InvalidApPassword);
        }

        if !(10_000..=1_000_000).contains(&cfg.i2c_frequency_hz) {
            return Err(ConfigError::InvalidI2cFrequency(cfg.i2c_frequency_hz));
        }

        if cfg.sample_interval_ms == 0 {
            return Err(ConfigError::InvalidSampleInterval);
        }

        if cfg.shunt_resistor_ohm <= 0.0 {
            return Err(ConfigError::InvalidShunt);
        }

        if cfg.bus_voltage_min_v < 0.0 || cfg.bus_voltage_max_v <= cfg.bus_voltage_min_v {
            return Err(ConfigError::InvalidBusVoltageGuard {
                min: cfg.bus_voltage_min_v,
                max: cfg.bus_voltage_max_v,
            });
        }

        Ok(Self {
            ap_ssid,
            ap_password,
            ap_channel: cfg.ap_channel.clamp(1, 13),
            measurement_target: cfg.measurement_target.to_string(),
            ina226_addr: cfg.ina226_addr,
            shunt_resistor_ohm: cfg.shunt_resistor_ohm,
            i2c_frequency_hz: cfg.i2c_frequency_hz,
            sample_interval_ms: cfg.sample_interval_ms,
            invalid_guard_enabled: cfg.invalid_guard_enabled,
            bus_voltage_min_v: cfg.bus_voltage_min_v,
            bus_voltage_max_v: cfg.bus_voltage_max_v,
        })
    }
}
