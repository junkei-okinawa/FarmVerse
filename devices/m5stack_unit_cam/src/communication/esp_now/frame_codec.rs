pub const START_MARKER: [u8; 4] = [0xFA, 0xCE, 0xAA, 0xBB];
pub const END_MARKER: [u8; 4] = [0xCD, 0xEF, 0x56, 0x78];

pub const FRAME_OVERHEAD: usize = 4 + 6 + 1 + 4 + 4 + 4 + 4;
pub const ESP_NOW_MAX_SIZE: usize = 250;

pub fn safe_initial_payload_size(initial_chunk_size: usize) -> usize {
    let max_payload_size = ESP_NOW_MAX_SIZE - FRAME_OVERHEAD;
    initial_chunk_size.min(max_payload_size)
}

pub fn build_hash_payload(
    hash: &str,
    voltage_percentage: u8,
    temperature_celsius: Option<f32>,
    tds_voltage: Option<f32>,
    timestamp: &str,
) -> String {
    let temp_data = temperature_celsius.unwrap_or(-999.0);
    let tds_data = tds_voltage.unwrap_or(-999.0);
    format!(
        "HASH:{},VOLT:{},TEMP:{:.1},TDS_VOLT:{:.1},{}",
        hash, voltage_percentage, temp_data, tds_data, timestamp
    )
}

pub fn calculate_xor_checksum(data: &[u8]) -> u32 {
    let mut checksum: u32 = 0;
    for chunk in data.chunks(4) {
        let mut val: u32 = 0;
        for (i, &b) in chunk.iter().enumerate() {
            val |= (b as u32) << (i * 8);
        }
        checksum ^= val;
    }
    checksum
}

pub fn build_sensor_data_frame(
    frame_type: u8,
    mac_address: [u8; 6],
    sequence: u32,
    data: &[u8],
) -> Vec<u8> {
    let mut frame = Vec::new();

    frame.extend_from_slice(&START_MARKER);
    frame.extend_from_slice(&mac_address);
    frame.push(frame_type);
    frame.extend_from_slice(&sequence.to_le_bytes());

    let data_len = data.len() as u32;
    frame.extend_from_slice(&data_len.to_le_bytes());
    frame.extend_from_slice(data);

    let checksum = calculate_xor_checksum(data);
    frame.extend_from_slice(&checksum.to_le_bytes());
    frame.extend_from_slice(&END_MARKER);

    frame
}
