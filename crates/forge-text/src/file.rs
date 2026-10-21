//! A text file as the editor holds it: `\n`-only text plus what is needed to write it back
//! byte for byte (separator, encoding, BOM).

use serde::{Deserialize, Serialize};

use crate::encoding::{self, Encoding, Unmappable};
use crate::eol::{self, LineSeparator};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextFile {
    /// The content with `\n` separators.
    pub text: String,
    /// The separator the file uses (`\n` when it has no line breaks).
    pub eol: LineSeparator,
    pub encoding: Encoding,
    pub bom: bool,
    /// Bytes that are invalid in `encoding` were replaced.
    pub malformed: bool,
}

/// Decode a file, detecting its encoding unless `encoding` is given ("Reload in …").
pub fn read_text(bytes: &[u8], encoding: Option<Encoding>) -> TextFile {
    let decoded = encoding::decode(bytes, encoding);
    TextFile {
        eol: eol::detect(&decoded.text).unwrap_or_default(),
        text: eol::normalize(&decoded.text),
        encoding: decoded.encoding,
        bom: decoded.bom,
        malformed: decoded.malformed,
    }
}

/// Encode `\n`-only text with a separator, encoding and BOM.
pub fn write_text(
    text: &str,
    eol: LineSeparator,
    encoding: Encoding,
    bom: bool,
) -> Result<Vec<u8>, Unmappable> {
    encoding::encode(&eol::apply(text, eol), encoding, bom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_separator_encoding_and_bom() {
        let file = read_text(b"\xEF\xBB\xBFa\r\nb\r\n", None);
        assert_eq!(file.text, "a\nb\n");
        assert_eq!(file.eol, LineSeparator::Crlf);
        assert!(file.bom);
        let bytes = write_text(&file.text, file.eol, file.encoding, file.bom).unwrap();
        assert_eq!(bytes, b"\xEF\xBB\xBFa\r\nb\r\n");
        let file = read_text(b"caf\xE9\n", None);
        assert_eq!(
            (file.text.as_str(), file.encoding),
            ("caf\u{e9}\n", Encoding::Windows1252)
        );
        assert_eq!(read_text(b"x", None).eol, LineSeparator::Lf);
    }
}
