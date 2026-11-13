//! Color values in code, for the editor's color previews and color picker (PHPStorm's gutter
//! swatches): finding colors in CSS, markup and scripts, parsing every CSS notation and
//! writing a picked color back in the notation it was written in.
//!
//! The crate is pure so it also runs as WebAssembly in the UI. Offsets are UTF-16.
//!
//! - [`color`]: [`Rgba`], parsing and formatting (hex, `rgb()`, `hsl()`, `hwb()`, names).
//! - [`named`]: CSS color names and the Tailwind CSS palette.
//! - [`find`]: colors in a document by language.

pub mod color;
pub mod find;
pub mod named;

pub use color::{format, parse, Rgba};
pub use find::{find_colors, ColorMatch};

use serde::{Deserialize, Serialize};

/// Colors of a document.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindRequest {
    pub text: String,
    #[serde(default)]
    pub language_id: Option<String>,
}

pub fn find(req: &FindRequest) -> Vec<ColorMatch> {
    find_colors(&req.text, req.language_id.as_deref().unwrap_or(""))
}

/// Write `color` in the notation of `original`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatRequest {
    pub color: Rgba,
    #[serde(default)]
    pub original: String,
}

pub fn format_color(req: &FormatRequest) -> String {
    format(req.color, &req.original)
}

/// A parsed color text (the picker's hex field).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Parsed {
    pub color: Rgba,
    pub hex: String,
}

pub fn parse_color(text: &str) -> Option<Parsed> {
    let t = text.trim();
    // The picker's hex field accepts digits without `#`.
    let (color, _) = parse(t).or_else(|| parse(&format!("#{t}")))?;
    Some(Parsed {
        color,
        hex: color.hex(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests() {
        let found = find(&FindRequest {
            text: "a { color: #fff }".into(),
            language_id: Some("css".into()),
        });
        assert_eq!(found.len(), 1);
        assert!(find(&FindRequest {
            text: "#fff".into(),
            language_id: None
        })
        .is_empty());
        let text = format_color(&FormatRequest {
            color: Rgba::new(0.0, 0.0, 0.0, 1.0),
            original: "white".into(),
        });
        assert_eq!(text, "black");
        assert_eq!(parse_color("ff0000").unwrap().hex, "#ff0000");
        assert_eq!(parse_color(" rgb(0 0 0 / 50%) ").unwrap().hex, "#00000080");
        assert!(parse_color("nope").is_none());
    }
}
