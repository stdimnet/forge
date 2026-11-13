//! Placeholder textures of generated mods: tiny RGBA PNGs drawn here (an item, a block and the
//! mod icon), so generated projects need no binary templates. The encoder writes stored
//! (uncompressed) deflate blocks, which every PNG reader accepts.

/// An RGBA image.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

impl Image {
    fn new(width: u32, height: u32) -> Image {
        Image {
            width,
            height,
            pixels: vec![[0, 0, 0, 0]; (width * height) as usize],
        }
    }

    fn set(&mut self, x: u32, y: u32, c: [u8; 4]) {
        if x < self.width && y < self.height {
            self.pixels[(y * self.width + x) as usize] = c;
        }
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &d in data {
        a = (a + d as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

/// zlib stream with stored deflate blocks.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    let mut chunks = data.chunks(65535).peekable();
    if chunks.peek().is_none() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(chunk) = chunks.next() {
        out.push(if chunks.peek().is_none() { 1 } else { 0 });
        let len = chunk.len() as u16;
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&(!len).to_le_bytes());
        out.extend_from_slice(chunk);
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// Encode an RGBA image as PNG.
pub fn encode(img: &Image) -> Vec<u8> {
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&img.width.to_be_bytes());
    ihdr.extend_from_slice(&img.height.to_be_bytes());
    // 8 bits, RGBA, deflate, adaptive filtering, no interlace.
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    let mut raw = Vec::with_capacity((img.width * img.height * 4 + img.height) as usize);
    for y in 0..img.height {
        raw.push(0);
        for x in 0..img.width {
            raw.extend_from_slice(&img.pixels[(y * img.width + x) as usize]);
        }
    }
    chunk(&mut out, b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// A color from the mod id, so different mods get different placeholders.
pub fn hue_color(seed: &str, light: u8) -> [u8; 4] {
    let h = seed
        .bytes()
        .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    let hue = (h % 360) as f32;
    let (s, l) = (0.55f32, light as f32 / 100.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (hue / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    [to(r), to(g), to(b), 255]
}

/// A 16×16 item texture: a gem with an outline and a highlight.
pub fn item_texture(seed: &str) -> Vec<u8> {
    let (fill, dark, light) = (
        hue_color(seed, 55),
        hue_color(seed, 25),
        hue_color(seed, 80),
    );
    let mut img = Image::new(16, 16);
    for y in 0..16i32 {
        for x in 0..16i32 {
            let d = (x - 8).abs() + (y - 8).abs();
            if d < 6 {
                img.set(x as u32, y as u32, fill);
            } else if d == 6 {
                img.set(x as u32, y as u32, dark);
            }
        }
    }
    for (x, y) in [(6, 5), (7, 4), (6, 6), (5, 6)] {
        img.set(x, y, light);
    }
    encode(&img)
}

/// A 16×16 block texture: tiles with a darker grid.
pub fn block_texture(seed: &str) -> Vec<u8> {
    let (fill, dark, light) = (
        hue_color(seed, 50),
        hue_color(seed, 30),
        hue_color(seed, 65),
    );
    let mut img = Image::new(16, 16);
    for y in 0..16u32 {
        for x in 0..16u32 {
            let c = if x == 0 || y == 0 || x == 15 || y == 15 || x == 8 || y == 8 {
                dark
            } else if (x + y) % 5 == 0 {
                light
            } else {
                fill
            };
            img.set(x, y, c);
        }
    }
    encode(&img)
}

/// The 64×64 mod icon: the block texture scaled up with a lighter frame.
pub fn icon(seed: &str) -> Vec<u8> {
    let (fill, dark, light) = (
        hue_color(seed, 50),
        hue_color(seed, 30),
        hue_color(seed, 75),
    );
    let mut img = Image::new(64, 64);
    for y in 0..64u32 {
        for x in 0..64u32 {
            let edge = x < 4 || y < 4 || x >= 60 || y >= 60;
            let d = (x as i32 - 32).abs() + (y as i32 - 32).abs();
            let c = if edge {
                dark
            } else if d < 20 {
                light
            } else {
                fill
            };
            img.set(x, y, c);
        }
    }
    encode(&img)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn writes_valid_pngs() {
        let png = item_texture("example");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 16);
        assert!(png.ends_with(&[0xAE, 0x42, 0x60, 0x82]));
        // 16 rows of a filter byte and 16 RGBA pixels, stored in one deflate block.
        let idat_len = u32::from_be_bytes(png[33..37].try_into().unwrap()) as usize;
        assert_eq!(idat_len, 2 + 5 + 16 * (1 + 64) + 4);
        assert_eq!(item_texture("example"), png);
        assert_ne!(block_texture("a"), block_texture("b"));
        let icon = icon("x");
        assert_eq!(u32::from_be_bytes(icon[20..24].try_into().unwrap()), 64);
    }
}
