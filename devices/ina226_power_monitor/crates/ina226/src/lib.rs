#![forbid(unsafe_code)]

use embedded_hal::i2c::I2c;

const REG_CONFIGURATION: u8 = 0x00;
const REG_SHUNT_VOLTAGE: u8 = 0x01;
const REG_BUS_VOLTAGE: u8 = 0x02;
const REG_POWER: u8 = 0x03;
const REG_CURRENT: u8 = 0x04;
const REG_CALIBRATION: u8 = 0x05;
const REG_MANUFACTURER_ID: u8 = 0xFE;
const REG_DIE_ID: u8 = 0xFF;
const CONFIG_RESET_BIT: u16 = 1 << 15;
const CONFIG_RESERVED_BIT: u16 = 1 << 14;
const CONFIG_RESET_DEFAULT: u16 = 0x4127;
pub const CONFIG_RESET_DEFAULT_RAW: u16 = CONFIG_RESET_DEFAULT;

#[derive(Debug)]
pub enum Error<E> {
    I2c(E),
}

#[derive(Debug, Clone, Copy)]
pub enum Averaging {
    Avg1 = 0,
    Avg4 = 1,
    Avg16 = 2,
    Avg64 = 3,
    Avg128 = 4,
    Avg256 = 5,
    Avg512 = 6,
    Avg1024 = 7,
}

#[derive(Debug, Clone, Copy)]
pub enum ConversionTime {
    Us140 = 0,
    Us204 = 1,
    Us332 = 2,
    Us588 = 3,
    Us1100 = 4,
    Us2116 = 5,
    Us4156 = 6,
    Us8244 = 7,
}

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    PowerDown = 0,
    ShuntTriggered = 1,
    BusTriggered = 2,
    ShuntAndBusTriggered = 3,
    AdcOff = 4,
    ShuntContinuous = 5,
    BusContinuous = 6,
    ShuntAndBusContinuous = 7,
}

#[derive(Debug, Clone, Copy)]
pub struct Configuration {
    pub averaging: Averaging,
    pub bus_conversion_time: ConversionTime,
    pub shunt_conversion_time: ConversionTime,
    pub mode: Mode,
}

impl Default for Configuration {
    fn default() -> Self {
        Self {
            averaging: Averaging::Avg16,
            bus_conversion_time: ConversionTime::Us1100,
            shunt_conversion_time: ConversionTime::Us1100,
            mode: Mode::ShuntAndBusContinuous,
        }
    }
}

impl Configuration {
    pub fn raw(self) -> u16 {
        self.to_raw()
    }

    fn to_raw(self) -> u16 {
        CONFIG_RESERVED_BIT
            | ((self.averaging as u16) << 9)
            | ((self.bus_conversion_time as u16) << 6)
            | ((self.shunt_conversion_time as u16) << 3)
            | (self.mode as u16)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Measurements {
    pub bus_raw: u16,
    pub current_raw: i16,
    pub power_raw: u16,
    pub bus_voltage_v: f32,
    pub current_ma: f32,
    pub power_mw: f32,
}

pub struct Ina226<I2C> {
    i2c: I2C,
    address: u8,
    current_lsb_a: f32,
    power_lsb_w: f32,
}

impl<I2C, E> Ina226<I2C>
where
    I2C: I2c<Error = E>,
{
    pub fn new(i2c: I2C, address: u8, shunt_ohm: f32) -> Result<Self, Error<E>> {
        let current_lsb_a = 0.0001_f32;
        let calibration = (0.00512_f32 / (current_lsb_a * shunt_ohm)) as u16;

        let mut dev = Self {
            i2c,
            address,
            current_lsb_a,
            power_lsb_w: current_lsb_a * 25.0,
        };

        // Keep initialization flow close to known-good ESP-IDF component behavior.
        dev.reset()?;
        dev.write_u16(REG_CALIBRATION, calibration)?;
        Ok(dev)
    }

    pub fn reset(&mut self) -> Result<(), Error<E>> {
        self.write_u16(REG_CONFIGURATION, CONFIG_RESET_BIT)
    }

    pub fn set_configuration(&mut self, config: Configuration) -> Result<(), Error<E>> {
        self.write_u16(REG_CONFIGURATION, config.to_raw())
    }

    pub fn read_manufacturer_id(&mut self) -> Result<u16, Error<E>> {
        self.read_u16(REG_MANUFACTURER_ID)
    }

    pub fn read_die_id(&mut self) -> Result<u16, Error<E>> {
        self.read_u16(REG_DIE_ID)
    }

    pub fn read_measurements(&mut self) -> Result<Measurements, Error<E>> {
        let bus_raw = self.read_u16(REG_BUS_VOLTAGE)?;
        let current_raw = self.read_i16(REG_CURRENT)?;
        let power_raw = self.read_u16(REG_POWER)?;

        let bus_voltage_v = (bus_raw as f32) * 0.00125;
        let current_ma = (current_raw as f32) * self.current_lsb_a * 1000.0;
        let power_mw = (power_raw as f32) * self.power_lsb_w * 1000.0;

        Ok(Measurements {
            bus_raw,
            current_raw,
            power_raw,
            bus_voltage_v,
            current_ma,
            power_mw,
        })
    }

    pub fn read_shunt_voltage_mv(&mut self) -> Result<f32, Error<E>> {
        let shunt_raw = self.read_i16(REG_SHUNT_VOLTAGE)?;
        Ok((shunt_raw as f32) * 0.0025)
    }

    pub fn read_configuration_raw(&mut self) -> Result<u16, Error<E>> {
        self.read_u16(REG_CONFIGURATION)
    }

    pub fn read_calibration_raw(&mut self) -> Result<u16, Error<E>> {
        self.read_u16(REG_CALIBRATION)
    }

    pub fn read_bus_voltage_raw(&mut self) -> Result<u16, Error<E>> {
        self.read_u16(REG_BUS_VOLTAGE)
    }

    pub fn read_shunt_voltage_raw(&mut self) -> Result<i16, Error<E>> {
        self.read_i16(REG_SHUNT_VOLTAGE)
    }

    pub fn read_current_raw(&mut self) -> Result<i16, Error<E>> {
        self.read_i16(REG_CURRENT)
    }

    pub fn read_power_raw(&mut self) -> Result<u16, Error<E>> {
        self.read_u16(REG_POWER)
    }

    fn write_u16(&mut self, register: u8, value: u16) -> Result<(), Error<E>> {
        let buf = [register, (value >> 8) as u8, (value & 0xFF) as u8];
        self.i2c.write(self.address, &buf).map_err(Error::I2c)
    }

    fn read_u16(&mut self, register: u8) -> Result<u16, Error<E>> {
        let mut buf = [0_u8; 2];
        self.i2c
            .write_read(self.address, &[register], &mut buf)
            .map_err(Error::I2c)?;
        Ok(u16::from_be_bytes(buf))
    }

    fn read_i16(&mut self, register: u8) -> Result<i16, Error<E>> {
        Ok(self.read_u16(register)? as i16)
    }
}
