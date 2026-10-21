//! Image metadata for the image editor (PHPStorm shows "512x512 PNG (32-bit color) 4.17 kB").

use std::sync::OnceLock;

use base64::Engine;
use regex_lite::Regex;
use serde::{Deserialize, Serialize};

/// Extensions the image editor opens, with their MIME types.
const IMAGE_TYPES: &[(&str, &str)] = &[
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("ico", "image/x-icon"),
    ("bmp", "image/bmp"),
    ("svg", "image/svg+xml"),
];

/// Images larger than this are not sent to the UI.
pub const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;

fn extension(path: &str) -> String {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// MIME type by file extension, for the files the image editor opens.
pub fn mime_for(path: &str) -> Option<&'static str> {
    let ext = extension(path);
    IMAGE_TYPES.iter().find(|(e, _)| *e == ext).map(|(_, m)| *m)
}

/// Whether the image editor opens `path` (SVG included).
pub fn is_image_path(path: &str) -> bool {
    mime_for(path).is_some()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageInfo {
    /// `PNG`, `JPEG`, `GIF`, `WebP`, `ICO`, `BMP` or `SVG`.
    pub format: String,
    pub mime: String,
    /// Pixels; for SVG the size of the root element (`width`/`height`, else the `viewBox`).
    pub width: u32,
    pub height: u32,
    /// File size in bytes.
    pub size: u64,
    /// Bits per pixel when the header says (PNG, BMP).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<u32>,
}

/// An image for the UI: its info and its bytes in base64.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageData {
    pub info: ImageInfo,
    pub base64: String,
}

fn svg_root_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?is)<svg\b[^>]*>").expect("svg regex"))
}

fn svg_attr(tag: &str, name: &str) -> Option<String> {
    let re = Regex::new(&format!(r#"(?i)\s{name}\s*=\s*(?:"([^"]*)"|'([^']*)')"#)).ok()?;
    let c = re.captures(tag)?;
    Some(c.get(1).or(c.get(2))?.as_str().trim().to_string())
}

/// A length in pixels (`120`, `120px`, `1.5in`); percentages and `em` give `None`.
fn svg_length(v: &str) -> Option<f64> {
    let v = v.trim();
    let split = v
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e'))
        .unwrap_or(v.len());
    let (num, unit) = v.split_at(split);
    let n: f64 = num.parse().ok()?;
    let px = match unit.trim() {
        "" | "px" => n,
        "pt" => n * 4.0 / 3.0,
        "pc" => n * 16.0,
        "in" => n * 96.0,
        "cm" => n * 96.0 / 2.54,
        "mm" => n * 96.0 / 25.4,
        _ => return None,
    };
    (px > 0.0).then_some(px)
}

/// Width and height of an SVG document.
fn svg_size(text: &str) -> Option<(u32, u32)> {
    let tag = svg_root_re().find(text)?.as_str();
    let view_box = svg_attr(tag, "viewBox").and_then(|vb| {
        let n: Vec<f64> = vb
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        (n.len() == 4 && n[2] > 0.0 && n[3] > 0.0).then(|| (n[2], n[3]))
    });
    let w = svg_attr(tag, "width").and_then(|v| svg_length(&v));
    let h = svg_attr(tag, "height").and_then(|v| svg_length(&v));
    let (w, h) = match (w, h, view_box) {
        (Some(w), Some(h), _) => (w, h),
        (Some(w), None, Some((vw, vh))) => (w, w * vh / vw),
        (None, Some(h), Some((vw, vh))) => (h * vw / vh, h),
        (None, None, Some(vb)) => vb,
        // Browsers render an SVG without any size at 300x150.
        _ => (300.0, 150.0),
    };
    Some((w.round().max(1.0) as u32, h.round().max(1.0) as u32))
}

fn format_name(t: imagesize::ImageType) -> Option<&'static str> {
    use imagesize::ImageType as T;
    Some(match t {
        T::Png => "PNG",
        T::Jpeg => "JPEG",
        T::Gif => "GIF",
        T::Webp => "WebP",
        T::Ico => "ICO",
        T::Bmp => "BMP",
        _ => return None,
    })
}

fn mime_of(format: &str) -> &'static str {
    match format {
        "PNG" => "image/png",
        "JPEG" => "image/jpeg",
        "GIF" => "image/gif",
        "WebP" => "image/webp",
        "ICO" => "image/x-icon",
        "BMP" => "image/bmp",
        _ => "image/svg+xml",
    }
}

fn bit_depth(format: &str, bytes: &[u8]) -> Option<u32> {
    match format {
        // IHDR: bit depth at 24, colour type at 25.
        "PNG" if bytes.len() > 25 => {
            let depth = bytes[24] as u32;
            let channels = match bytes[25] {
                0 | 3 => 1,
                2 => 3,
                4 => 2,
                6 => 4,
                _ => return None,
            };
            Some(depth * channels)
        }
        "BMP" if bytes.len() > 29 => Some(u16::from_le_bytes([bytes[28], bytes[29]]) as u32),
        _ => None,
    }
}

/// The info of an image file's bytes; `path` tells SVG apart. `None` for unknown formats.
pub fn image_info(path: &str, bytes: &[u8]) -> Option<ImageInfo> {
    let size = bytes.len() as u64;
    let looks_svg = extension(path) == "svg" || {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
        head.trim_start().starts_with("<svg") || head.contains("<svg")
    };
    let raster = imagesize::image_type(bytes).ok().and_then(format_name);
    if let Some(format) = raster {
        let dims = imagesize::blob_size(bytes).ok()?;
        return Some(ImageInfo {
            format: format.into(),
            mime: mime_of(format).into(),
            width: dims.width as u32,
            height: dims.height as u32,
            size,
            bit_depth: bit_depth(format, bytes),
        });
    }
    if looks_svg {
        let (width, height) = svg_size(&String::from_utf8_lossy(bytes))?;
        return Some(ImageInfo {
            format: "SVG".into(),
            mime: "image/svg+xml".into(),
            width,
            height,
            size,
            bit_depth: None,
        });
    }
    None
}

/// An image file for the UI. Errors name what is wrong in the words the editor shows.
pub fn image_data(path: &str, bytes: &[u8]) -> Result<ImageData, String> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(format!(
            "The image is too large to show ({} MB)",
            bytes.len() / (1024 * 1024)
        ));
    }
    let info = image_info(path, bytes).ok_or_else(|| "Unsupported or damaged image".to_string())?;
    Ok(ImageData {
        info,
        base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x3 RGBA PNG header (enough for the size and depth).
    fn png() -> Vec<u8> {
        let mut b = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        b.extend_from_slice(&[0, 0, 0, 13]);
        b.extend_from_slice(b"IHDR");
        b.extend_from_slice(&2u32.to_be_bytes());
        b.extend_from_slice(&3u32.to_be_bytes());
        b.extend_from_slice(&[8, 6, 0, 0, 0]);
        b.extend_from_slice(&[0, 0, 0, 0]);
        b
    }

    #[test]
    fn reads_png_size_and_depth() {
        let info = image_info("a/logo.png", &png()).unwrap();
        assert_eq!(
            (info.format.as_str(), info.width, info.height),
            ("PNG", 2, 3)
        );
        assert_eq!(info.bit_depth, Some(32));
        assert_eq!(info.mime, "image/png");
        assert_eq!(info.size, 33);
    }

    #[test]
    fn reads_gif_and_bmp() {
        let gif = b"GIF89a\x10\x00\x20\x00\x00\x00\x00";
        let info = image_info("x.gif", gif).unwrap();
        assert_eq!((info.width, info.height), (16, 32));
        let mut bmp = b"BM".to_vec();
        bmp.extend_from_slice(&[0; 12]);
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&7i32.to_le_bytes());
        bmp.extend_from_slice(&5i32.to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&24u16.to_le_bytes());
        bmp.extend_from_slice(&[0; 24]);
        let info = image_info("x.bmp", &bmp).unwrap();
        assert_eq!(
            (
                info.format.as_str(),
                info.width,
                info.height,
                info.bit_depth
            ),
            ("BMP", 7, 5, Some(24))
        );
    }

    #[test]
    fn svg_sizes_from_attributes_and_view_box() {
        let svg = |s: &str| image_info("i.svg", s.as_bytes()).map(|i| (i.width, i.height));
        assert_eq!(
            svg(r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="32px"/>"#),
            Some((64, 32))
        );
        assert_eq!(
            svg(r#"<?xml version="1.0"?><svg viewBox="0 0 120 80"></svg>"#),
            Some((120, 80))
        );
        assert_eq!(
            svg(r#"<svg width='240' viewBox="0, 0, 120, 60">"#),
            Some((240, 120))
        );
        assert_eq!(svg(r#"<svg width="100%" height="100%">"#), Some((300, 150)));
        assert_eq!(svg("not an image"), None);
    }

    #[test]
    fn image_data_is_base64_and_refuses_unknown_bytes() {
        let data = image_data("logo.png", &png()).unwrap();
        assert!(data.base64.starts_with("iVBORw0KGgo"));
        assert!(image_data("x.png", b"plain text").is_err());
        assert!(is_image_path("assets/Logo.PNG"));
        assert!(is_image_path("icon.svg"));
        assert!(!is_image_path("notes.md"));
    }
}
