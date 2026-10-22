//! CSS color values: parsing every notation (hex, `rgb()`/`rgba()`, `hsl()`/`hsla()`,
//! `hwb()`, named colors, legacy comma and modern space syntax) and writing a color back in
//! the notation of the text it came from.

use serde::{Deserialize, Serialize};

use crate::named::{name_of, named_color};

/// A color: red, green and blue from 0 to 255 (fractions kept for round trips), alpha 0..1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    #[serde(default = "one")]
    pub a: f64,
}

fn one() -> f64 {
    1.0
}

impl Rgba {
    pub const fn new(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }

    fn clamped(self) -> Self {
        Self {
            r: self.r.clamp(0.0, 255.0),
            g: self.g.clamp(0.0, 255.0),
            b: self.b.clamp(0.0, 255.0),
            a: self.a.clamp(0.0, 1.0),
        }
    }

    fn bytes(self) -> [u8; 4] {
        let c = self.clamped();
        [
            c.r.round() as u8,
            c.g.round() as u8,
            c.b.round() as u8,
            (c.a * 255.0).round() as u8,
        ]
    }

    /// `#rrggbb`, or `#rrggbbaa` when not opaque.
    pub fn hex(self) -> String {
        let [r, g, b, a] = self.bytes();
        if a == 255 {
            format!("#{r:02x}{g:02x}{b:02x}")
        } else {
            format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
        }
    }

    /// Hue (degrees), saturation and lightness (0..1).
    pub fn hsl(self) -> (f64, f64, f64) {
        let c = self.clamped();
        let (r, g, b) = (c.r / 255.0, c.g / 255.0, c.b / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = (max + min) / 2.0;
        let d = max - min;
        if d.abs() < 1e-12 {
            return (0.0, 0.0, l);
        }
        let s = d / (1.0 - (2.0 * l - 1.0).abs());
        (hue(r, g, b, max, d), s, l)
    }

    /// Hue (degrees), whiteness and blackness (0..1).
    pub fn hwb(self) -> (f64, f64, f64) {
        let c = self.clamped();
        let (r, g, b) = (c.r / 255.0, c.g / 255.0, c.b / 255.0);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let h = if d.abs() < 1e-12 {
            0.0
        } else {
            hue(r, g, b, max, d)
        };
        (h, min, 1.0 - max)
    }

    pub fn from_hsl(h: f64, s: f64, l: f64, a: f64) -> Self {
        let s = s.clamp(0.0, 1.0);
        let l = l.clamp(0.0, 1.0);
        let f = |n: f64| {
            let k = (n + h.rem_euclid(360.0) / 30.0) % 12.0;
            let a = s * l.min(1.0 - l);
            l - a * (-1.0f64).max((k - 3.0).min(9.0 - k).min(1.0))
        };
        Self::new(f(0.0) * 255.0, f(8.0) * 255.0, f(4.0) * 255.0, a)
    }

    pub fn from_hwb(h: f64, w: f64, bl: f64, a: f64) -> Self {
        let (w, bl) = (w.clamp(0.0, 1.0), bl.clamp(0.0, 1.0));
        if w + bl >= 1.0 {
            let gray = w / (w + bl);
            return Self::new(gray * 255.0, gray * 255.0, gray * 255.0, a);
        }
        let base = Self::from_hsl(h, 1.0, 0.5, a);
        let scale = 1.0 - w - bl;
        let ch = |v: f64| (v / 255.0 * scale + w) * 255.0;
        Self::new(ch(base.r), ch(base.g), ch(base.b), a)
    }
}

fn hue(r: f64, g: f64, b: f64, max: f64, d: f64) -> f64 {
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0).rem_euclid(360.0)
}

/// Notation of a color text, kept when the color is written back.
#[derive(Debug, Clone, PartialEq)]
pub enum Notation {
    Hex {
        digits: usize,
        upper: bool,
    },
    Function {
        /// `rgb`, `rgba`, `hsl`, `hsla`, `hwb` as written (case kept).
        name: String,
        /// Space-separated with `/ alpha` (CSS Color 4) rather than commas.
        modern: bool,
        /// The separator after commas (`", "` or `","`).
        comma: String,
        /// Channels written as percentages (`rgb(100% 0% 0%)`).
        percent: bool,
        /// Alpha written as a percentage.
        alpha_percent: bool,
        /// The alpha was written.
        has_alpha: bool,
        /// The hue had a `deg` unit.
        deg: bool,
    },
    Named,
}

/// Parse a CSS color; `None` for anything else (including `var()` and `calc()`).
pub fn parse(text: &str) -> Option<(Rgba, Notation)> {
    let t = text.trim();
    if let Some(hex) = t.strip_prefix('#') {
        return parse_hex(hex).map(|c| {
            (
                c,
                Notation::Hex {
                    digits: hex.len(),
                    upper: hex.chars().any(|c| c.is_ascii_uppercase()),
                },
            )
        });
    }
    if let Some(open) = t.find('(') {
        if !t.ends_with(')') {
            return None;
        }
        let name = &t[..open];
        let args = &t[open + 1..t.len() - 1];
        return parse_function(name, args);
    }
    named_color(t).map(|c| (c, Notation::Named))
}

fn parse_hex(hex: &str) -> Option<Rgba> {
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let digit = |i: usize, len: usize| -> Option<f64> {
        let s = &hex[i * len..i * len + len];
        let v = u8::from_str_radix(s, 16).ok()?;
        Some(if len == 1 {
            f64::from(v * 17)
        } else {
            f64::from(v)
        })
    };
    match hex.len() {
        3 | 4 => Some(Rgba::new(
            digit(0, 1)?,
            digit(1, 1)?,
            digit(2, 1)?,
            if hex.len() == 4 {
                digit(3, 1)? / 255.0
            } else {
                1.0
            },
        )),
        6 | 8 => Some(Rgba::new(
            digit(0, 2)?,
            digit(1, 2)?,
            digit(2, 2)?,
            if hex.len() == 8 {
                digit(3, 2)? / 255.0
            } else {
                1.0
            },
        )),
        _ => None,
    }
}

/// A number with an optional unit: `12`, `.5`, `50%`, `120deg`.
fn number(s: &str) -> Option<(f64, &str)> {
    let s = s.trim();
    let end = s
        .char_indices()
        .find(|(i, c)| {
            !(c.is_ascii_digit()
                || *c == '.'
                || ((*c == '-' || *c == '+') && *i == 0)
                || *c == 'e' && *i > 0 && s[i + 1..].starts_with(|n: char| n.is_ascii_digit()))
        })
        .map_or(s.len(), |(i, _)| i);
    if s.eq_ignore_ascii_case("none") {
        return Some((0.0, ""));
    }
    let v: f64 = s[..end].parse().ok()?;
    if !v.is_finite() {
        return None;
    }
    Some((v, &s[end..]))
}

fn parse_function(name: &str, args: &str) -> Option<(Rgba, Notation)> {
    let lower = name.to_ascii_lowercase();
    if !matches!(lower.as_str(), "rgb" | "rgba" | "hsl" | "hsla" | "hwb") {
        return None;
    }
    let modern = !args.contains(',');
    let (channels, alpha): (Vec<&str>, Option<&str>) = if modern {
        let (c, a) = match args.split_once('/') {
            Some((c, a)) => (c, Some(a.trim())),
            None => (args, None),
        };
        (c.split_whitespace().collect(), a)
    } else {
        let parts: Vec<&str> = args.split(',').map(str::trim).collect();
        match parts.len() {
            3 => (parts, None),
            4 => (parts[..3].to_vec(), Some(parts[3])),
            _ => return None,
        }
    };
    if channels.len() != 3 || alpha.is_some_and(str::is_empty) {
        return None;
    }
    let (a, alpha_percent) = match alpha {
        None => (1.0, false),
        Some(a) => {
            let (v, unit) = number(a)?;
            match unit {
                "%" => (v / 100.0, true),
                "" => (v, false),
                _ => return None,
            }
        }
    };
    let comma = if modern {
        String::new()
    } else if args.contains(", ") {
        ", ".into()
    } else {
        ",".into()
    };
    let mut percent = false;
    let mut deg = false;
    let color = if lower.starts_with("rgb") {
        let mut v = [0.0; 3];
        for (i, ch) in channels.iter().enumerate() {
            let (n, unit) = number(ch)?;
            v[i] = match unit {
                "%" => {
                    percent = true;
                    n * 2.55
                }
                "" => n,
                _ => return None,
            };
        }
        Rgba::new(v[0], v[1], v[2], a)
    } else {
        let (h, unit) = number(channels[0])?;
        let h = match unit.to_ascii_lowercase().as_str() {
            "" => h,
            "deg" => {
                deg = true;
                h
            }
            "rad" => h.to_degrees(),
            "grad" => h * 0.9,
            "turn" => h * 360.0,
            _ => return None,
        };
        let pct = |s: &str| -> Option<f64> {
            let (n, unit) = number(s)?;
            match unit {
                "%" | "" => Some(n / 100.0),
                _ => None,
            }
        };
        let (x, y) = (pct(channels[1])?, pct(channels[2])?);
        if lower == "hwb" {
            Rgba::from_hwb(h, x, y, a)
        } else {
            Rgba::from_hsl(h, x, y, a)
        }
    };
    Some((
        color.clamped(),
        Notation::Function {
            name: name.to_string(),
            modern,
            comma,
            percent,
            alpha_percent,
            has_alpha: alpha.is_some(),
            deg,
        },
    ))
}

/// A number with at most `decimals` decimals, without trailing zeros.
fn num(v: f64, decimals: usize) -> String {
    let s = format!("{v:.decimals$}");
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    };
    if s == "-0" {
        "0".into()
    } else {
        s
    }
}

/// Write `color` in the notation of `original` (a hex color stays hex, `rgba(…)` stays
/// `rgba(…)` with the same separators, a named color becomes hex when no name fits).
pub fn format(color: Rgba, original: &str) -> String {
    let color = color.clamped();
    let notation = parse(original).map(|(_, n)| n).unwrap_or(Notation::Hex {
        digits: 6,
        upper: false,
    });
    match notation {
        Notation::Hex { digits, upper } => {
            let s = hex_with(color, digits);
            if upper {
                s.to_ascii_uppercase()
            } else {
                s
            }
        }
        Notation::Named => match name_of(color) {
            Some(name) => {
                if original.chars().all(|c| !c.is_ascii_lowercase()) && original.len() > 1 {
                    name.to_ascii_uppercase()
                } else {
                    name.to_string()
                }
            }
            None => color.hex(),
        },
        Notation::Function {
            name,
            modern,
            comma,
            percent,
            alpha_percent,
            has_alpha,
            deg,
        } => {
            let lower = name.to_ascii_lowercase();
            let alpha_needed = has_alpha || color.a < 1.0;
            let channels: [String; 3] = if lower.starts_with("rgb") {
                let ch = |v: f64| {
                    if percent {
                        format!("{}%", num(v / 2.55, 1))
                    } else {
                        num(v.round(), 0)
                    }
                };
                [ch(color.r), ch(color.g), ch(color.b)]
            } else {
                let (h, x, y) = if lower == "hwb" {
                    color.hwb()
                } else {
                    color.hsl()
                };
                let h = num(h.round(), 0);
                [
                    if deg { format!("{h}deg") } else { h },
                    format!("{}%", num(x * 100.0, 0)),
                    format!("{}%", num(y * 100.0, 0)),
                ]
            };
            let alpha = if alpha_percent {
                format!("{}%", num(color.a * 100.0, 0))
            } else {
                num(color.a, 2)
            };
            // Legacy `rgb(…)` gains the `a` when it needs an alpha.
            let fname = if !modern && alpha_needed && (lower == "rgb" || lower == "hsl") {
                format!(
                    "{name}{}",
                    if name.ends_with(|c: char| c.is_ascii_uppercase()) {
                        "A"
                    } else {
                        "a"
                    }
                )
            } else {
                name
            };
            if modern {
                let mut s = format!("{fname}({})", channels.join(" "));
                if alpha_needed {
                    s.insert_str(s.len() - 1, &format!(" / {alpha}"));
                }
                s
            } else if alpha_needed {
                format!("{fname}({}{comma}{alpha})", channels.join(&comma))
            } else {
                format!("{fname}({})", channels.join(&comma))
            }
        }
    }
}

/// Hex with `digits` (3, 4, 6, 8) when it can hold the color, else the closest longer form.
fn hex_with(color: Rgba, digits: usize) -> String {
    let [r, g, b, a] = color.bytes();
    let short = [r, g, b, a].iter().all(|v| v % 17 == 0);
    let opaque = a == 255;
    match (digits, short, opaque) {
        (3, true, true) => format!("#{:x}{:x}{:x}", r / 17, g / 17, b / 17),
        (3 | 4, true, _) => format!("#{:x}{:x}{:x}{:x}", r / 17, g / 17, b / 17, a / 17),
        (4 | 8, _, _) => format!("#{r:02x}{g:02x}{b:02x}{a:02x}"),
        _ => color.hex(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(t: &str) -> [u8; 4] {
        parse(t).unwrap_or_else(|| panic!("{t}")).0.bytes()
    }

    #[test]
    fn parses_notations() {
        assert_eq!(rgb("#f00"), [255, 0, 0, 255]);
        assert_eq!(rgb("#F008"), [255, 0, 0, 136]);
        assert_eq!(rgb("#1e90ff"), [30, 144, 255, 255]);
        assert_eq!(rgb("#1e90ff80"), [30, 144, 255, 128]);
        assert_eq!(rgb("rgb(255, 0, 0)"), [255, 0, 0, 255]);
        assert_eq!(rgb("rgba(0,0,255,.5)"), [0, 0, 255, 128]);
        assert_eq!(rgb("rgb(0 128 0 / 50%)"), [0, 128, 0, 128]);
        assert_eq!(rgb("rgb(100% 0% 0%)"), [255, 0, 0, 255]);
        assert_eq!(rgb("hsl(120, 100%, 50%)"), [0, 255, 0, 255]);
        assert_eq!(rgb("hsl(240deg 100% 50% / 0.25)"), [0, 0, 255, 64]);
        assert_eq!(rgb("hsla(0.5turn, 100%, 25%, 1)"), [0, 128, 128, 255]);
        assert_eq!(rgb("hwb(0 0% 0%)"), [255, 0, 0, 255]);
        assert_eq!(rgb("hwb(0 50% 50%)"), [128, 128, 128, 255]);
        assert_eq!(rgb("RebeccaPurple"), [102, 51, 153, 255]);
        assert_eq!(rgb("transparent"), [0, 0, 0, 0]);
        for bad in [
            "#ff",
            "#ggg",
            "rgb(1,2)",
            "rgb(var(--x))",
            "calc(1)",
            "rgb(1 2 3 /)",
            "nope",
            "rgb(1px 2 3)",
        ] {
            assert!(parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn converts_hsl_both_ways() {
        let c = Rgba::new(102.0, 51.0, 153.0, 1.0);
        let (h, s, l) = c.hsl();
        assert_eq!(
            (h.round(), (s * 100.0).round(), (l * 100.0).round()),
            (270.0, 50.0, 40.0)
        );
        let back = Rgba::from_hsl(h, s, l, 1.0).bytes();
        assert_eq!(back, [102, 51, 153, 255]);
    }

    #[test]
    fn formats_in_the_original_notation() {
        let red = Rgba::new(255.0, 0.0, 0.0, 1.0);
        let half_blue = Rgba::new(0.0, 0.0, 255.0, 0.5);
        let teal = Rgba::new(0.0, 128.0, 128.0, 1.0);
        assert_eq!(format(red, "#abc"), "#f00");
        assert_eq!(format(red, "#ABCDEF"), "#FF0000");
        assert_eq!(format(teal, "#abc"), "#008080");
        assert_eq!(format(half_blue, "#abc"), "#0000ff80");
        assert_eq!(format(Rgba::new(0.0, 0.0, 255.0, 0.6), "#abc"), "#00f9");
        assert_eq!(format(red, "rgb(1, 2, 3)"), "rgb(255, 0, 0)");
        assert_eq!(format(half_blue, "rgb(1,2,3)"), "rgba(0,0,255,0.5)");
        assert_eq!(format(red, "rgba(1, 2, 3, .3)"), "rgba(255, 0, 0, 1)");
        assert_eq!(format(half_blue, "rgb(1 2 3)"), "rgb(0 0 255 / 0.5)");
        assert_eq!(format(red, "rgb(1 2 3 / 50%)"), "rgb(255 0 0 / 100%)");
        assert_eq!(format(red, "rgb(10% 20% 30%)"), "rgb(100% 0% 0%)");
        assert_eq!(format(teal, "hsl(0, 0%, 0%)"), "hsl(180, 100%, 25%)");
        assert_eq!(format(teal, "hsl(0deg 0% 0%)"), "hsl(180deg 100% 25%)");
        assert_eq!(
            format(half_blue, "hsl(0, 0%, 0%)"),
            "hsla(240, 100%, 50%, 0.5)"
        );
        assert_eq!(format(red, "hwb(0 10% 10%)"), "hwb(0 0% 0%)");
        assert_eq!(format(red, "blue"), "red");
        assert_eq!(format(red, "BLUE"), "RED");
        assert_eq!(format(Rgba::new(1.0, 2.0, 3.0, 1.0), "blue"), "#010203");
        assert_eq!(format(red, "var(--x)"), "#ff0000");
    }

    #[test]
    fn hex_output() {
        assert_eq!(Rgba::new(255.0, 255.0, 255.0, 1.0).hex(), "#ffffff");
        assert_eq!(Rgba::new(300.0, -1.0, 0.0, 0.0).hex(), "#ff000000");
    }
}
