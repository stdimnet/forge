//! Formatting with a scheme's options, computed in Rust:
//!
//! - [`preview`]: the sample of a language page, laid out completely with the options
//!   (indentation, braces, spaces, blank lines, conversions), shown next to the options.
//! - [`post_format`]: the passes Reformat Code runs after a language server or tool
//!   formatted a file: the options those formatters do not know (PHP spaces, brace
//!   placement and code conversions, quotes, hex colors, SQL keyword case, Markdown spacing,
//!   Blade/Twig delimiters), blank lines, indents on empty lines and the formatter markers.

mod clike;
pub mod lexer;
mod markdown;
mod markers;
mod markup;
mod samples;
mod sql;
mod yaml;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{model, Values, GENERAL};

pub use samples::sample;

/// Typed access to one language's options (falling back to the model's defaults).
pub struct Opts<'a> {
    pub lang: &'a str,
    pub values: &'a Values,
}

impl<'a> Opts<'a> {
    pub fn new(lang: &'a str, values: &'a Values) -> Self {
        Self { lang, values }
    }
    pub fn get(&self, key: &str) -> Option<Value> {
        self.values
            .get(&format!("{}.{key}", self.lang))
            .cloned()
            .or_else(|| {
                model()
                    .language(self.lang)
                    .and_then(|l| l.option(key))
                    .map(|o| o.default.clone())
            })
    }
    pub fn bool(&self, key: &str) -> bool {
        self.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
    }
    pub fn int(&self, key: &str) -> usize {
        self.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as usize
    }
    pub fn str(&self, key: &str) -> String {
        self.get(key)
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    }
    /// Whether a choice option has `value`.
    pub fn is(&self, key: &str, value: &str) -> bool {
        self.str(key) == value
    }
    pub fn indent(&self) -> Indent {
        let size = self.int("indent");
        Indent {
            use_tabs: self.bool("useTabs"),
            tab_size: self.int("tabSize").max(1),
            size,
            continuation: if self.get("continuationIndent").is_some() {
                self.int("continuationIndent")
            } else {
                size
            },
        }
    }
}

/// Indentation of a language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Indent {
    pub use_tabs: bool,
    pub tab_size: usize,
    pub size: usize,
    pub continuation: usize,
}

impl Indent {
    /// Whitespace for an indentation of `cols` columns.
    pub fn render(&self, cols: usize) -> String {
        if self.use_tabs {
            "\t".repeat(cols / self.tab_size) + &" ".repeat(cols % self.tab_size)
        } else {
            " ".repeat(cols)
        }
    }
    /// Columns of leading whitespace.
    pub fn width(&self, ws: &str) -> usize {
        ws.chars().fold(0, |n, c| {
            if c == '\t' {
                n - n % self.tab_size + self.tab_size
            } else {
                n + 1
            }
        })
    }
}

/// The sample of `language` formatted with `values` (`None` for an unknown language).
pub fn preview(language: &str, values: &Values) -> Option<String> {
    let src = sample(language)?;
    let opts = Opts::new(language, values);
    let out = match language {
        "php" => clike::format(src, lexer::Lang::Php, &opts, clike::Mode::Layout),
        "javascript" => clike::format(src, lexer::Lang::Js, &opts, clike::Mode::Layout),
        "typescript" => clike::format(src, lexer::Lang::Ts, &opts, clike::Mode::Layout),
        "css" => clike::format(src, lexer::Lang::Css, &opts, clike::Mode::Layout),
        "json" => clike::format(src, lexer::Lang::Json, &opts, clike::Mode::Layout),
        "lua" => clike::format(src, lexer::Lang::Lua, &opts, clike::Mode::Layout),
        "sql" => forge_sqlcode::format(
            src,
            forge_sqlcode::SqlDialect::Generic,
            &forge_sqlcode::SqlStyle::from_values(values),
        ),
        "yaml" => yaml::format(src, &opts, true),
        "markdown" => markdown::format(src, &opts),
        "html" | "xml" | "blade" | "twig" => markup::format(src, &opts, true),
        GENERAL => {
            let general = Opts::new(GENERAL, values);
            let php = Opts::new("php", values);
            let text = clike::format(src, lexer::Lang::Php, &php, clike::Mode::Layout);
            return Some(markers::restore(src, &text, &general).unwrap_or(text));
        }
        _ => return None,
    };
    Some(out)
}

/// What Reformat Code sends after the language server or tool formatted a file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PostFormatRequest {
    /// The file type id (`php`, `scss`, `tsx`).
    pub file_type: String,
    /// The text before formatting (for the formatter markers).
    pub original: String,
    /// The formatted text.
    pub text: String,
}

/// Apply the options other formatters do not know to an already formatted file. `values` are
/// the selected scheme's.
pub fn post_format(req: &PostFormatRequest, values: &Values) -> String {
    let Some(lang) = model().language_for_file_type(&req.file_type) else {
        let general = Opts::new(GENERAL, values);
        return markers::restore(&req.original, &req.text, &general).unwrap_or(req.text.clone());
    };
    let opts = Opts::new(&lang.id, values);
    let text = &req.text;
    let mut out = match lang.id.as_str() {
        "php" => clike::format(text, lexer::Lang::Php, &opts, clike::Mode::Spacing),
        // Quotes in plain JavaScript/TypeScript files (the language server does the rest);
        // files with JSX are left alone, its text and attributes are not JavaScript strings.
        "javascript" | "typescript"
            if matches!(req.file_type.as_str(), "javascript" | "typescript")
                && !looks_like_jsx(text) =>
        {
            let lang = if lang.id == "typescript" {
                lexer::Lang::Ts
            } else {
                lexer::Lang::Js
            };
            clike::format(text, lang, &opts, clike::Mode::Passes)
        }
        "css" if req.file_type != "sass" && req.file_type != "stylus" => {
            clike::format(text, lexer::Lang::Css, &opts, clike::Mode::Passes)
        }
        "sql" => sql::format(text, &opts, false),
        "markdown" => markdown::format(text, &opts),
        "blade" | "twig" => markup::delimiters(text, &opts),
        _ => text.clone(),
    };
    // Blank lines (PHP's are set by the PHP pass of `phpforge-lang-php`).
    if !matches!(lang.id.as_str(), "php" | "markdown") {
        out = blank_lines(&out, &lang.id, opts.int("blankLines.keepMax"));
    }
    if opts.bool("keepIndentsOnEmptyLines") {
        out = keep_indents(&out);
    }
    let general = Opts::new(GENERAL, values);
    markers::restore(&req.original, &out, &general).unwrap_or(out)
}

/// JSX in a `.js`/`.ts` file: closing tags (`</div>`) or self-closing ones (`<Foo />`).
fn looks_like_jsx(text: &str) -> bool {
    let b = text.as_bytes();
    b.windows(3)
        .any(|w| w[0] == b'<' && w[1] == b'/' && w[2].is_ascii_alphabetic())
        || b.windows(2).enumerate().any(|(i, w)| {
            w == b"/>"
                && b[..i]
                    .iter()
                    .rev()
                    .take_while(|c| **c != b'\n')
                    .any(|c| *c == b'<')
        })
}

/// Byte ranges that must not change (strings and comments), for line based passes.
fn protected_ranges(text: &str, language: &str) -> Vec<(usize, usize)> {
    let lang = match language {
        "php" => lexer::Lang::Php,
        "javascript" => lexer::Lang::Js,
        "typescript" => lexer::Lang::Ts,
        "css" => lexer::Lang::Css,
        "json" => lexer::Lang::Json,
        "lua" => lexer::Lang::Lua,
        "sql" => lexer::Lang::Sql,
        "yaml" => return yaml::block_scalars(text),
        _ => return markup::protected(text),
    };
    let mut out = Vec::new();
    let mut at = 0;
    for t in lexer::lex(text, lang) {
        at += t.ws.len();
        if matches!(
            t.kind,
            lexer::Kind::Str | lexer::Kind::Comment | lexer::Kind::Verbatim
        ) && t.text.contains('\n')
        {
            out.push((at, at + t.text.len()));
        }
        at += t.text.len();
    }
    out
}

/// Cut runs of blank lines to `keep_max` outside strings and comments.
pub fn blank_lines(text: &str, language: &str, keep_max: usize) -> String {
    let protected = protected_ranges(text, language);
    let inside = |at: usize| protected.iter().any(|(s, e)| *s < at && at < *e);
    let mut out = String::with_capacity(text.len());
    let mut blank_run = 0;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let blank = line.trim().is_empty() && line.ends_with('\n');
        if blank && !inside(at) {
            blank_run += 1;
            if blank_run <= keep_max {
                out.push_str(line);
            }
        } else {
            blank_run = 0;
            out.push_str(line);
        }
        at += line.len();
    }
    out
}

/// "Keep indents on empty lines": empty lines get the indentation of the next line.
pub fn keep_indents(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut next_indent = String::new();
    for line in lines.iter().rev() {
        if line.trim().is_empty() {
            out.push(next_indent.clone());
        } else {
            next_indent = line[..line.len() - line.trim_start().len()].to_string();
            out.push((*line).to_string());
        }
    }
    out.reverse();
    // The last "line" after the final line break stays empty.
    if let Some(last) = out.last_mut() {
        if lines.last().is_some_and(|l| l.is_empty()) {
            last.clear();
        }
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_language_has_a_preview() {
        let defaults = model().defaults();
        for l in model().all() {
            let out = preview(&l.id, &defaults).unwrap_or_else(|| panic!("{}", l.id));
            assert!(!out.trim().is_empty(), "{}", l.id);
        }
        assert!(preview("cobol", &defaults).is_none());
    }

    #[test]
    fn previews_follow_indent_options() {
        let mut values = model().defaults();
        values.insert("javascript.indent".into(), json!(2));
        values.insert("javascript.tabSize".into(), json!(2));
        let js = preview("javascript", &values).unwrap();
        assert!(
            js.lines()
                .any(|l| l.starts_with("  ") && !l.starts_with("    ")),
            "{js}"
        );
        values.insert("json.useTabs".into(), json!(true));
        let json = preview("json", &values).unwrap();
        assert!(json.contains("\n\t\""), "{json}");
    }

    #[test]
    fn post_format_cuts_blank_lines_and_keeps_strings() {
        let mut values = model().defaults();
        values.insert("javascript.blankLines.keepMax".into(), json!(1));
        let req = PostFormatRequest {
            file_type: "javascript".into(),
            original: String::new(),
            text: "a();\n\n\n\nb(`x\n\n\n\ny`);\n".into(),
        };
        assert_eq!(post_format(&req, &values), "a();\n\nb(`x\n\n\n\ny`);\n");
    }

    #[test]
    fn quotes_skip_jsx() {
        let mut values = model().defaults();
        values.insert("javascript.punctuation.quotes".into(), json!("single"));
        let req = |text: &str| PostFormatRequest {
            file_type: "javascript".into(),
            original: String::new(),
            text: text.into(),
        };
        assert_eq!(post_format(&req("a(\"x\");\n"), &values), "a('x');\n");
        let jsx = "const a = <p>Don't \"x\"</p>;\n";
        assert_eq!(post_format(&req(jsx), &values), jsx);
    }

    #[test]
    fn keeps_indents_on_empty_lines() {
        assert_eq!(keep_indents("a {\n\n    b;\n}\n"), "a {\n    \n    b;\n}\n");
    }
}
