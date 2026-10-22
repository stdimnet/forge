//! Where Emmet applies: the syntax at the caret (HTML, JSX, CSS) for a file's language and
//! the abbreviation that ends at the caret.

use crate::html::{state_at, State};
use crate::markup::Flavor;
use crate::text::line_start;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Syntax {
    Markup(Flavor),
    /// CSS; `semicolons` is false in indented syntaxes (Sass, Stylus).
    Css {
        semicolons: bool,
    },
}

/// Which family a language belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Html,
    Jsx,
    Css { semicolons: bool },
}

pub fn family(language: &str) -> Option<Family> {
    Some(match language {
        "html" | "xhtml" | "vue" | "blade" | "twig" | "smarty" | "latte" | "jinja"
        | "handlebars" | "gotemplate" | "php" => Family::Html,
        "jsx" | "tsx" | "javascript" => Family::Jsx,
        "css" | "scss" | "less" => Family::Css { semicolons: true },
        "sass" | "stylus" => Family::Css { semicolons: false },
        _ => return None,
    })
}

/// Template expressions where markup does not apply: `{{ }}`, `{% %}`, `{!! !!}`, `@php`.
fn in_template_code(text: &str, byte: usize, language: &str) -> bool {
    let before = &text[..byte];
    let unclosed = |open: &str, close: &str| {
        before
            .rfind(open)
            .is_some_and(|o| !before[o + open.len()..].contains(close))
    };
    match language {
        "blade" => unclosed("{{", "}}") || unclosed("{!!", "!!}") || unclosed("@php", "@endphp"),
        "twig" | "jinja" | "handlebars" | "vue" | "gotemplate" => {
            unclosed("{{", "}}") || unclosed("{%", "%}") || unclosed("{#", "#}")
        }
        "latte" | "smarty" => unclosed("{", "}"),
        _ => false,
    }
}

/// The syntax at `byte` for a file in `language`; `None` where Emmet does not apply
/// (inside a tag, comment, script, PHP code, template expression, CSS selector).
pub fn syntax_at(text: &str, byte: usize, language: &str, abbr_start: usize) -> Option<Syntax> {
    match family(language)? {
        Family::Css { semicolons } => {
            css_position(text, abbr_start, semicolons).then_some(Syntax::Css { semicolons })
        }
        Family::Jsx => Some(Syntax::Markup(Flavor::Jsx)),
        Family::Html => {
            if in_template_code(text, byte, language) {
                return None;
            }
            match state_at(text, byte) {
                State::Text => Some(Syntax::Markup(Flavor::Html)),
                State::Style(start) => css_position(&text[start..], abbr_start - start, true)
                    .then_some(Syntax::Css { semicolons: true }),
                _ => None,
            }
        }
    }
}

/// Whether `at` is where a declaration starts: inside a rule's braces with only whitespace
/// since the last `{`, `;` or `}` (or at the start of an indented line in Sass/Stylus).
/// At-rule abbreviations (`@m`) may also start at the top level.
pub fn css_position(css: &str, at: usize, semicolons: bool) -> bool {
    let at = at.min(css.len());
    if !semicolons {
        let start = line_start(css, at);
        let prefix = &css[start..at];
        return !prefix.is_empty() && prefix.trim().is_empty() || css[at..].starts_with('@');
    }
    let b = css.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;
    let mut last_delim = 0;
    while i < at {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'*') => {
                match css[i + 2..].find("*/") {
                    Some(e) => i += e + 4,
                    None => return false,
                }
                last_delim = i;
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                // SCSS / Less line comment.
                match css[i..].find('\n') {
                    Some(e) => i += e,
                    None => return false,
                }
                last_delim = i;
                continue;
            }
            q @ (b'"' | b'\'') => {
                i += 1;
                while i < at && b[i] != q {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                if i >= at {
                    return false;
                }
            }
            b'{' => {
                depth += 1;
                last_delim = i + 1;
            }
            b'}' => {
                depth -= 1;
                last_delim = i + 1;
            }
            b';' => last_delim = i + 1,
            _ => {}
        }
        i += 1;
    }
    let clean = css[last_delim.min(at)..at].trim().is_empty();
    clean && (depth > 0 || css[at..].starts_with('@'))
}

fn markup_char(c: char) -> bool {
    c.is_alphanumeric()
        || matches!(
            c,
            '-' | '_' | ':' | '.' | '#' | '*' | '$' | '@' | '+' | '^' | '!' | '/' | '%' | '>'
        )
}

/// Byte where the markup abbreviation that ends at `end` starts (Emmet's extraction: brackets
/// and braces are balanced, a `>` that closes a tag ends the abbreviation).
pub fn extract_markup(text: &str, end: usize) -> Option<usize> {
    let line = line_start(text, end);
    let prefix = &text[line..end];
    let chars: Vec<(usize, char)> = prefix.char_indices().collect();
    let mut i = chars.len();
    let (mut braces, mut brackets, mut parens) = (0, 0, 0);
    while i > 0 {
        let (pos, c) = chars[i - 1];
        if braces > 0 {
            match c {
                '}' => braces += 1,
                '{' => braces -= 1,
                _ => {}
            }
            i -= 1;
            continue;
        }
        if brackets > 0 {
            match c {
                ']' => brackets += 1,
                '[' => brackets -= 1,
                _ => {}
            }
            i -= 1;
            continue;
        }
        match c {
            '}' => braces = 1,
            ']' => brackets = 1,
            ')' => parens += 1,
            '(' if parens == 0 => break,
            '(' => parens -= 1,
            '>' if closes_tag(prefix, pos) => break,
            c if markup_char(c) => {}
            _ => break,
        }
        i -= 1;
    }
    // Unbalanced, or an unclosed `[` / `{` before it (`a[href|`).
    if braces > 0 || brackets > 0 || parens > 0 || (i > 0 && matches!(chars[i - 1].1, '[' | '{')) {
        return None;
    }
    let start = chars.get(i).map_or(prefix.len(), |(p, _)| *p);
    (start < prefix.len()).then_some(line + start)
}

/// Whether the `>` at `pos` ends a tag (`<div>`, `?>`, `-->`), not a child operator.
fn closes_tag(prefix: &str, pos: usize) -> bool {
    let before = &prefix[..pos];
    if before.ends_with('?') || before.ends_with("--") {
        return true;
    }
    match before.rfind(['<', '>']) {
        Some(i) if before.as_bytes()[i] == b'<' => before[i + 1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!'),
        _ => false,
    }
}

fn css_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(c, '-' | ':' | '#' | '.' | '%' | '!' | '+' | '@' | '$' | '_')
}

/// Byte where the CSS abbreviation that ends at `end` starts.
pub fn extract_css(text: &str, end: usize) -> Option<usize> {
    let line = line_start(text, end);
    let prefix = &text[line..end];
    let start = prefix
        .char_indices()
        .rev()
        .take_while(|(_, c)| css_char(*c))
        .last()
        .map(|(i, _)| i)?;
    let before = prefix[..start].chars().next_back();
    if !before.is_none_or(|c| c.is_whitespace() || c == '{' || c == ';') {
        return None;
    }
    let abbr = &prefix[start..];
    abbr.starts_with(|c: char| c.is_ascii_alphabetic() || c == '@')
        .then_some(line + start)
}

/// In JSX, markup only expands where an element can start: after `(`, `>`, `return`,
/// `=>`, `&&`, `||`, `?` or `:`, and not inside a string or comment on the line.
pub fn jsx_position(text: &str, start: usize) -> bool {
    let line = line_start(text, start);
    let mut quote: Option<char> = None;
    let mut prev = ' ';
    for c in text[line..start].chars() {
        match quote {
            Some(q) if c == q && prev != '\\' => quote = None,
            Some(_) => {}
            None if matches!(c, '"' | '\'' | '`') => quote = Some(c),
            None if c == '/' && prev == '/' => return false,
            None => {}
        }
        prev = c;
    }
    if quote.is_some() {
        return false;
    }
    let before = text[..start].trim_end();
    if before.is_empty() {
        return false;
    }
    ["(", ">", "return", "=>", "&&", "||", "?", ":"]
        .iter()
        .any(|t| {
            before.ends_with(t)
                && (*t != "return"
                    || !before[..before.len() - 6]
                        .ends_with(|c: char| c.is_alphanumeric() || c == '_'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abbr_at(text: &str) -> Option<&str> {
        extract_markup(text, text.len()).map(|s| &text[s..])
    }

    #[test]
    fn extracts_markup_abbreviations() {
        assert_eq!(abbr_at("  ul>li.item$*3"), Some("ul>li.item$*3"));
        assert_eq!(abbr_at("<div class=\"x\">ul>li"), Some("ul>li"));
        assert_eq!(
            abbr_at("text a[title=\"hello world\"]"),
            Some("a[title=\"hello world\"]")
        );
        assert_eq!(abbr_at("p{some text here}"), Some("p{some text here}"));
        assert_eq!(abbr_at("(header>nav)+footer"), Some("(header>nav)+footer"));
        assert_eq!(abbr_at("<?php echo 1 ?>div"), Some("div"));
        assert_eq!(abbr_at("<!-- x -->span"), Some("span"));
        assert_eq!(abbr_at("foo(div"), Some("div"));
        assert_eq!(abbr_at("x "), None);
        assert_eq!(abbr_at("a[href"), None);
        assert_eq!(abbr_at("@foreach"), Some("@foreach"));
    }

    #[test]
    fn css_positions() {
        let css = "a:hover {\n  color: red;\n  m10";
        assert!(css_position(css, css.len() - 3, true));
        let sel = ".a {\n}\n.b";
        assert!(!css_position(sel, sel.len() - 2, true));
        let value = "a { color: re";
        assert!(!css_position(value, value.len() - 2, true));
        let at = "@m";
        assert!(css_position(at, 0, true));
        let comment = "a { /* m10";
        assert!(!css_position(comment, comment.len() - 3, true));
        let sass = ".a\n  m10";
        assert!(css_position(sass, sass.len() - 3, false));
        assert!(!css_position(".a", 0, false));
    }

    #[test]
    fn extracts_css_abbreviations() {
        let t = "a {\n  bgc#fff";
        assert_eq!(extract_css(t, t.len()).map(|s| &t[s..]), Some("bgc#fff"));
        let t = "a {m10";
        assert_eq!(extract_css(t, t.len()).map(|s| &t[s..]), Some("m10"));
        let t = "a { (m10";
        assert_eq!(extract_css(t, t.len()), None);
    }

    #[test]
    fn syntax_by_language() {
        let html = "<div>\n  ul>li";
        let s = html.len() - 5;
        assert_eq!(
            syntax_at(html, html.len(), "html", s),
            Some(Syntax::Markup(Flavor::Html))
        );
        let tag = "<div ul";
        assert_eq!(syntax_at(tag, tag.len(), "html", 5), None);
        let php = "<?php\n$x = div";
        assert_eq!(syntax_at(php, php.len(), "php", php.len() - 3), None);
        let php = "<?php $x = 1; ?>\n<p>div";
        assert!(syntax_at(php, php.len(), "php", php.len() - 3).is_some());
        let blade = "<p>{{ $user->div";
        assert_eq!(
            syntax_at(blade, blade.len(), "blade", blade.len() - 3),
            None
        );
        let style = "<style>\n  a { m10";
        assert_eq!(
            syntax_at(style, style.len(), "vue", style.len() - 3),
            Some(Syntax::Css { semicolons: true })
        );
        let script = "<script>\n  div";
        assert_eq!(
            syntax_at(script, script.len(), "html", script.len() - 3),
            None
        );
        assert_eq!(syntax_at("x", 1, "python", 0), None);
    }

    #[test]
    fn jsx_positions() {
        let at = |t: &str, abbr: &str| jsx_position(t, t.len() - abbr.len());
        assert!(at("return (\n  div", "div"));
        assert!(at("const x = <div>\n  ul", "ul"));
        assert!(at("return div", "div"));
        assert!(at("ok && div", "div"));
        assert!(!at("const x = div", "div"));
        assert!(!at("const s = \"(div", "div"));
        assert!(!at("// (div", "div"));
        assert!(!at("myreturn div", "div"));
    }
}
