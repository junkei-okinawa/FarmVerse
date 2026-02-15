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

const BANK_DSP: i32 = 0x00;
const BANK_SENSOR: i32 = 0x01;

pub fn standby_sequence() -> [RegWrite; 3] {
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
    ]
}

pub fn deep_sleep_standby_sequence() -> [RegWrite; 2] {
    [
        // BANK_SEL = DSP
        RegWrite {
            reg: REG_BANK_SEL,
            mask: 0xFF,
            value: BANK_DSP,
        },
        // DVP output off only (keep SCCB path safer across deep sleep)
        RegWrite {
            reg: REG_DSP_R_DVP_SP,
            mask: 0xFF,
            value: 0x00,
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

pub fn resume_sequence() -> [RegWrite; 2] {
    [
        // BANK_SEL = SENSOR
        RegWrite {
            reg: REG_BANK_SEL,
            mask: 0xFF,
            value: BANK_SENSOR,
        },
        // CLKRC restore
        RegWrite {
            reg: REG_SENSOR_CLKRC,
            mask: 0x3F,
            value: 0x00,
        },
    ]
}
