//! Finding color values in a document: CSS declarations (CSS, SCSS, Less, Sass, Stylus,
//! `<style>` blocks and `style` attributes), string literals of scripts and Vue/JSX
//! components, and Tailwind classes (`bg-blue-500`, `text-white/50`) in class attributes and
//! strings.

use serde::Serialize;

use crate::color::{parse, Rgba};
use crate::named::{named_color, tailwind_color};

/// A color in the document (UTF-16 offsets).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ColorMatch {
    pub from: usize,
    pub to: usize,
    pub color: Rgba,
    /// `#rrggbb` / `#rrggbbaa`, for the swatch.
    pub hex: String,
    /// The color text as written.
    pub text: String,
    /// Whether the color picker may write the color back (not for Tailwind classes).
    pub editable: bool,
}

const MAX_TEXT: usize = 4 * 1024 * 1024;
const MAX_MATCHES: usize = 5000;

struct Found {
    from: usize,
    to: usize,
    color: Rgba,
    editable: bool,
}

/// All colors of a document in `language` (a Forge file type id).
pub fn find_colors(text: &str, language: &str) -> Vec<ColorMatch> {
    if text.len() > MAX_TEXT {
        return Vec::new();
    }
    let mut out = Vec::new();
    match language {
        "css" => scan_css(text, 0, text.len(), false, false, &mut out),
        "scss" | "less" => scan_css(text, 0, text.len(), false, true, &mut out),
        "sass" | "stylus" => scan_css(text, 0, text.len(), true, true, &mut out),
        "html" | "xhtml" | "vue" | "blade" | "twig" | "php" | "smarty" | "latte" | "jinja"
        | "handlebars" | "gotemplate" | "svelte" | "astro" => scan_markup(text, &mut out),
        "javascript" | "typescript" | "jsx" | "tsx" => scan_js(text, 0, text.len(), &mut out),
        _ => {}
    }
    out.sort_by_key(|f| (f.from, f.to));
    let mut last_end = 0;
    out.retain(|f| {
        let keep = f.from >= last_end;
        if keep {
            last_end = f.to;
        }
        keep
    });
    out.truncate(MAX_MATCHES);
    to_utf16(text, out)
}

fn to_utf16(text: &str, found: Vec<Found>) -> Vec<ColorMatch> {
    let mut units = 0;
    let mut byte = 0;
    let mut at = |target: usize| {
        units += text[byte..target].encode_utf16().count();
        byte = target;
        units
    };
    found
        .into_iter()
        .map(|f| {
            let from = at(f.from);
            let to = at(f.to);
            ColorMatch {
                from,
                to,
                hex: f.color.hex(),
                color: f.color,
                text: text[f.from..f.to].to_string(),
                editable: f.editable,
            }
        })
        .collect()
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

fn char_len(text: &str, i: usize) -> usize {
    text[i..].chars().next().map_or(1, char::len_utf8)
}

/// End of a parenthesized group starting at `open` (`(`), or `to`.
fn close_paren(b: &[u8], open: usize, to: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, &c) in b.iter().enumerate().take(to).skip(open) {
        match c {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'\n' | b';' | b'{' | b'}' => return None,
            _ => {}
        }
    }
    None
}

/// Colors in a CSS value (or any text): hex, color functions and, with `named`, color names.
fn scan_value(text: &str, from: usize, to: usize, named: bool, out: &mut Vec<Found>) {
    let b = text.as_bytes();
    let mut i = from;
    while i < to {
        let c = b[i];
        match c {
            b'"' | b'\'' if named => {
                // Strings in CSS values (`content: "red"`) are not colors.
                i += 1;
                while i < to && b[i] != c {
                    i += 1;
                }
                i += 1;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = text[i..to].find("*/").map_or(to, |e| i + e + 2);
            }
            b'#' => {
                let mut j = i + 1;
                while j < to && b[j].is_ascii_hexdigit() {
                    j += 1;
                }
                let prev_ok = i == 0 || !(is_word(b[i - 1]) || b[i - 1] == b'&');
                let next_ok = j >= b.len() || !is_word(b[j]);
                if matches!(j - i - 1, 3 | 4 | 6 | 8) && prev_ok && next_ok {
                    if let Some((color, _)) = parse(&text[i..j]) {
                        out.push(Found {
                            from: i,
                            to: j,
                            color,
                            editable: true,
                        });
                    }
                }
                i = j.max(i + 1);
            }
            c if c.is_ascii_alphabetic() => {
                let mut j = i;
                while j < to && is_word(b[j]) {
                    j += 1;
                }
                let word = &text[i..j];
                let prev = if i > 0 { b[i - 1] } else { b' ' };
                if j < to && b[j] == b'(' {
                    let lower = word.to_ascii_lowercase();
                    if matches!(lower.as_str(), "rgb" | "rgba" | "hsl" | "hsla" | "hwb") {
                        if let Some(k) = close_paren(b, j, to) {
                            if let Some((color, _)) = parse(&text[i..=k]) {
                                out.push(Found {
                                    from: i,
                                    to: k + 1,
                                    color,
                                    editable: true,
                                });
                            }
                            i = k + 1;
                            continue;
                        }
                    } else if lower == "url" || lower == "var" {
                        i = close_paren(b, j, to).map_or(j + 1, |k| k + 1);
                        continue;
                    }
                    i = j + 1;
                    continue;
                }
                if named && !matches!(prev, b'$' | b'@' | b'-' | b'.' | b'#' | b'%' | b'_') {
                    if let Some(color) = named_color(word) {
                        out.push(Found {
                            from: i,
                            to: j,
                            color,
                            editable: true,
                        });
                    }
                }
                i = j;
            }
            _ => i += char_len(text, i),
        }
    }
}

/// Declarations in CSS (`prop: value` segments between `{`, `;` and `}`; selectors before
/// `{` are skipped). `indented` reads Sass/Stylus lines; `line_comments` skips `//`.
fn scan_css(
    text: &str,
    from: usize,
    to: usize,
    indented: bool,
    line_comments: bool,
    out: &mut Vec<Found>,
) {
    let b = text.as_bytes();
    if indented {
        let mut ls = from;
        while ls < to {
            let le = text[ls..to].find('\n').map_or(to, |e| ls + e);
            let line = &text[ls..le];
            let body_end = line.find("//").map_or(le, |c| ls + c);
            let trimmed = line.trim_start();
            let decl = trimmed.starts_with('$')
                || trimmed.starts_with("--")
                || line.find(": ").is_some_and(|c| ls + c < body_end);
            if decl && !trimmed.starts_with("//") {
                if let Some(c) = text[ls..body_end].find(':') {
                    scan_value(text, ls + c + 1, body_end, true, out);
                }
            }
            ls = le + 1;
        }
        return;
    }
    let mut seg = from;
    let mut i = from;
    let mut parens = 0;
    let declaration = |s: usize, e: usize, out: &mut Vec<Found>| {
        let part = &text[s..e];
        let trimmed = part.trim_start();
        if trimmed.starts_with('@') {
            scan_value(text, s, e, false, out);
        } else if let Some(c) = part.find(':') {
            scan_value(text, s + c + 1, e, true, out);
        }
    };
    while i < to {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = text[i..to].find("*/").map_or(to, |e| i + e + 2);
                continue;
            }
            b'/' if line_comments
                && b.get(i + 1) == Some(&b'/')
                && (i == 0
                    || b[i - 1].is_ascii_whitespace()
                    || matches!(b[i - 1], b';' | b'{' | b'}')) =>
            {
                declaration(seg, i, out);
                i = text[i..to].find('\n').map_or(to, |e| i + e);
                seg = i;
                continue;
            }
            q @ (b'"' | b'\'') => {
                i += 1;
                while i < to && b[i] != q && b[i] != b'\n' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'(' => parens += 1,
            b')' => parens -= 1,
            b'{' if parens <= 0 => seg = i + 1,
            b';' | b'}' if parens <= 0 => {
                declaration(seg, i, out);
                seg = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    if seg < to {
        declaration(seg, to.min(text.len()), out);
    }
}

/// Utilities that take a color in Tailwind CSS (longest first).
const TAILWIND_UTILITIES: &[&str] = &[
    "ring-offset",
    "placeholder",
    "decoration",
    "border-x",
    "border-y",
    "border-t",
    "border-r",
    "border-b",
    "border-l",
    "border-s",
    "border-e",
    "outline",
    "divide",
    "accent",
    "border",
    "shadow",
    "stroke",
    "caret",
    "fill",
    "from",
    "text",
    "ring",
    "via",
    "bg",
    "to",
];

/// Tailwind color classes in a class list or string.
fn scan_tailwind(text: &str, from: usize, to: usize, out: &mut Vec<Found>) {
    let mut i = from;
    let b = text.as_bytes();
    while i < to {
        while i < to && (b[i].is_ascii_whitespace() || matches!(b[i], b'\'' | b'"' | b'`' | b',')) {
            i += 1;
        }
        let s = i;
        while i < to && !(b[i].is_ascii_whitespace() || matches!(b[i], b'\'' | b'"' | b'`' | b','))
        {
            i += 1;
        }
        let token = &text[s..i];
        if token.contains('[') || token.is_empty() {
            continue;
        }
        let start = s + token.rfind(':').map_or(0, |c| c + 1);
        let class = text[start..i].trim_start_matches(['!', '-']);
        let start = i - class.len();
        let Some(color) = tailwind_class(class) else {
            continue;
        };
        out.push(Found {
            from: start,
            to: i,
            color,
            editable: false,
        });
    }
}

fn tailwind_class(class: &str) -> Option<Rgba> {
    let util = TAILWIND_UTILITIES
        .iter()
        .find(|u| class.starts_with(*u) && class.as_bytes().get(u.len()) == Some(&b'-'))?;
    let rest = &class[util.len() + 1..];
    let (body, opacity) = match rest.split_once('/') {
        Some((b, o)) => (b, Some(o.parse::<f64>().ok()?)),
        None => (rest, None),
    };
    let mut color = match body.rsplit_once('-') {
        Some((name, shade)) => tailwind_color(name, Some(shade.parse().ok()?))?,
        None => tailwind_color(body, None)?,
    };
    if let Some(o) = opacity {
        color.a = (o / 100.0).clamp(0.0, 1.0);
    }
    Some(color)
}

/// String literals of JavaScript / TypeScript (and JSX attribute values).
fn scan_js(text: &str, from: usize, to: usize, out: &mut Vec<Found>) {
    let b = text.as_bytes();
    let mut i = from;
    while i < to {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                i = text[i..to].find('\n').map_or(to, |e| i + e);
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = text[i..to].find("*/").map_or(to, |e| i + e + 2);
            }
            q @ (b'"' | b'\'' | b'`') => {
                let s = i + 1;
                let mut j = s;
                while j < to && b[j] != q && (q == b'`' || b[j] != b'\n') {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                let e = j.min(to);
                scan_string(text, s, e, out);
                i = e + 1;
            }
            _ => i += char_len(text, i),
        }
    }
}

fn scan_string(text: &str, from: usize, to: usize, out: &mut Vec<Found>) {
    let content = &text[from..to];
    let trimmed = content.trim();
    if !trimmed.is_empty() && trimmed.len() < 30 {
        if let Some(color) = named_color(trimmed) {
            let s = from + (content.len() - content.trim_start().len());
            out.push(Found {
                from: s,
                to: s + trimmed.len(),
                color,
                editable: true,
            });
            return;
        }
    }
    scan_value(text, from, to, false, out);
    scan_tailwind(text, from, to, out);
}

/// HTML-like documents: `<style>` blocks, `style` attributes, class attributes (Tailwind)
/// and `<script>` blocks; PHP blocks and comments are skipped.
fn scan_markup(text: &str, out: &mut Vec<Found>) {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        if text[i..].starts_with("<!--") {
            i = text[i..].find("-->").map_or(b.len(), |e| i + e + 3);
            continue;
        }
        if text[i..].starts_with("<?") {
            i = text[i..].find("?>").map_or(b.len(), |e| i + e + 2);
            continue;
        }
        if !b.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
            i += 1;
            continue;
        }
        let name_end = (i + 1..b.len())
            .find(|&j| !(is_word(b[j]) || b[j] == b':' || b[j] == b'.'))
            .unwrap_or(b.len());
        let name = text[i + 1..name_end].to_ascii_lowercase();
        let tag_start = i;
        // Attributes up to `>`.
        let mut j = name_end;
        let mut tag_end = b.len();
        while j < b.len() {
            match b[j] {
                b'>' => {
                    tag_end = j + 1;
                    break;
                }
                b'<' => {
                    tag_end = j;
                    break;
                }
                c if c.is_ascii_whitespace() || c == b'/' => j += 1,
                _ => {
                    let ns = j;
                    while j < b.len()
                        && !b[j].is_ascii_whitespace()
                        && !matches!(b[j], b'=' | b'>' | b'/')
                    {
                        j += 1;
                    }
                    let attr = text[ns..j].to_ascii_lowercase();
                    if j < b.len() && b[j] == b'=' {
                        j += 1;
                        let (vs, ve);
                        if j < b.len() && (b[j] == b'"' || b[j] == b'\'') {
                            let q = b[j];
                            vs = j + 1;
                            ve = text[vs..].find(q as char).map_or(b.len(), |e| vs + e);
                            j = (ve + 1).min(b.len());
                        } else if j < b.len() && b[j] == b'{' {
                            // JSX-style `className={…}` in markup: strings inside.
                            vs = j;
                            ve = text[j..].find('}').map_or(b.len(), |e| j + e);
                            scan_js(text, vs, ve, out);
                            j = (ve + 1).min(b.len());
                            continue;
                        } else {
                            vs = j;
                            while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != b'>' {
                                j += 1;
                            }
                            ve = j;
                        }
                        match attr.as_str() {
                            "style" | ":style" => scan_css(text, vs, ve, false, false, out),
                            "class" | "classname" | ":class" | "v-bind:class" => {
                                scan_tailwind(text, vs, ve, out)
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        i = tag_end;
        if (name == "style" || name == "script") && !text[..tag_end].ends_with("/>") {
            let close = format!("</{name}");
            let end = text[tag_end..]
                .to_ascii_lowercase()
                .find(&close)
                .map_or(b.len(), |e| tag_end + e);
            if name == "style" {
                let lang = &text[tag_start..tag_end];
                let indented = lang.contains("lang=\"sass\"") || lang.contains("lang=\"stylus\"");
                scan_css(text, tag_end, end, indented, true, out);
            } else {
                scan_js(text, tag_end, end, out);
            }
            i = end;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str, lang: &str) -> Vec<String> {
        find_colors(text, lang)
            .into_iter()
            .map(|m| m.text)
            .collect()
    }

    #[test]
    fn css_declarations() {
        let css = "/* red */\n#main, a:hover { color: red; background: #FFF url(x.png); }\n\
                   .b { border: 1px solid rgba(0, 0, 0, .5); box-shadow: 0 0 2px hsl(200 50% 50% / 40%); }\n\
                   :root { --brand: #1e90ff; } .c { color: var(--red); content: \"blue\"; }";
        assert_eq!(
            found(css, "css"),
            [
                "red",
                "#FFF",
                "rgba(0, 0, 0, .5)",
                "hsl(200 50% 50% / 40%)",
                "#1e90ff"
            ]
        );
    }

    #[test]
    fn scss_and_sass() {
        let scss = "$primary: #333;\n// tomato\n.a {\n  &:hover { color: darken($primary, 10%); }\n  background: linear-gradient(white, #000);\n}";
        assert_eq!(found(scss, "scss"), ["#333", "white", "#000"]);
        let sass = "$c: #abc\n.a\n  color: navy\n  &:hover\n    color: #fff // white";
        assert_eq!(found(sass, "sass"), ["#abc", "navy", "#fff"]);
    }

    #[test]
    fn markup_styles_attributes_and_tailwind() {
        let html = "<!-- #fff -->\n<div style=\"color: teal; margin: 0\" class=\"bg-blue-500 hover:text-white/50 p-4 bg-[#123456]\">red</div>\n\
                    <style>\n  p { color: #f00 }\n</style><?php $c = '#00ff00'; ?>";
        assert_eq!(
            found(html, "html"),
            ["teal", "bg-blue-500", "text-white/50", "#f00"]
        );
        let m = &find_colors(html, "html")[2];
        assert!(!m.editable);
        assert_eq!(m.hex, "#ffffff80");
        let vue = "<template><p :class=\"'text-red-600'\" /></template>\n<script>\nconst c = '#ff8800'\n</script>\n<style lang=\"scss\">\n.a { color: rgb(1 2 3) }\n</style>";
        assert_eq!(found(vue, "vue"), ["text-red-600", "#ff8800", "rgb(1 2 3)"]);
    }

    #[test]
    fn script_strings() {
        let js = "// '#fff'\nconst theme = { primary: '#1e90ff', bg: \"rgba(0,0,0,0.5)\", accent: 'tomato', id: '#a', css: `color: #abc;` };\n\
                  <div className=\"text-sky-400\" />";
        assert_eq!(
            found(js, "tsx"),
            [
                "#1e90ff",
                "rgba(0,0,0,0.5)",
                "tomato",
                "#abc",
                "text-sky-400"
            ]
        );
    }

    #[test]
    fn utf16_offsets() {
        let css = "a { content: '😀'; color: red }";
        let m = &find_colors(css, "css")[0];
        assert_eq!((m.from, m.to), (26, 29));
        assert_eq!(m.hex, "#ff0000");
    }

    #[test]
    fn other_languages_have_none() {
        assert!(find_colors("x = '#fff'", "python").is_empty());
    }
}
