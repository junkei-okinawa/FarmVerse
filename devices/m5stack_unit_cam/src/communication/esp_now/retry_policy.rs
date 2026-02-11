pub fn retry_delay_ms(attempt: u8) -> u32 {
    300 * attempt as u32
}

pub fn no_mem_retry_delay_ms(attempt: u8) -> u32 {
    800 + (attempt as u32 * 400)
}
