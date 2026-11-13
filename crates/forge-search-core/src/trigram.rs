use std::cell::RefCell;

/// Texts from this size on are deduplicated with a bitmap instead of a sort.
const BITMAP_FROM: usize = 4096;
const BITMAP_WORDS: usize = 1 << 18;

thread_local! {
    static SEEN: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

pub fn pack(a: u8, b: u8, c: u8) -> u32 {
    u32::from(a) << 16 | u32::from(b) << 8 | u32::from(c)
}

/// Distinct trigrams of `bytes`, ASCII case folded, in no particular order.
pub fn of_text(bytes: &[u8]) -> Vec<u32> {
    if bytes.len() < BITMAP_FROM {
        let lower = bytes.to_ascii_lowercase();
        let mut grams: Vec<u32> = lower.windows(3).map(|w| pack(w[0], w[1], w[2])).collect();
        grams.sort_unstable();
        grams.dedup();
        return grams;
    }
    SEEN.with(|seen| {
        let mut seen = seen.borrow_mut();
        if seen.is_empty() {
            seen.resize(BITMAP_WORDS, 0);
        }
        let mut out = Vec::new();
        let (mut a, mut b) = (bytes[0].to_ascii_lowercase(), bytes[1].to_ascii_lowercase());
        for &c in &bytes[2..] {
            let c = c.to_ascii_lowercase();
            let gram = pack(a, b, c);
            let (word, bit) = ((gram >> 6) as usize, 1u64 << (gram & 63));
            if seen[word] & bit == 0 {
                seen[word] |= bit;
                out.push(gram);
            }
            (a, b) = (b, c);
        }
        for &gram in &out {
            seen[(gram >> 6) as usize] &= !(1u64 << (gram & 63));
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_paths_agree() {
        let mut text = Vec::new();
        for i in 0..3000u32 {
            text.extend_from_slice(format!("Fn{i} Return x\n").as_bytes());
        }
        let mut by_bitmap = of_text(&text);
        by_bitmap.sort_unstable();
        let mut by_sort: Vec<u32> = text
            .to_ascii_lowercase()
            .windows(3)
            .map(|w| pack(w[0], w[1], w[2]))
            .collect();
        by_sort.sort_unstable();
        by_sort.dedup();
        assert!(text.len() >= BITMAP_FROM);
        assert_eq!(by_bitmap, by_sort);
        assert_eq!(of_text(&text).len(), by_sort.len());
        assert!(of_text(b"ab").is_empty());
    }
}
