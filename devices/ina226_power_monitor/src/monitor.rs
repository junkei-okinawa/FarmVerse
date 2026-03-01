use std::fmt::Debug;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use embedded_hal::i2c::I2c;
use esp_idf_hal::delay::{FreeRtos, BLOCK};
use esp_idf_hal::i2c::I2cDriver;
use ina226::{Averaging, Configuration, ConversionTime, Ina226, Mode};
use log::{error, warn};

use crate::model::Sample;
use crate::monitor_core::{
    evaluate_quality, format_addrs as core_format_addrs, quality_label,
    resolve_ina226_address as core_resolve_ina226_address, GuardConfig,
};

pub fn scan_i2c_bus(i2c: &mut I2cDriver<'_>) -> Vec<u8> {
    let mut found = Vec::new();
    for addr in 0x03_u8..=0x77_u8 {
        if i2c.write(addr, &[], BLOCK).is_ok() {
            found.push(addr);
        }
    }
    found
}

pub fn resolve_ina226_address(detected: &[u8], preferred: u8) -> anyhow::Result<u8> {
    match core_resolve_ina226_address(detected, preferred) {
        Ok(addr) => {
            if addr != preferred {
        warn!("INA226 preferred 0x{preferred:02X} not found; fallback to detected 0x{addr:02X}");
            }
            Ok(addr)
        }
        Err(msg) => Err(anyhow::anyhow!("{msg}")),
    }
}

pub fn format_addrs(addrs: &[u8]) -> String {
    core_format_addrs(addrs)
}

pub fn spawn_measurement_task<I2C, E>(
    mut ina: Ina226<I2C>,
    latest: Arc<Mutex<Sample>>,
    target: String,
    interval_ms: u32,
    guard: GuardConfig,
) -> anyhow::Result<std::thread::JoinHandle<()>>
where
    I2C: I2c<Error = E> + Send + 'static,
    E: Debug + Send + 'static,
{
    std::thread::Builder::new()
        .name("ina226-loop".to_string())
        .stack_size(16 * 1024)
        .spawn(move || {
            let mut consecutive_failures: u32 = 0;
            let mut initialized = false;
            let measurement_cfg = Configuration {
                averaging: Averaging::Avg16,
                bus_conversion_time: ConversionTime::Us1100,
                shunt_conversion_time: ConversionTime::Us1100,
                mode: Mode::ShuntAndBusContinuous,
            };

            loop {
                if !initialized {
                    match ina.init().and_then(|_| ina.set_configuration(measurement_cfg)) {
                        Ok(()) => {
                            initialized = true;
                            consecutive_failures = 0;
                        }
                        Err(e) => {
                            consecutive_failures = consecutive_failures.saturating_add(1);
                            warn!("INA226 init retry failed: {:?}", e);
                            if let Ok(mut guard) = latest.lock() {
                                *guard = Sample {
                                    timestamp_ms: unsafe { esp_idf_sys::esp_timer_get_time() / 1000 },
                                    bus_raw: 0,
                                    bus_voltage_v: 0.0,
                                    current_raw: 0,
                                    current_ma: 0.0,
                                    power_raw: 0,
                                    power_mw: 0.0,
                                    target: target.clone(),
                                    sensor_online: false,
                                    quality: "offline".to_string(),
                                    status_message: format!(
                                        "Sensor Offline (init failures: {})",
                                        consecutive_failures
                                    ),
                                };
                            }
                            FreeRtos::delay_ms(interval_ms);
                            continue;
                        }
                    }
                }

                match ina.read_measurements() {
                    Ok(m) => {
                        consecutive_failures = 0;
                        let (quality, status_message) = evaluate_quality(m.bus_voltage_v, guard);

                        let sample = Sample {
                            timestamp_ms: unsafe { esp_idf_sys::esp_timer_get_time() / 1000 },
                            bus_raw: m.bus_raw,
                            bus_voltage_v: m.bus_voltage_v,
                            current_raw: m.current_raw,
                            current_ma: m.current_ma,
                            power_raw: m.power_raw,
                            power_mw: m.power_mw,
                            target: target.clone(),
                            sensor_online: true,
                            quality: quality_label(&quality).to_string(),
                            status_message,
                        };

                        println!(
                            "{},{},{:.6},{},{:.3},{},{:.3},{}",
                            sample.timestamp_ms,
                            sample.bus_raw,
                            sample.bus_voltage_v,
                            sample.current_raw,
                            sample.current_ma,
                            sample.power_raw,
                            sample.power_mw,
                            sample.target
                        );

                        if let Ok(mut guard) = latest.lock() {
                            *guard = sample;
                        }
                    }
                    Err(e) => {
                        initialized = false;
                        consecutive_failures = consecutive_failures.saturating_add(1);
                        error!("measurement read failed: {:?}", e);
                        warn!("retrying after delay");
                        if let Ok(mut guard) = latest.lock() {
                            *guard = Sample {
                                timestamp_ms: unsafe { esp_idf_sys::esp_timer_get_time() / 1000 },
                                bus_raw: 0,
                                bus_voltage_v: 0.0,
                                current_raw: 0,
                                current_ma: 0.0,
                                power_raw: 0,
                                power_mw: 0.0,
                                target: target.clone(),
                                sensor_online: false,
                                quality: "offline".to_string(),
                                status_message: format!(
                                    "Sensor Offline (read failures: {})",
                                    consecutive_failures
                                ),
                            };
                        }
                    }
                }

                FreeRtos::delay_ms(interval_ms);
            }
        })
        .context("failed to spawn INA226 measurement task")
}
