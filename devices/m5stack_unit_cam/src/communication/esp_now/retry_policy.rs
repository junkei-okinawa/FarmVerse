pub fn retry_delay_ms(attempt: u8) -> u32 {
    300 * attempt as u32
}

pub fn no_mem_retry_delay_ms(attempt: u8) -> u32 {
    800 + (attempt as u32 * 400)
}

pub fn retry_count_for_chunk(chunk_index: usize) -> u8 {
    if chunk_index < 3 {
        2
    } else {
        1
    }
}
