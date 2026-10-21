//! Line separators. Editor buffers always use `\n`; a file remembers its separator and gets
//! it back on save.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum LineSeparator {
    #[default]
    #[serde(rename = "\n", alias = "lf", alias = "LF")]
    Lf,
    #[serde(rename = "\r\n", alias = "crlf", alias = "CRLF")]
    Crlf,
    #[serde(rename = "\r", alias = "cr", alias = "CR")]
    Cr,
}

impl LineSeparator {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
            Self::Cr => "\r",
        }
    }

    /// Status bar text: `LF`, `CRLF`, `CR`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::Crlf => "CRLF",
            Self::Cr => "CR",
        }
    }

    /// The separator of the operating system Forge runs on ("System-dependent").
    pub fn system() -> Self {
        if cfg!(windows) {
            Self::Crlf
        } else {
            Self::Lf
        }
    }

    /// Parse a settings or EditorConfig value (`lf`, `crlf`, `cr`, `system`).
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "lf" | "\n" => Some(Self::Lf),
            "crlf" | "\r\n" => Some(Self::Crlf),
            "cr" | "\r" => Some(Self::Cr),
            "system" => Some(Self::system()),
            _ => None,
        }
    }
}

/// Line separators counted in a text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EolCounts {
    pub lf: usize,
    pub crlf: usize,
    pub cr: usize,
}

pub fn count(text: &str) -> EolCounts {
    let bytes = text.as_bytes();
    let mut c = EolCounts::default();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => {
                c.crlf += 1;
                i += 1;
            }
            b'\r' => c.cr += 1,
            b'\n' => c.lf += 1,
            _ => {}
        }
        i += 1;
    }
    c
}

/// The separator most lines use; `None` for a text without line breaks.
pub fn detect(text: &str) -> Option<LineSeparator> {
    let c = count(text);
    if c.lf == 0 && c.crlf == 0 && c.cr == 0 {
        return None;
    }
    // Ties prefer LF, then CRLF.
    let mut best = (c.lf, LineSeparator::Lf);
    if c.crlf > best.0 {
        best = (c.crlf, LineSeparator::Crlf);
    }
    if c.cr > best.0 {
        best = (c.cr, LineSeparator::Cr);
    }
    Some(best.1)
}

/// Replace every separator by `\n`.
pub fn normalize(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Turn a `\n`-only text into one with `sep`.
pub fn apply(text: &str, sep: LineSeparator) -> String {
    match sep {
        LineSeparator::Lf => text.to_string(),
        other => text.replace('\n', other.as_str()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_the_majority_separator() {
        assert_eq!(detect("a\nb\r\nc\r\n"), Some(LineSeparator::Crlf));
        assert_eq!(detect("a\nb\n"), Some(LineSeparator::Lf));
        assert_eq!(detect("a\rb\r"), Some(LineSeparator::Cr));
        assert_eq!(detect("a\r\nb\n"), Some(LineSeparator::Lf));
        assert_eq!(detect("single line"), None);
    }

    #[test]
    fn normalizes_and_applies() {
        assert_eq!(normalize("a\r\nb\rc\n"), "a\nb\nc\n");
        assert_eq!(apply("a\nb\n", LineSeparator::Crlf), "a\r\nb\r\n");
        assert_eq!(apply("a\nb", LineSeparator::Cr), "a\rb");
        assert_eq!(apply("a\nb", LineSeparator::Lf), "a\nb");
    }

    #[test]
    fn parses_settings_values_and_serializes_as_strings() {
        assert_eq!(LineSeparator::parse("CRLF"), Some(LineSeparator::Crlf));
        assert_eq!(LineSeparator::parse("lf"), Some(LineSeparator::Lf));
        assert_eq!(LineSeparator::parse("nope"), None);
        assert_eq!(
            serde_json::to_string(&LineSeparator::Crlf).unwrap(),
            r#""\r\n""#
        );
        let lf: LineSeparator = serde_json::from_str(r#""lf""#).unwrap();
        assert_eq!(lf, LineSeparator::Lf);
    }
}
