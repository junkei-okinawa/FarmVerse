use std::sync::{Arc, Mutex};

use anyhow::Context;
use esp_idf_hal::delay::FreeRtos;
use esp_idf_hal::i2c::{I2cConfig, I2cDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::units::Hertz;
use ina226::{Averaging, Configuration, ConversionTime, Ina226, Mode, CONFIG_RESET_DEFAULT_RAW};
use log::{info, warn};

mod config;
mod model;
mod monitor;
mod monitor_core;
mod web;

use config::AppConfig;
use model::Sample;
use monitor_core::GuardConfig;

fn main() -> anyhow::Result<()> {
    esp_idf_sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let app_config = AppConfig::load().context("failed to load cfg.toml")?;

    info!(
        "INA226 power monitor boot: SDA=GPIO5 (D4), SCL=GPIO6 (D5), addr=0x{:02X}, I2C={}Hz",
        app_config.ina226_addr, app_config.i2c_frequency_hz
    );

    let peripherals = Peripherals::take().context("failed to take peripherals")?;
    let pins = peripherals.pins;
    let modem = peripherals.modem;

    let i2c_config = I2cConfig::new()
        .baudrate(Hertz(app_config.i2c_frequency_hz))
        .timeout(std::time::Duration::from_millis(50).into());

    let mut i2c = I2cDriver::new(peripherals.i2c0, pins.gpio5, pins.gpio6, &i2c_config)
        .context("failed to initialize I2C on GPIO5/GPIO6")?;

    let detected = monitor::scan_i2c_bus(&mut i2c);
    if detected.is_empty() {
        warn!("no I2C devices found on bus");
    } else {
        info!(
            "detected I2C addresses: {}",
            monitor::format_addrs(&detected)
        );
    }

    let ina_addr = monitor::resolve_ina226_address(&detected, app_config.ina226_addr)?;
    info!("using INA226 address: 0x{ina_addr:02X}");

    let mut ina = Ina226::new(i2c, ina_addr, app_config.shunt_resistor_ohm)
        .map_err(|e| anyhow::anyhow!("failed to initialize INA226: {:?}", e))?;

    let reset_cfg_raw = ina
        .read_configuration_raw()
        .map_err(|e| anyhow::anyhow!("failed to read INA226 config after reset: {:?}", e))?;
    info!(
        "INA226 config after reset: raw=0x{reset_cfg_raw:04X}, expected_default=0x{:04X}",
        CONFIG_RESET_DEFAULT_RAW
    );

    let config = Configuration {
        averaging: Averaging::Avg16,
        bus_conversion_time: ConversionTime::Us1100,
        shunt_conversion_time: ConversionTime::Us1100,
        mode: Mode::ShuntAndBusContinuous,
    };
    ina.set_configuration(config)
        .map_err(|e| anyhow::anyhow!("failed to apply INA226 configuration: {:?}", e))?;

    let manufacturer_id = ina
        .read_manufacturer_id()
        .map_err(|e| anyhow::anyhow!("failed to read INA226 manufacturer ID: {:?}", e))?;
    let die_id = ina
        .read_die_id()
        .map_err(|e| anyhow::anyhow!("failed to read INA226 die ID: {:?}", e))?;

    info!(
        "INA226 communication OK: addr=0x{ina_addr:02X}, manufacturer_id=0x{manufacturer_id:04X}, die_id=0x{die_id:04X}"
    );
    let cfg_raw = ina
        .read_configuration_raw()
        .map_err(|e| anyhow::anyhow!("failed to read INA226 config raw: {:?}", e))?;
    let cal_raw = ina
        .read_calibration_raw()
        .map_err(|e| anyhow::anyhow!("failed to read INA226 calibration raw: {:?}", e))?;
    let shunt_raw = ina
        .read_shunt_voltage_raw()
        .map_err(|e| anyhow::anyhow!("failed to read INA226 shunt raw: {:?}", e))?;
    let bus_raw = ina
        .read_bus_voltage_raw()
        .map_err(|e| anyhow::anyhow!("failed to read INA226 bus raw: {:?}", e))?;
    info!(
        "INA226 register snapshot: config=0x{cfg_raw:04X} (expected=0x{:04X}), calibration=0x{cal_raw:04X}, shunt_raw={}, bus_raw={}",
        config.raw(),
        shunt_raw,
        bus_raw
    );

    println!("timestamp_ms,bus_raw,bus_voltage_v,current_raw,current_ma,power_raw,power_mw,target");

    let latest = Arc::new(Mutex::new(Sample::empty(
        app_config.measurement_target.clone(),
    )));
    let _worker = monitor::spawn_measurement_task(
        ina,
        Arc::clone(&latest),
        app_config.measurement_target.clone(),
        app_config.sample_interval_ms,
        GuardConfig {
            enabled: app_config.invalid_guard_enabled,
            bus_voltage_min_v: app_config.bus_voltage_min_v,
            bus_voltage_max_v: app_config.bus_voltage_max_v,
        },
    )?;

    let _web_runtime = web::start(modem, Arc::clone(&latest), &app_config)?;

    loop {
        FreeRtos::delay_ms(1000);
    }
}
