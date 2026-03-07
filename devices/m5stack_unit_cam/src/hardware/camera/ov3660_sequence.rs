use super::ov2640_sequence::RegWrite;

pub const REG_SYSTEM_CTRL0: i32 = 0x3008;
pub const CTRL_RUN: i32 = 0x02;
pub const CTRL_STANDBY: i32 = 0x42;

// 0x3008:
// - Bit7: software reset
// - Bit6: software power down (standby)
// 常用値として 0x02(動作) / 0x42(standby) を使う。

/// OV3660 を通常スタンバイへ移行するレジスタ列です。
pub fn standby_sequence() -> [RegWrite; 1] {
    [RegWrite {
        reg: REG_SYSTEM_CTRL0,
        mask: 0xFF,
        value: CTRL_STANDBY,
    }]
}

/// OV3660は通常スタンバイとDeepSleep前スタンバイで同じレジスタ列を使う。
/// API対称性と将来の分岐余地のため関数を分けている。
pub fn deep_sleep_standby_sequence() -> [RegWrite; 1] {
    standby_sequence()
}

/// OV3660 を通常動作へ復帰させるレジスタ列です。
pub fn resume_sequence() -> [RegWrite; 1] {
    [RegWrite {
        reg: REG_SYSTEM_CTRL0,
        mask: 0xFF,
        value: CTRL_RUN,
    }]
}
