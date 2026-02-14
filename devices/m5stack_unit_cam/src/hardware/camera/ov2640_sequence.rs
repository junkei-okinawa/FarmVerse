#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegWrite {
    pub reg: i32,
    pub mask: i32,
    pub value: i32,
}

// Common OV2640 registers used in current low-power flow.
const REG_BANK_SEL: i32 = 0xFF;
const REG_DSP_R_DVP_SP: i32 = 0xD3;
const REG_SENSOR_CLKRC: i32 = 0x11;
const REG_SENSOR_COM7: i32 = 0x12;

const BANK_DSP: i32 = 0x00;
const BANK_SENSOR: i32 = 0x01;

pub fn standby_sequence() -> [RegWrite; 4] {
    [
        // BANK_SEL = DSP
        RegWrite {
            reg: REG_BANK_SEL,
            mask: 0xFF,
            value: BANK_DSP,
        },
        // R_DVP_SP clear
        RegWrite {
            reg: REG_DSP_R_DVP_SP,
            mask: 0xFF,
            value: 0x00,
        },
        // BANK_SEL = SENSOR
        RegWrite {
            reg: REG_BANK_SEL,
            mask: 0xFF,
            value: BANK_SENSOR,
        },
        // COM7 bit4 sleep + CLKRC max divider encoded as two writes in controller
        // Keep COM7 write as part of sequence; CLKRC is explicit below for testability.
        RegWrite {
            reg: REG_SENSOR_COM7,
            mask: 0x10,
            value: 0x10,
        },
    ]
}

pub fn standby_clkrc_write() -> RegWrite {
    RegWrite {
        reg: REG_SENSOR_CLKRC,
        mask: 0x3F,
        value: 0x3F,
    }
}

pub fn resume_sequence() -> [RegWrite; 3] {
    [
        // BANK_SEL = SENSOR
        RegWrite {
            reg: REG_BANK_SEL,
            mask: 0xFF,
            value: BANK_SENSOR,
        },
        // COM7 bit4 clear
        RegWrite {
            reg: REG_SENSOR_COM7,
            mask: 0x10,
            value: 0x00,
        },
        // CLKRC restore
        RegWrite {
            reg: REG_SENSOR_CLKRC,
            mask: 0x3F,
            value: 0x00,
        },
    ]
}
