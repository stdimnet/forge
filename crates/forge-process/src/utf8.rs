/// Turns a byte stream into UTF-8 text without breaking multi-byte characters that are
/// split across reads. Invalid sequences become U+FFFD.
#[derive(Debug, Default)]
pub struct Utf8Decoder {
    pending: Vec<u8>,
}

impl Utf8Decoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Decode `bytes`, keeping an incomplete trailing character for the next call.
    pub fn decode(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();
        let mut rest: &[u8] = &self.pending;
        loop {
            match std::str::from_utf8(rest) {
                Ok(s) => {
                    out.push_str(s);
                    rest = &[];
                    break;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    out.push_str(std::str::from_utf8(&rest[..valid]).expect("valid prefix"));
                    match e.error_len() {
                        Some(len) => {
                            out.push('\u{FFFD}');
                            rest = &rest[valid + len..];
                        }
                        // Incomplete character at the end: wait for more bytes.
                        None => {
                            rest = &rest[valid..];
                            break;
                        }
                    }
                }
            }
        }
        self.pending = rest.to_vec();
        out
    }

    /// Flush what is left at end of stream.
    pub fn finish(&mut self) -> String {
        let s = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_split_characters_together() {
        let mut d = Utf8Decoder::new();
        let bytes = "größe €".as_bytes();
        let (a, b) = bytes.split_at(3); // splits the "ö"
        let mut s = d.decode(a);
        s += &d.decode(b);
        assert_eq!(s, "größe €");
        assert_eq!(d.finish(), "");
    }

    #[test]
    fn replaces_invalid_bytes() {
        let mut d = Utf8Decoder::new();
        assert_eq!(d.decode(b"a\xffb"), "a\u{FFFD}b");
        assert_eq!(d.decode(b"\xe2\x82"), "");
        assert_eq!(d.finish(), "\u{FFFD}");
    }
}
