pub fn pack(a: u8, b: u8, c: u8) -> u32 {
    u32::from(a) << 16 | u32::from(b) << 8 | u32::from(c)
}

/// Sorted, deduplicated trigrams of `bytes`, ASCII case folded.
pub fn of_text(bytes: &[u8]) -> Vec<u32> {
    let lower = bytes.to_ascii_lowercase();
    let mut grams: Vec<u32> = lower.windows(3).map(|w| pack(w[0], w[1], w[2])).collect();
    grams.sort_unstable();
    grams.dedup();
    grams
}
