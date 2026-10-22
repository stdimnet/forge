//! File encodings: what the status bar shows and what the encoding popup offers.
//!
//! Detection: a byte order mark wins (UTF-8, UTF-16 LE/BE); otherwise valid UTF-8 is UTF-8
//! and anything else is read as Windows-1252 (a superset of ISO-8859-1's printable range),
//! like PHPStorm falling back to a single-byte encoding.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum Encoding {
    #[default]
    #[serde(rename = "UTF-8", alias = "utf-8", alias = "utf8")]
    Utf8,
    #[serde(rename = "UTF-16LE", alias = "utf-16le")]
    Utf16Le,
    #[serde(rename = "UTF-16BE", alias = "utf-16be")]
    Utf16Be,
    #[serde(rename = "ISO-8859-1", alias = "iso-8859-1", alias = "latin1")]
    Iso8859_1,
    #[serde(rename = "windows-1252", alias = "Windows-1252", alias = "cp1252")]
    Windows1252,
}

/// Every encoding Forge reads and writes, in the order the popup lists them.
pub const ALL: [Encoding; 5] = [
    Encoding::Utf8,
    Encoding::Utf16Le,
    Encoding::Utf16Be,
    Encoding::Iso8859_1,
    Encoding::Windows1252,
];

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const UTF16LE_BOM: &[u8] = &[0xFF, 0xFE];
const UTF16BE_BOM: &[u8] = &[0xFE, 0xFF];

impl Encoding {
    /// The name PHPStorm shows (`UTF-8`, `windows-1252`).
    pub fn name(self) -> &'static str {
        match self {
            Self::Utf8 => "UTF-8",
            Self::Utf16Le => "UTF-16LE",
            Self::Utf16Be => "UTF-16BE",
            Self::Iso8859_1 => "ISO-8859-1",
            Self::Windows1252 => "windows-1252",
        }
    }

    /// Parse an encoding name (also EditorConfig `charset` values: `utf-8`, `utf-8-bom`,
    /// `utf-16le`, `utf-16be`, `latin1`). Returns the encoding and whether a BOM is wanted.
    pub fn parse(value: &str) -> Option<(Self, bool)> {
        let v = value.trim().to_ascii_lowercase().replace('_', "-");
        Some(match v.as_str() {
            "utf-8" | "utf8" => (Self::Utf8, false),
            "utf-8-bom" | "utf-8 bom" => (Self::Utf8, true),
            "utf-16le" => (Self::Utf16Le, true),
            "utf-16be" => (Self::Utf16Be, true),
            "iso-8859-1" | "latin1" | "latin-1" => (Self::Iso8859_1, false),
            "windows-1252" | "cp1252" => (Self::Windows1252, false),
            _ => return None,
        })
    }

    fn bom(self) -> &'static [u8] {
        match self {
            Self::Utf8 => UTF8_BOM,
            Self::Utf16Le => UTF16LE_BOM,
            Self::Utf16Be => UTF16BE_BOM,
            _ => &[],
        }
    }

    /// Whether files in this encoding can start with a byte order mark.
    pub fn supports_bom(self) -> bool {
        !self.bom().is_empty()
    }
}

/// A decoded file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodedText {
    pub text: String,
    pub encoding: Encoding,
    /// The file starts with a byte order mark (kept on save).
    pub bom: bool,
    /// Some bytes were not valid in `encoding` and were replaced by U+FFFD.
    pub malformed: bool,
}

/// Guess the encoding of `bytes`: BOM, then UTF-8 validity, else Windows-1252.
pub fn detect(bytes: &[u8]) -> (Encoding, bool) {
    if bytes.starts_with(UTF8_BOM) {
        (Encoding::Utf8, true)
    } else if bytes.starts_with(UTF16LE_BOM) {
        (Encoding::Utf16Le, true)
    } else if bytes.starts_with(UTF16BE_BOM) {
        (Encoding::Utf16Be, true)
    } else if std::str::from_utf8(bytes).is_ok() {
        (Encoding::Utf8, false)
    } else {
        (Encoding::Windows1252, false)
    }
}

/// Decode `bytes`, detecting the encoding unless `forced` is given ("Reload in another
/// encoding"). A BOM matching the encoding is stripped and remembered.
pub fn decode(bytes: &[u8], forced: Option<Encoding>) -> DecodedText {
    let encoding = forced.unwrap_or_else(|| detect(bytes).0);
    let bom = encoding.supports_bom() && bytes.starts_with(encoding.bom());
    let body = if bom {
        &bytes[encoding.bom().len()..]
    } else {
        bytes
    };
    let (text, malformed) = match encoding {
        Encoding::Utf8 => match std::str::from_utf8(body) {
            Ok(s) => (s.to_string(), false),
            Err(_) => (String::from_utf8_lossy(body).into_owned(), true),
        },
        Encoding::Utf16Le | Encoding::Utf16Be => {
            let le = encoding == Encoding::Utf16Le;
            let units: Vec<u16> = body
                .chunks(2)
                .map(|c| match c {
                    [a, b] if le => u16::from_le_bytes([*a, *b]),
                    [a, b] => u16::from_be_bytes([*a, *b]),
                    _ => 0xFFFD,
                })
                .collect();
            let odd = body.len() % 2 == 1;
            match String::from_utf16(&units) {
                Ok(s) => (s, odd),
                Err(_) => (String::from_utf16_lossy(&units), true),
            }
        }
        Encoding::Iso8859_1 => (body.iter().map(|&b| b as char).collect(), false),
        Encoding::Windows1252 => (body.iter().map(|&b| cp1252_char(b)).collect(), false),
    };
    DecodedText {
        text,
        encoding,
        bom,
        malformed,
    }
}

/// Characters of a text that `encoding` cannot represent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Unmappable {
    pub encoding: Encoding,
    /// The first few characters that do not fit, without duplicates.
    pub chars: Vec<char>,
}

impl std::fmt::Display for Unmappable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let chars: String = self.chars.iter().collect();
        write!(
            f,
            "{} cannot represent these characters: {chars}",
            self.encoding.name()
        )
    }
}

/// Encode `text` (with a BOM when `bom` is set and the encoding has one).
pub fn encode(text: &str, encoding: Encoding, bom: bool) -> Result<Vec<u8>, Unmappable> {
    let mut out = Vec::with_capacity(text.len() + 3);
    if bom {
        out.extend_from_slice(encoding.bom());
    }
    match encoding {
        Encoding::Utf8 => out.extend_from_slice(text.as_bytes()),
        Encoding::Utf16Le => text
            .encode_utf16()
            .for_each(|u| out.extend_from_slice(&u.to_le_bytes())),
        Encoding::Utf16Be => text
            .encode_utf16()
            .for_each(|u| out.extend_from_slice(&u.to_be_bytes())),
        Encoding::Iso8859_1 | Encoding::Windows1252 => {
            let mut bad: Vec<char> = Vec::new();
            for c in text.chars() {
                let byte = if encoding == Encoding::Iso8859_1 {
                    u8::try_from(c as u32).ok()
                } else {
                    cp1252_byte(c)
                };
                match byte {
                    Some(b) => out.push(b),
                    None if bad.len() < 10 && !bad.contains(&c) => bad.push(c),
                    None => {}
                }
            }
            if !bad.is_empty() {
                return Err(Unmappable {
                    encoding,
                    chars: bad,
                });
            }
        }
    }
    Ok(out)
}

/// Windows-1252 differs from ISO-8859-1 in 0x80–0x9F. Undefined bytes map to the C1
/// control with the same value (as browsers do), so every byte round-trips.
const CP1252_HIGH: [u32; 32] = [
    0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014,
    0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
];

fn cp1252_char(b: u8) -> char {
    if (0x80..0xA0).contains(&b) {
        char::from_u32(CP1252_HIGH[(b - 0x80) as usize]).unwrap_or('\u{FFFD}')
    } else {
        b as char
    }
}

fn cp1252_byte(c: char) -> Option<u8> {
    let cp = c as u32;
    if cp < 0x80 || (0xA0..=0xFF).contains(&cp) {
        return Some(cp as u8);
    }
    CP1252_HIGH
        .iter()
        .position(|&x| x == cp)
        .map(|i| 0x80 + i as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_boms_utf8_and_falls_back_to_windows_1252() {
        assert_eq!(detect(b"\xEF\xBB\xBFhi"), (Encoding::Utf8, true));
        assert_eq!(detect(b"\xFF\xFEh\0"), (Encoding::Utf16Le, true));
        assert_eq!(detect(b"\xFE\xFF\0h"), (Encoding::Utf16Be, true));
        assert_eq!(detect("caf\u{e9}".as_bytes()), (Encoding::Utf8, false));
        assert_eq!(detect(b"caf\xE9"), (Encoding::Windows1252, false));
    }

    #[test]
    fn decodes_each_encoding() {
        let d = decode(b"\xEF\xBB\xBFa\xC3\xA9", None);
        assert_eq!(
            (d.text.as_str(), d.encoding, d.bom),
            ("a\u{e9}", Encoding::Utf8, true)
        );
        let d = decode(b"\x80 caf\xE9", None);
        assert_eq!(d.text, "\u{20AC} caf\u{e9}");
        assert_eq!(d.encoding, Encoding::Windows1252);
        let d = decode(b"\x80", Some(Encoding::Iso8859_1));
        assert_eq!(d.text, "\u{80}");
        let d = decode(b"\xFF\xFEh\0i\0", None);
        assert_eq!((d.text.as_str(), d.bom), ("hi", true));
        let d = decode(b"\0h\0i", Some(Encoding::Utf16Be));
        assert_eq!((d.text.as_str(), d.bom), ("hi", false));
        let d = decode(b"caf\xE9", Some(Encoding::Utf8));
        assert!(d.malformed);
        assert_eq!(d.text, "caf\u{FFFD}");
    }

    #[test]
    fn encodes_and_round_trips() {
        assert_eq!(
            encode("a\u{e9}", Encoding::Utf8, true).unwrap(),
            b"\xEF\xBB\xBFa\xC3\xA9"
        );
        assert_eq!(
            encode("\u{20AC}", Encoding::Windows1252, false).unwrap(),
            b"\x80"
        );
        assert_eq!(
            encode("hi", Encoding::Utf16Le, true).unwrap(),
            b"\xFF\xFEh\0i\0"
        );
        for b in 0u8..=255 {
            let text = decode(&[b], Some(Encoding::Windows1252)).text;
            assert_eq!(
                encode(&text, Encoding::Windows1252, false).unwrap(),
                vec![b]
            );
            let text = decode(&[b], Some(Encoding::Iso8859_1)).text;
            assert_eq!(encode(&text, Encoding::Iso8859_1, false).unwrap(), vec![b]);
        }
    }

    #[test]
    fn reports_unmappable_characters() {
        let err = encode("\u{20AC} \u{4e2d}\u{4e2d}", Encoding::Iso8859_1, false).unwrap_err();
        assert_eq!(err.chars, vec!['\u{20AC}', '\u{4e2d}']);
        assert!(err.to_string().contains("ISO-8859-1"));
        // The BOM flag is ignored for single-byte encodings.
        assert_eq!(encode("a", Encoding::Iso8859_1, true).unwrap(), b"a");
    }

    #[test]
    fn parses_names_and_editorconfig_charsets() {
        assert_eq!(Encoding::parse("utf-8-bom"), Some((Encoding::Utf8, true)));
        assert_eq!(
            Encoding::parse("latin1"),
            Some((Encoding::Iso8859_1, false))
        );
        assert_eq!(
            Encoding::parse("Windows-1252"),
            Some((Encoding::Windows1252, false))
        );
        assert_eq!(Encoding::parse("koi8-r"), None);
        assert_eq!(
            serde_json::to_string(&Encoding::Windows1252).unwrap(),
            r#""windows-1252""#
        );
    }
}
