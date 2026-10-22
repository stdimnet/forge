//! Documentation comments (PHPDoc, JSDoc / TSDoc, LuaDoc / LuaCATS) for PHPStorm's
//! rendered view ("Render Doc Comments", Reader mode): [`doc_comments`] finds them in a
//! file, [`parse_doc_comment`] turns one into the parts of the documentation popup
//! ([`HoverDoc`]): the description as Markdown, the tags as `Params:` / `Returns:` /
//! `Throws:` / `See also:` sections and `@deprecated` as the deprecation note. Inline tags
//! (`{@link https://… text}`, `{@see Order}`) become links and code.
//!
//! Lines are 0-based; columns are UTF-16 code units.

use serde::{Deserialize, Serialize};

use crate::ast::{Block, Inline};
use crate::hover::{section_title, HoverDoc, HoverSection};
use crate::parse::{parse, parse_inline, Flavor};

/// One documentation comment of a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocComment {
    /// First line (`/**` or the first `---`).
    pub line: u32,
    /// Last line (`*/` or the last `---`).
    pub end_line: u32,
    /// Where the comment starts on `line`.
    pub column: u32,
    pub doc: HoverDoc,
}

/// The comment syntax of a language's documentation comments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DocStyle {
    /// `/** … */` with PHPDoc tags (`@param Type $name`).
    Php,
    /// `/** … */` with JSDoc tags (`@param {Type} name`).
    Js,
    /// `---` lines with LuaCATS / EmmyLua tags (`@param name type`).
    Lua,
}

/// The documentation comment style of a Forge file type id, or `None` when the language
/// has none.
pub fn doc_style(language: &str) -> Option<DocStyle> {
    match language {
        "php" | "blade" => Some(DocStyle::Php),
        "javascript" | "jsx" | "typescript" | "tsx" | "javascriptreact" | "typescriptreact"
        | "vue" | "html" | "java" | "kotlin" | "scala" | "groovy" | "dart" | "css" | "scss"
        | "less" | "c" | "cpp" | "csharp" | "swift" | "go" | "rust" => Some(DocStyle::Js),
        "lua" => Some(DocStyle::Lua),
        _ => None,
    }
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// The documentation comments of `text` (a file in `language`), in file order. A comment
/// counts when it starts its line: `/**` (not `/**/`) in C-like languages, a run of `---`
/// lines in Lua.
pub fn doc_comments(text: &str, language: &str) -> Vec<DocComment> {
    let Some(style) = doc_style(language) else {
        return Vec::new();
    };
    let lines: Vec<&str> = text
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let mut out = Vec::new();
    let mut n = 0;
    while n < lines.len() {
        let line = lines[n];
        let trimmed = line.trim_start();
        let column = utf16_len(&line[..line.len() - trimmed.len()]) as u32;
        if style == DocStyle::Lua {
            if is_lua_doc_line(trimmed) {
                let start = n;
                while n + 1 < lines.len() && is_lua_doc_line(lines[n + 1].trim_start()) {
                    n += 1;
                }
                let raw = lines[start..=n].join("\n");
                out.push(DocComment {
                    line: start as u32,
                    end_line: n as u32,
                    column,
                    doc: parse_doc_comment(&raw, style),
                });
            }
            n += 1;
            continue;
        }
        if trimmed.starts_with("/**") && !trimmed.starts_with("/**/") {
            let start = n;
            let mut end = None;
            // `*/` on the first line after `/**`, or on a later line.
            if trimmed[3..].contains("*/") {
                end = Some(n);
            } else {
                for (k, l) in lines.iter().enumerate().skip(n + 1) {
                    if l.contains("*/") {
                        end = Some(k);
                        break;
                    }
                }
            }
            if let Some(end) = end {
                let raw = lines[start..=end].join("\n");
                out.push(DocComment {
                    line: start as u32,
                    end_line: end as u32,
                    column,
                    doc: parse_doc_comment(&raw, style),
                });
                n = end + 1;
                continue;
            }
        }
        n += 1;
    }
    out
}

fn is_lua_doc_line(trimmed: &str) -> bool {
    trimmed.starts_with("---") && !trimmed.starts_with("----")
}

/// The text of a comment without its delimiters and line prefixes (`*`, `---`).
fn comment_body(raw: &str, style: DocStyle) -> Vec<String> {
    let mut out = Vec::new();
    let lines: Vec<&str> = raw.split('\n').collect();
    let last = lines.len() - 1;
    for (i, line) in lines.iter().enumerate() {
        let mut l = line.trim_start();
        if style == DocStyle::Lua {
            l = l.strip_prefix("---").unwrap_or(l);
        } else {
            if i == 0 {
                l = l.strip_prefix("/**").unwrap_or(l);
            }
            if i == last || l.contains("*/") {
                if let Some(at) = l.find("*/") {
                    l = &l[..at];
                }
            }
            if i > 0 {
                l = l.strip_prefix('*').unwrap_or(l);
            }
        }
        // One space after the prefix; deeper indentation is Markdown (code, lists).
        let l = l.strip_prefix(' ').unwrap_or(l);
        out.push(l.trim_end().to_string());
    }
    // Drop blank first and last lines (`/**` and ` */` on their own lines).
    while out.first().is_some_and(|l| l.trim().is_empty()) {
        out.remove(0);
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out
}

/// Inline tags as Markdown: `{@link https://x text}` → `[text](https://x)`,
/// `{@see Order::total()}` → `` `Order::total()` ``, `{@inheritDoc}` → its name.
fn inline_tags(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(at) = rest.find("{@") {
        let Some(close) = rest[at..].find('}') else {
            break;
        };
        out.push_str(&rest[..at]);
        let inner = &rest[at + 2..at + close];
        let (tag, body) = inner.split_once(char::is_whitespace).unwrap_or((inner, ""));
        let body = body.trim();
        match tag {
            "link" | "see" | "linkplain" | "linkcode" => {
                let (target, label) = body
                    .split_once(|c: char| c.is_whitespace() || c == '|')
                    .map(|(t, l)| (t, l.trim()))
                    .unwrap_or((body, ""));
                if is_url(target) {
                    let label = if label.is_empty() { target } else { label };
                    out.push_str(&format!("[{label}]({target})"));
                } else if label.is_empty() {
                    out.push_str(&format!("`{target}`"));
                } else {
                    out.push_str(&format!("{label} (`{target}`)"));
                }
            }
            "inheritDoc" | "inheritdoc" => out.push_str("inheritDoc"),
            "code" | "literal" => out.push_str(&format!("`{body}`")),
            _ => out.push_str(body),
        }
        rest = &rest[at + close + 1..];
    }
    out.push_str(rest);
    out
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

fn text(t: &str) -> Inline {
    Inline::Text { text: t.into() }
}

fn code(t: &str) -> Inline {
    Inline::Code {
        text: t.into(),
        parts: None,
    }
}

/// The first whitespace-separated word of `s` and the rest; a JSDoc `{type}` (which may
/// contain spaces and nested braces) counts as one word.
fn split_word(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    if s.starts_with('{') {
        let mut depth = 0;
        for (i, c) in s.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return (&s[..=i], s[i + 1..].trim_start());
                    }
                }
                _ => {}
            }
        }
        return (s, "");
    }
    // PHP types with generics keep their spaces: `array<int, string>`.
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '<' | '(' | '[' => depth += 1,
            '>' | ')' | ']' => depth -= 1,
            c if c.is_whitespace() && depth <= 0 => return (&s[..i], s[i..].trim_start()),
            _ => {}
        }
    }
    (s, "")
}

/// `– description` appended to an entry (when there is one).
fn with_description(mut entry: Vec<Inline>, desc: &str) -> Vec<Inline> {
    let desc = desc.trim().trim_start_matches(['-', '–']).trim();
    if !desc.is_empty() {
        if !entry.is_empty() {
            entry.push(text(" – "));
        }
        entry.extend(parse_inline(&inline_tags(desc), Flavor::Docs));
    }
    entry
}

/// A type as code, without JSDoc's braces.
fn type_inline(t: &str) -> Inline {
    code(
        t.strip_prefix('{')
            .and_then(|t| t.strip_suffix('}'))
            .unwrap_or(t),
    )
}

/// One tag's entry in its section.
fn tag_entry(tag: &str, body: &str, style: DocStyle) -> Vec<Inline> {
    match tag {
        "param" | "arg" | "argument" | "property" | "property-read" | "property-write" => {
            let (first, rest) = split_word(body);
            let (ty, name, desc) = match style {
                // `@param name type desc`
                DocStyle::Lua => {
                    let (ty, desc) = split_word(rest);
                    (ty, first, desc)
                }
                // `@param {Type} name desc` or `@param name desc`
                DocStyle::Js if first.starts_with('{') => {
                    let (name, desc) = split_word(rest);
                    (first, name, desc)
                }
                DocStyle::Js => ("", first, rest),
                // `@param Type $name desc`, `@param $name desc`
                DocStyle::Php if first.starts_with('$') || first.starts_with("...$") => {
                    ("", first, rest)
                }
                DocStyle::Php => {
                    let (name, desc) = split_word(rest);
                    if name.starts_with('$') || name.starts_with("...$") || name.starts_with("&$") {
                        (first, name, desc)
                    } else {
                        (first, "", rest)
                    }
                }
            };
            let mut entry = Vec::new();
            if !ty.is_empty() {
                entry.push(type_inline(ty));
            }
            if !name.is_empty() {
                if !entry.is_empty() {
                    entry.push(text(" "));
                }
                entry.push(code(name.trim_start_matches('[').trim_end_matches(']')));
            }
            with_description(entry, desc)
        }
        "return" | "returns" | "throws" | "throw" | "exception" | "var" | "type" => {
            if body.is_empty() {
                return Vec::new();
            }
            let (ty, desc) = split_word(body);
            // `@var Type $name desc`
            let desc = match split_word(desc) {
                (name, rest) if tag == "var" && name.starts_with('$') => rest,
                _ => desc,
            };
            with_description(vec![type_inline(ty)], desc)
        }
        "see" | "link" | "uses" | "used-by" => {
            let (target, desc) = split_word(body);
            let first = if is_url(target) {
                Inline::Link {
                    href: target.to_string(),
                    children: vec![text(if desc.trim().is_empty() {
                        target
                    } else {
                        desc.trim()
                    })],
                }
            } else {
                code(target)
            };
            if is_url(target) {
                vec![first]
            } else {
                with_description(vec![first], desc)
            }
        }
        "method" => vec![code(body.trim())],
        _ => parse_inline(&inline_tags(body), Flavor::Docs),
    }
}

/// The section a tag is listed in.
fn tag_title(tag: &str) -> String {
    match tag {
        "arg" | "argument" => "Params:".into(),
        "returns" => "Returns:".into(),
        "throw" | "exception" => "Throws:".into(),
        "property" | "property-read" | "property-write" => "Properties:".into(),
        "method" => "Methods:".into(),
        "type" => "Type:".into(),
        "uses" | "used-by" => "See also:".into(),
        "author" => "Author:".into(),
        "version" => "Version:".into(),
        "todo" => "TODO:".into(),
        other => section_title(other),
    }
}

const ORDER: [&str; 10] = [
    "Type:",
    "Params:",
    "Returns:",
    "Throws:",
    "Properties:",
    "Methods:",
    "Since:",
    "See also:",
    "Author:",
    "Version:",
];

/// Turn one documentation comment (with its delimiters) into the parts of the rendered
/// view.
pub fn parse_doc_comment(raw: &str, style: DocStyle) -> HoverDoc {
    let lines = comment_body(raw, style);
    let mut description: Vec<String> = Vec::new();
    // (tag, body lines)
    let mut tags: Vec<(String, Vec<String>)> = Vec::new();
    let mut in_fence = false;
    for line in &lines {
        let t = line.trim_start();
        if t.starts_with("```") {
            in_fence = !in_fence;
        }
        if !in_fence && t.starts_with('@') && t[1..].starts_with(|c: char| c.is_ascii_alphabetic())
        {
            let name_len = t[1..]
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
                .map(|i| i + 1)
                .unwrap_or(t.len());
            let tag = t[1..name_len].to_string();
            let body = t[name_len..].trim().to_string();
            tags.push((tag, vec![body]));
            continue;
        }
        match tags.last_mut() {
            Some((_, body)) => body.push(line.clone()),
            None => description.push(line.clone()),
        }
    }

    let mut doc = HoverDoc {
        body: parse(&inline_tags(description.join("\n").trim()), Flavor::Docs),
        ..HoverDoc::default()
    };
    let mut sections: Vec<(String, Vec<Vec<Inline>>)> = Vec::new();
    for (tag, body) in tags {
        let tag = tag.to_ascii_lowercase();
        // LuaCATS annotations that describe types, not documentation.
        if style == DocStyle::Lua
            && matches!(
                tag.as_str(),
                "class"
                    | "alias"
                    | "diagnostic"
                    | "meta"
                    | "module"
                    | "overload"
                    | "generic"
                    | "cast"
                    | "operator"
                    | "nodiscard"
                    | "async"
                    | "enum"
            )
        {
            continue;
        }
        if matches!(
            tag.as_str(),
            "inheritdoc" | "internal" | "api" | "psalm-suppress" | "phpstan-ignore"
        ) {
            continue;
        }
        if tag == "example" {
            let code_text = body[1..].join("\n");
            let first = body[0].trim();
            let code_text = if first.is_empty() {
                code_text
            } else if code_text.is_empty() {
                first.to_string()
            } else {
                format!("{first}\n{code_text}")
            };
            let lang = match style {
                DocStyle::Php => "php",
                DocStyle::Js => "js",
                DocStyle::Lua => "lua",
            };
            doc.body.push(Block::Paragraph {
                inline: vec![Inline::Strong {
                    children: vec![text("Example:")],
                }],
            });
            doc.body.push(Block::Code {
                lang: lang.into(),
                text: code_text.trim_end().to_string(),
                open: false,
            });
            continue;
        }
        let joined = body
            .iter()
            .map(|l| l.trim())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        if tag == "deprecated" {
            doc.deprecated = Some(parse_inline(&inline_tags(&joined), Flavor::Docs));
            continue;
        }
        let entry = tag_entry(&tag, &joined, style);
        let title = tag_title(&tag);
        match sections.iter_mut().find(|(t, _)| *t == title) {
            Some((_, entries)) => entries.push(entry),
            None => sections.push((title, vec![entry])),
        }
    }
    let rank = |t: &str| ORDER.iter().position(|o| *o == t).unwrap_or(98);
    sections.sort_by_key(|(t, _)| rank(t));
    doc.sections = sections
        .into_iter()
        .map(|(title, entries)| HoverSection { title, entries })
        .collect();
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(doc: &HoverDoc) -> Vec<(String, Vec<String>)> {
        doc.sections
            .iter()
            .map(|s| {
                (
                    s.title.clone(),
                    s.entries.iter().map(|e| Inline::text_of(e)).collect(),
                )
            })
            .collect()
    }

    const PHP: &str = r#"<?php

class Cart
{
    /**
     * Sum of the **line totals**, see {@link https://example.com/tax the tax rules}.
     *
     * Uses {@see Order::total()}.
     *
     * @param array<int, Line> $lines The lines
     *                                over two lines.
     * @param bool $gross
     * @return float the total
     * @throws \InvalidArgumentException when a line is invalid
     * @deprecated Use {@see total()} instead.
     * @since 2.0
     * @see https://example.com/docs
     */
    public function sum(array $lines, bool $gross): float
    {
    }

    /** @var list<string> */
    protected $items = [];

    /**/
}
"#;

    #[test]
    fn finds_php_doc_comments() {
        let found = doc_comments(PHP, "php");
        assert_eq!(found.len(), 2);
        assert_eq!(
            (found[0].line, found[0].end_line, found[0].column),
            (4, 17, 4)
        );
        assert_eq!((found[1].line, found[1].end_line), (22, 22));
        assert_eq!(
            texts(&found[1].doc),
            [("Type:".to_string(), vec!["list<string>".to_string()])]
        );
    }

    #[test]
    fn renders_description_and_tags() {
        let doc = &doc_comments(PHP, "php")[0].doc;
        // The description keeps Markdown and turns inline tags into links and code.
        let Block::Paragraph { inline } = &doc.body[0] else {
            panic!("paragraph expected");
        };
        assert!(inline.iter().any(|i| matches!(i, Inline::Strong { .. })));
        assert!(inline
            .iter()
            .any(|i| matches!(i, Inline::Link { href, .. } if href == "https://example.com/tax")));
        assert_eq!(
            Inline::text_of(match &doc.body[1] {
                Block::Paragraph { inline } => inline,
                _ => panic!(),
            }),
            "Uses Order::total()."
        );
        assert_eq!(
            texts(doc),
            [
                (
                    "Params:".to_string(),
                    vec![
                        "array<int, Line> $lines – The lines over two lines.".to_string(),
                        "bool $gross".to_string()
                    ]
                ),
                (
                    "Returns:".to_string(),
                    vec!["float – the total".to_string()]
                ),
                (
                    "Throws:".to_string(),
                    vec!["\\InvalidArgumentException – when a line is invalid".to_string()]
                ),
                ("Since:".to_string(), vec!["2.0".to_string()]),
                (
                    "See also:".to_string(),
                    vec!["https://example.com/docs".to_string()]
                ),
            ]
        );
        assert_eq!(
            Inline::text_of(doc.deprecated.as_deref().unwrap()),
            "Use total() instead."
        );
    }

    #[test]
    fn jsdoc_tags() {
        let js = "/**\n * Adds numbers.\n * @param {number} a - first\n * @param {{x: number}} [opts] options\n * @returns {Promise<number>} the sum\n * @example\n * add(1, 2)\n */\nfunction add(a, opts) {}\n";
        let found = doc_comments(js, "typescript");
        assert_eq!(found.len(), 1);
        let doc = &found[0].doc;
        assert_eq!(
            texts(doc),
            [
                (
                    "Params:".to_string(),
                    vec![
                        "number a – first".to_string(),
                        "{x: number} opts – options".to_string()
                    ]
                ),
                (
                    "Returns:".to_string(),
                    vec!["Promise<number> – the sum".to_string()]
                ),
            ]
        );
        assert!(
            matches!(&doc.body[2], Block::Code { text, lang, .. } if text == "add(1, 2)" && lang == "js")
        );
    }

    #[test]
    fn luadoc_comments() {
        let lua = "local M = {}\n\n--- Runs the jobs.\n---@param jobs table[] the jobs\n---@return boolean ok\nfunction M.run(jobs)\nend\n---- not a doc comment\n";
        let found = doc_comments(lua, "lua");
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].line, found[0].end_line), (2, 4));
        let doc = &found[0].doc;
        assert_eq!(
            Inline::text_of(match &doc.body[0] {
                Block::Paragraph { inline } => inline,
                _ => panic!(),
            }),
            "Runs the jobs."
        );
        assert_eq!(
            texts(doc),
            [
                (
                    "Params:".to_string(),
                    vec!["table[] jobs – the jobs".to_string()]
                ),
                ("Returns:".to_string(), vec!["boolean – ok".to_string()]),
            ]
        );
    }

    #[test]
    fn languages_without_doc_comments() {
        assert!(doc_comments("/** x */", "yaml").is_empty());
        assert_eq!(doc_style("vue"), Some(DocStyle::Js));
    }
}
