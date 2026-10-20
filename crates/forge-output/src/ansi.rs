//! ANSI escape handling: SGR colors and styles become styled segments, other control
//! sequences (cursor movement, OSC titles) are dropped. Colors are CSS values, either a
//! theme variable (`var(--ansi-red)`) or `rgb(…)`.

use serde::{Deserialize, Serialize};

fn is_false(b: &bool) -> bool {
    !*b
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Style {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub dim: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub inverse: bool,
}

impl Style {
    /// Only a foreground color.
    pub fn fg(color: &str) -> Self {
        Style {
            fg: Some(color.to_string()),
            ..Style::default()
        }
    }

    /// `self` on top of `base`: set values of `self` win.
    pub fn over(&self, base: &Style) -> Style {
        Style {
            fg: self.fg.clone().or_else(|| base.fg.clone()),
            bg: self.bg.clone().or_else(|| base.bg.clone()),
            bold: self.bold || base.bold,
            dim: self.dim || base.dim,
            italic: self.italic || base.italic,
            underline: self.underline || base.underline,
            inverse: self.inverse || base.inverse,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Segment {
    pub text: String,
    pub style: Style,
}

const NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

fn named(i: u32, bright: bool) -> String {
    format!(
        "var(--ansi-{}{})",
        if bright { "bright-" } else { "" },
        NAMES[i as usize]
    )
}

/// xterm 256-color palette entry as CSS.
pub fn color256(n: u32) -> String {
    match n {
        0..=7 => named(n, false),
        8..=15 => named(n - 8, true),
        16..=231 => {
            let v = n - 16;
            let level = |x: u32| if x == 0 { 0 } else { 55 + x * 40 };
            format!(
                "rgb({}, {}, {})",
                level(v / 36),
                level((v / 6) % 6),
                level(v % 6)
            )
        }
        _ => {
            let g = 8 + (n.min(255) - 232) * 10;
            format!("rgb({g}, {g}, {g})")
        }
    }
}

fn apply_sgr(style: &Style, params: &[u32]) -> Style {
    let mut s = style.clone();
    let params: &[u32] = if params.is_empty() { &[0] } else { params };
    let mut i = 0;
    while i < params.len() {
        let p = params[i];
        match p {
            0 => s = Style::default(),
            1 => s.bold = true,
            2 => s.dim = true,
            3 => s.italic = true,
            4 => s.underline = true,
            7 => s.inverse = true,
            22 => {
                s.bold = false;
                s.dim = false;
            }
            23 => s.italic = false,
            24 => s.underline = false,
            27 => s.inverse = false,
            30..=37 => s.fg = Some(named(p - 30, false)),
            90..=97 => s.fg = Some(named(p - 90, true)),
            40..=47 => s.bg = Some(named(p - 40, false)),
            100..=107 => s.bg = Some(named(p - 100, true)),
            39 => s.fg = None,
            49 => s.bg = None,
            38 | 48 => {
                let color = match (params.get(i + 1), params.get(i + 2)) {
                    (Some(&5), Some(&n)) => {
                        i += 2;
                        Some(color256(n))
                    }
                    (Some(&2), _) if params.len() > i + 4 => {
                        let c = format!(
                            "rgb({}, {}, {})",
                            params[i + 2],
                            params[i + 3],
                            params[i + 4]
                        );
                        i += 4;
                        Some(c)
                    }
                    _ => None,
                };
                if let Some(c) = color {
                    if p == 38 {
                        s.fg = Some(c);
                    } else {
                        s.bg = Some(c);
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    s
}

/// Incremental parser: feed chunks as they arrive; escape sequences split across chunks are
/// kept until complete. Segments may contain `\n` and `\r`.
#[derive(Debug, Default)]
pub struct AnsiParser {
    style: Style,
    pending: String,
}

impl AnsiParser {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, chunk: &str) -> Vec<Segment> {
        let input: Vec<char> = std::mem::take(&mut self.pending)
            .chars()
            .chain(chunk.chars())
            .collect();
        let mut out = Vec::new();
        let mut text = String::new();
        let flush = |text: &mut String, out: &mut Vec<Segment>, style: &Style| {
            if !text.is_empty() {
                out.push(Segment {
                    text: std::mem::take(text),
                    style: style.clone(),
                });
            }
        };
        let mut i = 0;
        while i < input.len() {
            let c = input[i];
            if c != '\x1b' {
                // Drop other C0 controls except newline, carriage return and tab.
                if c >= ' ' || c == '\n' || c == '\r' || c == '\t' {
                    text.push(c);
                }
                i += 1;
                continue;
            }
            if i + 1 >= input.len() {
                self.pending = input[i..].iter().collect();
                break;
            }
            match input[i + 1] {
                '[' => {
                    // CSI: parameters, then a final byte in @..~
                    let mut j = i + 2;
                    while j < input.len() && !('@'..='~').contains(&input[j]) {
                        j += 1;
                    }
                    if j >= input.len() {
                        self.pending = input[i..].iter().collect();
                        break;
                    }
                    if input[j] == 'm' {
                        flush(&mut text, &mut out, &self.style);
                        let raw: String = input[i + 2..j].iter().collect();
                        let params: Vec<u32> = raw
                            .split([';', ':'])
                            .filter(|p| !p.is_empty())
                            .filter_map(|p| p.parse().ok())
                            .collect();
                        self.style = apply_sgr(&self.style, &params);
                    }
                    i = j + 1;
                }
                ']' => {
                    // OSC: up to BEL or ESC \
                    let mut end = None;
                    let mut j = i + 2;
                    while j < input.len() {
                        if input[j] == '\x07' {
                            end = Some(j + 1);
                            break;
                        }
                        if input[j] == '\x1b' && input.get(j + 1) == Some(&'\\') {
                            end = Some(j + 2);
                            break;
                        }
                        j += 1;
                    }
                    match end {
                        Some(e) => i = e,
                        None => {
                            self.pending = input[i..].iter().collect();
                            break;
                        }
                    }
                }
                _ => i += 2,
            }
        }
        flush(&mut text, &mut out, &self.style);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(segs: &[Segment]) -> Vec<&str> {
        segs.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn parses_colors_and_resets() {
        let mut p = AnsiParser::new();
        let segs = p.feed("plain \x1b[1;31mred bold\x1b[0m done");
        assert_eq!(texts(&segs), ["plain ", "red bold", " done"]);
        assert_eq!(
            segs[1].style,
            Style {
                bold: true,
                ..Style::fg("var(--ansi-red)")
            }
        );
        assert_eq!(segs[2].style, Style::default());
        assert_eq!(
            serde_json::to_string(&segs[1].style).unwrap(),
            r#"{"fg":"var(--ansi-red)","bold":true}"#
        );
    }

    #[test]
    fn keeps_escape_sequences_split_across_chunks() {
        let mut p = AnsiParser::new();
        assert_eq!(texts(&p.feed("a\x1b[3")), ["a"]);
        let segs = p.feed("2mgreen");
        assert_eq!(
            segs[0],
            Segment {
                text: "green".into(),
                style: Style::fg("var(--ansi-green)")
            }
        );
        assert!(p.feed("\x1b").is_empty());
        assert_eq!(texts(&p.feed("]0;t\x07x")), ["x"]);
    }

    #[test]
    fn supports_256_colors_truecolor_and_drops_osc_and_cursor_sequences() {
        let mut p = AnsiParser::new();
        let segs = p.feed("\x1b]0;title\x07\x1b[2K\x1b[38;5;196mx\x1b[48;2;1;2;3my");
        assert_eq!(segs[0].style.fg.as_deref(), Some("rgb(255, 0, 0)"));
        assert_eq!(segs[1].style.bg.as_deref(), Some("rgb(1, 2, 3)"));
        assert_eq!(texts(&segs).concat(), "xy");
        assert_eq!(color256(9), "var(--ansi-bright-red)");
        assert_eq!(color256(232), "rgb(8, 8, 8)");
    }

    #[test]
    fn drops_control_characters() {
        let mut p = AnsiParser::new();
        assert_eq!(texts(&p.feed("a\x07b\tc")), ["ab\tc"]);
    }
}
