//! Emmet as in PHPStorm: abbreviations that expand into HTML, JSX or CSS with the expand key
//! (Tab by default), Surround with Emmet, Go to Next/Previous Edit Point, Update Tag with
//! Emmet and Remove Tag.
//!
//! The crate is pure (no file system or clock) so it also runs as WebAssembly in the UI,
//! which calls it synchronously from the expand key. Everything the UI sends and receives
//! uses UTF-16 offsets, like CodeMirror; conversion happens in this module.
//!
//! - [`abbr`]: the abbreviation parser (`ul>li.item$*3`).
//! - [`markup`]: HTML/JSX output (snippets, implicit tags, numbering, lorem, formatting).
//! - [`css`]: CSS property abbreviations (`m10`, `df`, `bgc#fff`).
//! - [`context`]: which syntax applies at the caret and where the abbreviation starts.
//! - [`html`]: a tolerant tag scanner for the actions.
//! - [`actions`]: edit points, Update Tag, Remove Tag.

pub mod abbr;
pub mod actions;
pub mod context;
pub mod css;
pub mod html;
pub mod lorem;
pub mod markup;
pub mod snippets;
mod text;

use context::{family, Family, Syntax};
use markup::{Flavor, MarkupOptions, Output};
use serde::{Deserialize, Serialize};
use text::{byte_of, dedent, line_end, line_indent, line_start, utf16_of};

/// The `editor.emmet.*` settings and the editor's indent unit.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EmmetOptions {
    /// Emmet in HTML-like files (HTML, Blade, Twig, Vue templates, PHP outside `<?php`).
    pub html: bool,
    /// Emmet in CSS, SCSS, Less, Sass and Stylus.
    pub css: bool,
    /// Emmet in JSX / TSX.
    pub jsx: bool,
    /// Attribute for `.class` in JSX: `className` (default) or `class`.
    pub jsx_class_attr: String,
    /// Wrapping a URL or e-mail address with `a` fills `href`.
    pub href_recognition: bool,
    pub indent: String,
}

impl Default for EmmetOptions {
    fn default() -> Self {
        Self {
            html: true,
            css: true,
            jsx: true,
            jsx_class_attr: "className".into(),
            href_recognition: true,
            indent: "\t".into(),
        }
    }
}

impl EmmetOptions {
    fn markup(&self, flavor: Flavor) -> MarkupOptions {
        MarkupOptions {
            flavor,
            jsx_class_attr: self.jsx_class_attr.clone(),
            indent: self.indent.clone(),
            href_recognition: self.href_recognition,
        }
    }

    fn css(&self, semicolons: bool) -> css::CssOptions {
        css::CssOptions {
            indent: self.indent.clone(),
            semicolons,
        }
    }

    fn enabled(&self, f: Family) -> bool {
        match f {
            Family::Html => self.html,
            Family::Jsx => self.jsx,
            Family::Css { .. } => self.css,
        }
    }
}

/// Expand the abbreviation that ends at `offset`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpandRequest {
    pub text: String,
    pub offset: usize,
    #[serde(default)]
    pub language_id: Option<String>,
    #[serde(default)]
    pub options: EmmetOptions,
    /// For the abbreviation preview: only abbreviations that are clearly Emmet (an operator,
    /// attributes, a known element or snippet) expand.
    #[serde(default)]
    pub preview: bool,
}

/// A tab stop of an expansion: UTF-16 offsets inside `text`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Field {
    pub from: usize,
    pub to: usize,
    pub index: usize,
}

/// Replace `from..to` of the document with `text` (the same shape as a live template
/// expansion, so the editor inserts both as snippets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Expansion {
    pub from: usize,
    pub to: usize,
    pub text: String,
    pub fields: Vec<Field>,
    /// Where the caret ends after the last field (UTF-16 offset inside `text`).
    pub end: usize,
    pub abbreviation: String,
    /// `html`, `jsx` or `css`.
    pub syntax: &'static str,
}

fn expansion(
    text: &str,
    from: usize,
    to: usize,
    out: Output,
    abbreviation: &str,
    syntax: &'static str,
) -> Expansion {
    let end = out.text.encode_utf16().count();
    Expansion {
        from: utf16_of(text, from),
        to: utf16_of(text, to),
        fields: out
            .stops
            .iter()
            .map(|s| Field {
                from: s.from,
                to: s.to,
                index: s.index,
            })
            .collect(),
        text: out.text,
        end,
        abbreviation: abbreviation.to_string(),
        syntax,
    }
}

fn syntax_name(s: Syntax) -> &'static str {
    match s {
        Syntax::Markup(Flavor::Html) => "html",
        Syntax::Markup(Flavor::Jsx) => "jsx",
        Syntax::Css { .. } => "css",
    }
}

/// The expansion of the abbreviation before the caret, or `None` when there is none (the
/// key then does what it always does).
pub fn expand(req: &ExpandRequest) -> Option<Expansion> {
    let language = req.language_id.as_deref()?;
    let fam = family(language)?;
    if !req.options.enabled(fam) {
        return None;
    }
    let text = &req.text;
    let end = byte_of(text, req.offset);
    // Only at the end of a word: `div|>` or `div|x` is not an abbreviation end.
    if text[end..]
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    let in_style =
        fam == Family::Html && matches!(html::state_at(text, end), html::State::Style(_));
    let start = match fam {
        Family::Css { .. } => context::extract_css(text, end),
        _ if in_style => context::extract_css(text, end),
        _ => context::extract_markup(text, end),
    }?;
    let syntax = context::syntax_at(text, end, language, start)?;
    if matches!(syntax, Syntax::Css { .. }) && !req.options.css {
        return None;
    }
    let abbr = &text[start..end];
    let out = match syntax {
        Syntax::Css { semicolons } => {
            if req.preview
                && !abbr.contains(|c: char| c.is_ascii_digit() || c == '#' || c == ':' || c == '+')
                && abbr.len() < 2
            {
                return None;
            }
            css::expand(abbr, &req.options.css(semicolons)).ok()?
        }
        Syntax::Markup(flavor) => {
            let before = text[line_start(text, start)..start].chars().next_back();
            let ok_before = match flavor {
                Flavor::Html => before.is_none_or(|c| c.is_whitespace() || c == '>'),
                Flavor::Jsx => context::jsx_position(text, start),
            };
            if !ok_before {
                return None;
            }
            let tree = abbr::parse(abbr).ok()?;
            if (flavor == Flavor::Jsx || req.preview) && !markup::looks_like_markup(&tree) {
                return None;
            }
            markup::expand(abbr, &req.options.markup(flavor), None).ok()?
        }
    };
    Some(expansion(text, start, end, out, abbr, syntax_name(syntax)))
}

/// Expand an abbreviation on its own (the preview in the Surround with Emmet popup).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WrapRequest {
    pub text: String,
    pub from: usize,
    pub to: usize,
    pub abbreviation: String,
    #[serde(default)]
    pub language_id: Option<String>,
    #[serde(default)]
    pub options: EmmetOptions,
}

/// Surround with Emmet: the selection (or the current line) wrapped with an abbreviation.
pub fn wrap(req: &WrapRequest) -> Result<Expansion, String> {
    let text = &req.text;
    let flavor = match req.language_id.as_deref().and_then(family) {
        Some(Family::Jsx) => Flavor::Jsx,
        Some(Family::Css { .. }) => return Err("Surround with Emmet works in markup".into()),
        _ => Flavor::Html,
    };
    let (mut from, mut to) = (
        byte_of(text, req.from.min(req.to)),
        byte_of(text, req.from.max(req.to)),
    );
    if from == to {
        // No selection: the current line without its indentation.
        from = line_start(text, from) + line_indent(text, from).len();
        to = line_end(text, to);
    }
    // Keep the indentation of the first line and trailing whitespace outside.
    let ls = line_start(text, from);
    if text[ls..from].trim().is_empty() {
        from = ls + line_indent(text, from).len();
    }
    while to > from && text[..to].ends_with(char::is_whitespace) {
        to -= text[..to].chars().next_back().map_or(1, char::len_utf8);
    }
    if from >= to {
        return Err("Nothing to surround".into());
    }
    // The first line's indentation counts for the common indentation of the lines.
    let ls = line_start(text, from);
    let content_from = if text[ls..from].trim().is_empty() {
        ls
    } else {
        from
    };
    let lines = dedent(&text[content_from..to]);
    let abbr = req.abbreviation.trim();
    let out = markup::expand(abbr, &req.options.markup(flavor), Some(&lines))?;
    Ok(expansion(
        text,
        from,
        to,
        out,
        abbr,
        if flavor == Flavor::Jsx { "jsx" } else { "html" },
    ))
}

/// Go to Next/Previous Edit Point.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditPointRequest {
    pub text: String,
    pub offset: usize,
    pub forward: bool,
}

pub fn edit_point(req: &EditPointRequest) -> Option<usize> {
    let byte = byte_of(&req.text, req.offset);
    actions::edit_point(&req.text, byte, req.forward).map(|b| utf16_of(&req.text, b))
}

/// A change for the editor (UTF-16 offsets).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Edit {
    pub from: usize,
    pub to: usize,
    pub text: String,
}

fn edits(text: &str, list: Vec<actions::ByteEdit>) -> Vec<Edit> {
    list.into_iter()
        .map(|e| Edit {
            from: utf16_of(text, e.from),
            to: utf16_of(text, e.to),
            text: e.text,
        })
        .collect()
}

/// Update Tag with Emmet / Remove Tag at `offset`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagRequest {
    pub text: String,
    pub offset: usize,
    #[serde(default)]
    pub abbreviation: String,
    #[serde(default)]
    pub options: EmmetOptions,
}

pub fn update_tag(req: &TagRequest) -> Result<Vec<Edit>, String> {
    let byte = byte_of(&req.text, req.offset);
    actions::update_tag(&req.text, byte, &req.abbreviation).map(|e| edits(&req.text, e))
}

pub fn remove_tag(req: &TagRequest) -> Result<Vec<Edit>, String> {
    let byte = byte_of(&req.text, req.offset);
    actions::remove_tag(&req.text, byte, &req.options.indent).map(|e| edits(&req.text, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(text: &str, lang: &str) -> ExpandRequest {
        ExpandRequest {
            text: text.into(),
            offset: text.encode_utf16().count(),
            language_id: Some(lang.into()),
            options: EmmetOptions {
                indent: "  ".into(),
                ..EmmetOptions::default()
            },
            preview: false,
        }
    }

    fn exp(text: &str, lang: &str) -> Option<String> {
        expand(&req(text, lang)).map(|e| e.text)
    }

    #[test]
    fn expands_in_html_like_files() {
        let e = expand(&req("<body>\n  ul>li*2", "html")).unwrap();
        assert_eq!(e.text, "<ul>\n  <li></li>\n  <li></li>\n</ul>");
        assert_eq!((e.from, e.to, e.syntax), (9, 16, "html"));
        assert_eq!(e.abbreviation, "ul>li*2");
        assert_eq!(e.end, e.text.encode_utf16().count());
        assert_eq!(
            exp("<p>a.btn", "blade").as_deref(),
            Some("<a href=\"\" class=\"btn\"></a>")
        );
        assert_eq!(
            exp("{% block x %}\n.box", "twig").as_deref(),
            Some("<div class=\"box\"></div>")
        );
        assert_eq!(
            exp("<?php $a = 1; ?>\nsection", "php").as_deref(),
            Some("<section></section>")
        );
        assert_eq!(
            exp("<template>\n  .card", "vue").as_deref(),
            Some("<div class=\"card\"></div>")
        );
    }

    #[test]
    fn leaves_other_places_alone() {
        assert_eq!(exp("<?php\n$x = div", "php"), None);
        assert_eq!(exp("<div cla", "html"), None);
        assert_eq!(exp("<p>{{ $user", "blade"), None);
        assert_eq!(exp("@foreach", "blade"), None);
        assert_eq!(exp("text ", "html"), None);
        assert_eq!(exp("$user", "php"), None);
        assert_eq!(exp("div", "python"), None);
        let mut r = req("<p>div", "html");
        r.options.html = false;
        assert!(expand(&r).is_none());
        // Not at the end of a word.
        let mut r = req("<p>divx", "html");
        r.offset = 6;
        assert!(expand(&r).is_none());
    }

    #[test]
    fn expands_css() {
        assert_eq!(exp("a {\n  m10", "css").as_deref(), Some("margin: 10px;"));
        assert_eq!(exp(".a\n  df", "sass").as_deref(), Some("display: flex"));
        assert_eq!(exp("a { color: m10", "css"), None);
        assert_eq!(exp(".b", "css"), None);
        assert_eq!(
            exp("<style>\n  a { bgc#000", "html").as_deref(),
            Some("background-color: #000;")
        );
        let mut r = req("a { m10", "scss");
        r.options.css = false;
        assert!(expand(&r).is_none());
    }

    #[test]
    fn expands_jsx() {
        assert_eq!(
            exp("return (\n  div.card", "tsx").as_deref(),
            Some("<div className=\"card\"></div>")
        );
        assert_eq!(exp("const div", "jsx"), None);
        assert_eq!(exp("return (\n  foo", "jsx"), None);
        let mut r = req("return (\n  .a", "jsx");
        r.options.jsx_class_attr = "class".into();
        assert_eq!(expand(&r).unwrap().text, "<div class=\"a\"></div>");
    }

    #[test]
    fn preview_needs_clear_abbreviations() {
        let mut r = req("<p>foo", "html");
        assert!(expand(&r).is_some());
        r.preview = true;
        assert!(expand(&r).is_none());
        let mut r = req("<p>ul>li", "html");
        r.preview = true;
        assert!(expand(&r).is_some());
    }

    #[test]
    fn utf16_offsets() {
        let e = expand(&req("<p>é😀 a{ü}", "html")).unwrap();
        assert_eq!((e.from, e.to), (7, 11));
        assert_eq!(e.text, "<a href=\"\">ü</a>");
        assert_eq!(e.fields[0].from, 9);
    }

    #[test]
    fn wraps_selection_and_lines() {
        let text = "<body>\n  <p>One</p>\n  <p>Two</p>\n</body>";
        let from = text.find("<p>One").unwrap();
        let to = text.find("</body>").unwrap();
        let e = wrap(&WrapRequest {
            text: text.into(),
            from,
            to,
            abbreviation: "div.box".into(),
            language_id: Some("html".into()),
            options: EmmetOptions {
                indent: "  ".into(),
                ..EmmetOptions::default()
            },
        })
        .unwrap();
        assert_eq!(
            e.text,
            "<div class=\"box\">\n  <p>One</p>\n  <p>Two</p>\n</div>"
        );
        assert_eq!((e.from, e.to), (from, to - 1));
        // No selection: the current line.
        let text = "<ul>\n  Home\n</ul>";
        let e = wrap(&WrapRequest {
            text: text.into(),
            from: 9,
            to: 9,
            abbreviation: "li>a".into(),
            language_id: Some("html".into()),
            options: EmmetOptions::default(),
        })
        .unwrap();
        assert_eq!(e.text, "<li><a href=\"\">Home</a></li>");
        assert_eq!((e.from, e.to), (7, 11));
        assert!(wrap(&WrapRequest {
            text: "a { }".into(),
            from: 0,
            to: 5,
            abbreviation: "div".into(),
            language_id: Some("css".into()),
            options: EmmetOptions::default(),
        })
        .is_err());
    }

    #[test]
    fn actions_use_utf16() {
        let text = "é<a href=\"\"></a>";
        let p = edit_point(&EditPointRequest {
            text: text.into(),
            offset: 0,
            forward: true,
        });
        assert_eq!(p, Some(10));
        let e = update_tag(&TagRequest {
            text: text.into(),
            offset: 3,
            abbreviation: "b".into(),
            options: EmmetOptions::default(),
        })
        .unwrap();
        assert_eq!(
            e[0],
            Edit {
                from: 2,
                to: 3,
                text: "b".into()
            }
        );
        let e = remove_tag(&TagRequest {
            text: text.into(),
            offset: 3,
            abbreviation: String::new(),
            options: EmmetOptions::default(),
        })
        .unwrap();
        assert_eq!(e.len(), 2);
    }

    #[test]
    fn options_deserialize_with_defaults() {
        let r: ExpandRequest = serde_json::from_str(
            r#"{"text":"<p>a","offset":4,"languageId":"html","options":{"indent":"  "}}"#,
        )
        .unwrap();
        assert!(r.options.html && r.options.jsx_class_attr == "className");
        assert_eq!(r.options.indent, "  ");
        assert!(expand(&r).is_some());
    }
}
