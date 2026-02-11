pub const DUMMY_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

pub fn simple_image_hash(data: &[u8]) -> String {
    format!(
        "{:08x}{:08x}",
        data.len(),
        data.iter().map(|&b| b as u32).sum::<u32>()
    )
}

pub fn prepare_image_payload(image_data: Option<Vec<u8>>) -> (Vec<u8>, String) {
    match image_data {
        Some(data) if data.is_empty() => (vec![], DUMMY_HASH.to_string()),
        Some(data) => {
            let hash = simple_image_hash(&data);
            (data, hash)
        }
        None => (vec![], DUMMY_HASH.to_string()),
    }
}
