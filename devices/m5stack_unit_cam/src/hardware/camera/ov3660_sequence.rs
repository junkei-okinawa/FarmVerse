use super::ov2640_sequence::RegWrite;

const REG_SYSTEM_CTRL0: i32 = 0x3008;

// 0x3008:
// - Bit7: software reset
// - Bit6: software power down (standby)
// 常用値として 0x02(動作) / 0x42(standby) を使う。

pub fn standby_sequence() -> [RegWrite; 1] {
    [RegWrite {
        reg: REG_SYSTEM_CTRL0,
        mask: 0xFF,
        value: 0x42,
    }]
}

pub fn deep_sleep_standby_sequence() -> [RegWrite; 1] {
    [RegWrite {
        reg: REG_SYSTEM_CTRL0,
        mask: 0xFF,
        value: 0x42,
    }]
}

pub fn resume_sequence() -> [RegWrite; 1] {
    [RegWrite {
        reg: REG_SYSTEM_CTRL0,
        mask: 0xFF,
        value: 0x02,
    }]
}
