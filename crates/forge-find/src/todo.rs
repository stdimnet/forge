//! TODO scanning (PHPStorm's TODO tool window): find `TODO` / `FIXME` style patterns inside
//! comments.
//!
//! - Patterns are regular expressions (`regex-lite` syntax) matched against the text of each
//!   comment line; PHPStorm's defaults are `\btodo\b.*` and `\bfixme\b.*`, case-insensitive.
//! - Comments are found by a small lexer per file type ([`CommentSyntax::for_path`]): line
//!   comments (`//`, `#`, `--`, `;`), block comments (`/* */`, `<!-- -->`, `{{-- --}}`,
//!   `{# #}`, `--[[ ]]`) and strings, so `"http://x"` is not a comment. Plain text and
//!   Markdown files are scanned as a whole, like PHPStorm does.
//! - Items carry the 0-based line, the UTF-16 column and length of the match in that line and
//!   the matched text without comment terminators.

use std::collections::BTreeMap;

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

/// Default cap on reported TODO items.
pub const DEFAULT_TODO_LIMIT: usize = 5000;

fn default_todo_limit() -> usize {
    DEFAULT_TODO_LIMIT
}

/// One TODO pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoPattern {
    pub pattern: String,
    #[serde(default)]
    pub case_sensitive: bool,
}

/// PHPStorm's default patterns: `\btodo\b.*` and `\bfixme\b.*`, case-insensitive.
pub fn default_patterns() -> Vec<TodoPattern> {
    ["\\btodo\\b.*", "\\bfixme\\b.*"]
        .into_iter()
        .map(|p| TodoPattern {
            pattern: p.into(),
            case_sensitive: false,
        })
        .collect()
}

/// What to scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoRequest {
    /// Patterns to look for; empty uses [`default_patterns`].
    #[serde(default)]
    pub patterns: Vec<TodoPattern>,
    /// Only these project-relative files (live updates after a change); `None` scans the
    /// whole project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
    /// Unsaved editor contents by project-relative path; they replace the files on disk
    /// (live updates while typing).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub buffers: BTreeMap<String, String>,
    /// Maximum number of items reported.
    #[serde(default = "default_todo_limit")]
    pub limit: usize,
}

impl Default for TodoRequest {
    fn default() -> Self {
        TodoRequest {
            patterns: Vec::new(),
            paths: None,
            buffers: BTreeMap::new(),
            limit: DEFAULT_TODO_LIMIT,
        }
    }
}

/// One TODO item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoItem {
    /// 0-based line.
    pub line: usize,
    /// UTF-16 column of the match in the line.
    pub column: usize,
    /// UTF-16 length of the match.
    pub length: usize,
    /// The matched text (`TODO: send the invoice`), comment terminators removed.
    pub text: String,
    /// Index of the pattern that matched.
    pub pattern: usize,
}

/// The items of one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoFile {
    pub path: String,
    pub items: Vec<TodoItem>,
}

/// The answer to a [`TodoRequest`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TodoResult {
    /// Files with at least one item, sorted by path.
    pub files: Vec<TodoFile>,
    pub count: usize,
    pub truncated: bool,
    /// An invalid pattern; no files then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// How comments look in a file type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommentSyntax {
    pub line: &'static [&'static str],
    pub block: &'static [(&'static str, &'static str)],
    /// String delimiters (their content is never a comment).
    pub strings: &'static [char],
    /// The whole file is text (plain text, Markdown).
    pub whole: bool,
}

const C_LIKE: CommentSyntax = CommentSyntax {
    line: &["//"],
    block: &[("/*", "*/")],
    strings: &['"', '\'', '`'],
    whole: false,
};
const PHP: CommentSyntax = CommentSyntax {
    line: &["//", "#"],
    block: &[("/*", "*/"), ("<!--", "-->")],
    strings: &['"', '\''],
    whole: false,
};
const BLADE: CommentSyntax = CommentSyntax {
    line: &[],
    block: &[("{{--", "--}}"), ("<!--", "-->"), ("/*", "*/")],
    strings: &[],
    whole: false,
};
const TWIG: CommentSyntax = CommentSyntax {
    line: &[],
    block: &[("{#", "#}"), ("<!--", "-->")],
    strings: &[],
    whole: false,
};
const MARKUP: CommentSyntax = CommentSyntax {
    line: &[],
    block: &[("<!--", "-->"), ("/*", "*/")],
    strings: &[],
    whole: false,
};
const CSS: CommentSyntax = CommentSyntax {
    line: &[],
    block: &[("/*", "*/")],
    strings: &['"', '\''],
    whole: false,
};
const SCSS: CommentSyntax = CommentSyntax {
    line: &["//"],
    block: &[("/*", "*/")],
    strings: &['"', '\''],
    whole: false,
};
const HASH: CommentSyntax = CommentSyntax {
    line: &["#"],
    block: &[],
    strings: &['"', '\''],
    whole: false,
};
const INI: CommentSyntax = CommentSyntax {
    line: &["#", ";"],
    block: &[],
    strings: &[],
    whole: false,
};
const SQL: CommentSyntax = CommentSyntax {
    line: &["--", "#"],
    block: &[("/*", "*/")],
    strings: &['\''],
    whole: false,
};
const LUA: CommentSyntax = CommentSyntax {
    line: &["--"],
    block: &[("--[[", "]]")],
    strings: &['"', '\''],
    whole: false,
};
const TEXT: CommentSyntax = CommentSyntax {
    line: &[],
    block: &[],
    strings: &[],
    whole: true,
};

impl CommentSyntax {
    /// The comment syntax of a project-relative path; `None` for files that have no
    /// comments Forge knows about (images are filtered before as binary).
    pub fn for_path(path: &str) -> Option<CommentSyntax> {
        let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
        if name.ends_with(".blade.php") {
            return Some(BLADE);
        }
        if matches!(
            name.as_str(),
            "dockerfile" | "makefile" | "gemfile" | "rakefile" | "containerfile"
        ) || name.starts_with(".env")
            || name == ".gitignore"
            || name == ".editorconfig"
        {
            return Some(if name == ".editorconfig" { INI } else { HASH });
        }
        let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
        Some(match ext {
            "php" | "phtml" | "inc" => PHP,
            "js" | "mjs" | "cjs" | "jsx" | "ts" | "mts" | "cts" | "tsx" | "java" | "kt" | "kts"
            | "c" | "h" | "cc" | "cpp" | "hpp" | "cs" | "go" | "rs" | "swift" | "dart"
            | "scala" | "groovy" | "gradle" | "json5" | "jsonc" | "proto" => C_LIKE,
            "vue" | "svelte" | "astro" => CommentSyntax {
                line: &["//"],
                block: &[("<!--", "-->"), ("/*", "*/")],
                strings: &[],
                whole: false,
            },
            "twig" => TWIG,
            "html" | "htm" | "xml" | "xhtml" | "svg" | "latte" | "tpl" | "smarty" => MARKUP,
            "css" => CSS,
            "scss" | "less" | "sass" | "styl" => SCSS,
            "py" | "rb" | "sh" | "bash" | "zsh" | "fish" | "yaml" | "yml" | "toml" | "pl" | "r"
            | "ps1" | "conf" | "nginx" | "properties" | "neon" | "tf" | "hcl" | "cmake" => HASH,
            "ini" | "cfg" => INI,
            "sql" | "ddl" => SQL,
            "lua" => LUA,
            "txt" | "md" | "markdown" | "rst" | "adoc" | "" => TEXT,
            _ => return None,
        })
    }
}

/// Byte ranges of the comments in `text`.
pub fn comment_ranges(text: &str, syntax: &CommentSyntax) -> Vec<(usize, usize)> {
    if syntax.whole {
        return vec![(0, text.len())];
    }
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &text[i..];
        // Block comments first: `--[[` must win over the line comment `--`.
        if let Some((open, close)) = syntax.block.iter().find(|(o, _)| rest.starts_with(o)) {
            let body = i + open.len();
            let end = text[body..]
                .find(close)
                .map_or(text.len(), |e| body + e + close.len());
            out.push((i, end));
            i = end;
            continue;
        }
        if let Some(open) = syntax.line.iter().find(|o| rest.starts_with(**o)) {
            // PHP 8 attributes (`#[Route]`) are code, not comments.
            if *open == "#" && rest.starts_with("#[") && syntax.line.contains(&"//") {
                i += 2;
                continue;
            }
            let end = text[i..].find('\n').map_or(text.len(), |e| i + e);
            out.push((i, end));
            i = end;
            continue;
        }
        let c = rest.chars().next().unwrap_or(' ');
        if syntax.strings.contains(&c) {
            i += c.len_utf8();
            while i < bytes.len() {
                let d = text[i..].chars().next().unwrap_or(c);
                i += d.len_utf8();
                if d == '\\' {
                    if let Some(n) = text[i..].chars().next() {
                        i += n.len_utf8();
                    }
                } else if d == c || (d == '\n' && c != '`' && c != '"' && c != '\'') {
                    break;
                }
            }
            continue;
        }
        i += c.len_utf8();
    }
    out
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// Comment terminators and decoration removed from the end of a match.
fn clean_match(text: &str) -> &str {
    let mut t = text.trim_end();
    for close in ["*/", "-->", "--}}", "#}", "]]"] {
        if let Some(s) = t.strip_suffix(close) {
            t = s.trim_end();
        }
    }
    t
}

/// Compiled TODO patterns.
#[derive(Debug, Clone)]
pub struct TodoScanner {
    patterns: Vec<Regex>,
    limit: usize,
}

impl TodoScanner {
    /// Compile the patterns of `req` (defaults when empty); an invalid one is an error.
    pub fn new(req: &TodoRequest) -> Result<Self, String> {
        let defaults;
        let list = if req.patterns.is_empty() {
            defaults = default_patterns();
            &defaults
        } else {
            &req.patterns
        };
        let patterns = list
            .iter()
            .map(|p| {
                RegexBuilder::new(&p.pattern)
                    .case_insensitive(!p.case_sensitive)
                    .build()
                    .map_err(|e| format!("Bad TODO pattern \"{}\": {e}", p.pattern))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(TodoScanner {
            patterns,
            limit: req.limit.max(1),
        })
    }

    /// Whether a file can contain TODO comments (its comment syntax is known).
    pub fn wants(&self, path: &str) -> bool {
        CommentSyntax::for_path(path).is_some()
    }

    /// TODO items of one file.
    pub fn scan(&self, path: &str, text: &str) -> Vec<TodoItem> {
        let Some(syntax) = CommentSyntax::for_path(path) else {
            return Vec::new();
        };
        let mut items = Vec::new();
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        for (from, to) in comment_ranges(text, &syntax) {
            let mut line = line_starts.partition_point(|&s| s <= from) - 1;
            let mut seg_start = from;
            loop {
                let line_end = line_starts.get(line + 1).map_or(text.len(), |&n| n - 1);
                let seg_end = line_end.min(to);
                let segment = text[seg_start..seg_end].trim_end_matches('\r');
                // First pattern that matches wins, like PHPStorm (one item per line).
                if let Some((index, m)) = self.patterns.iter().enumerate().find_map(|(i, re)| {
                    re.find(segment)
                        .filter(|m| m.end() > m.start())
                        .map(|m| (i, m))
                }) {
                    let matched = clean_match(m.as_str());
                    let line_start = line_starts[line];
                    let abs = seg_start + m.start();
                    items.push(TodoItem {
                        line,
                        column: utf16_len(&text[line_start..abs]),
                        length: utf16_len(matched),
                        text: matched.to_string(),
                        pattern: index,
                    });
                }
                if seg_end >= to || line + 1 >= line_starts.len() {
                    break;
                }
                line += 1;
                seg_start = line_starts[line];
            }
        }
        items
    }
}

/// Collects TODO results up to the request's limit.
#[derive(Debug)]
pub struct TodoCollector<'a> {
    scanner: &'a TodoScanner,
    result: TodoResult,
}

impl<'a> TodoCollector<'a> {
    pub fn new(scanner: &'a TodoScanner) -> Self {
        TodoCollector {
            scanner,
            result: TodoResult::default(),
        }
    }

    pub fn full(&self) -> bool {
        self.result.truncated
    }

    pub fn add(&mut self, path: &str, text: &str) {
        if self.full() {
            return;
        }
        let mut items = self.scanner.scan(path, text);
        if items.is_empty() {
            return;
        }
        let budget = self.scanner.limit - self.result.count;
        if items.len() > budget {
            items.truncate(budget);
            self.result.truncated = true;
        }
        self.result.count += items.len();
        if !items.is_empty() {
            self.result.files.push(TodoFile {
                path: path.to_string(),
                items,
            });
        }
    }

    pub fn finish(mut self) -> TodoResult {
        self.result.files.sort_by(|a, b| a.path.cmp(&b.path));
        self.result
    }
}

/// Scan in-memory files (browser mode, tests). `req.paths` limits the files and
/// `req.buffers` replaces their texts.
pub fn scan_todos<'t>(
    req: &TodoRequest,
    files: impl IntoIterator<Item = (&'t str, &'t str)>,
) -> TodoResult {
    let scanner = match TodoScanner::new(req) {
        Ok(s) => s,
        Err(e) => {
            return TodoResult {
                error: Some(e),
                ..TodoResult::default()
            }
        }
    };
    let mut collector = TodoCollector::new(&scanner);
    for (path, text) in files {
        if collector.full() {
            break;
        }
        if req
            .paths
            .as_ref()
            .is_some_and(|only| !only.iter().any(|p| p == path))
        {
            continue;
        }
        let text = req.buffers.get(path).map_or(text, String::as_str);
        collector.add(path, text);
    }
    collector.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(path: &str, text: &str) -> Vec<TodoItem> {
        TodoScanner::new(&TodoRequest::default())
            .unwrap()
            .scan(path, text)
    }

    #[test]
    fn finds_todo_and_fixme_in_php_comments_only() {
        let php = "<?php\n// TODO: send the invoice\n$url = \"http://x // todo not a comment\";\n/*\n * fixme later */\n# Todo hash style\n#[Route('/todo')]\n$todo = 1;\n";
        let items = scan("app/Order.php", php);
        let texts: Vec<_> = items.iter().map(|i| (i.line, i.text.as_str())).collect();
        assert_eq!(
            texts,
            vec![
                (1, "TODO: send the invoice"),
                (4, "fixme later"),
                (5, "Todo hash style")
            ]
        );
        assert_eq!((items[0].column, items[0].length), (3, 22));
        assert_eq!(items[1].pattern, 1);
    }

    #[test]
    fn handles_markup_blade_twig_sql_lua_and_text() {
        let blade = "<div>{{-- TODO: translate --}}</div>\n<!-- FIXME: aria -->";
        let items = scan("resources/views/a.blade.php", blade);
        assert_eq!(items[0].text, "TODO: translate");
        assert_eq!(items[1].text, "FIXME: aria");
        assert_eq!(scan("t.twig", "{# todo: cache #}")[0].text, "todo: cache");
        assert_eq!(
            scan("q.sql", "select 1; -- TODO index")[0].text,
            "TODO index"
        );
        assert_eq!(
            scan("m.lua", "--[[ TODO: block ]]\nx = 1 -- fixme")[1].line,
            1
        );
        assert_eq!(scan("NOTES.md", "- TODO write docs")[0].column, 2);
        assert!(scan("logo.png", "TODO").is_empty());
    }

    #[test]
    fn whole_words_and_unicode_columns() {
        assert!(scan("a.js", "// todos are not todo-less").len() == 1);
        assert!(scan("a.js", "// mastodon").is_empty());
        let items = scan("a.ts", "const s = 'ü'; // TODO ünïcode");
        assert_eq!(items[0].column, 18);
        assert_eq!(items[0].text, "TODO ünïcode");
    }

    #[test]
    fn custom_patterns_limits_and_errors() {
        let req = TodoRequest {
            patterns: vec![TodoPattern {
                pattern: "\\bHACK\\b.*".into(),
                case_sensitive: true,
            }],
            ..TodoRequest::default()
        };
        let files = [
            ("a.php", "<?php // HACK one\n// hack two"),
            ("b.php", "<?php"),
        ];
        let r = scan_todos(&req, files);
        assert_eq!(r.count, 1);
        assert_eq!(r.files.len(), 1);

        let only = TodoRequest {
            paths: Some(vec!["b.php".into()]),
            ..TodoRequest::default()
        };
        assert!(scan_todos(&only, [("a.php", "<?php // todo")])
            .files
            .is_empty());

        let limited = TodoRequest {
            limit: 1,
            ..TodoRequest::default()
        };
        let r = scan_todos(&limited, [("a.php", "<?php // todo 1\n// todo 2")]);
        assert!(r.truncated);
        assert_eq!(r.count, 1);

        let bad = TodoRequest {
            patterns: vec![TodoPattern {
                pattern: "(".into(),
                case_sensitive: false,
            }],
            ..TodoRequest::default()
        };
        assert!(scan_todos(&bad, []).error.is_some());
    }

    #[test]
    fn unsaved_buffers_replace_the_files() {
        let req = TodoRequest {
            paths: Some(vec!["a.php".into()]),
            buffers: [("a.php".to_string(), "<?php\n\n// FIXME: typed".to_string())].into(),
            ..TodoRequest::default()
        };
        let r = scan_todos(&req, [("a.php", "<?php // TODO: saved")]);
        assert_eq!(r.count, 1);
        assert_eq!(r.files[0].items[0].text, "FIXME: typed");
        assert_eq!(r.files[0].items[0].line, 2);
        // A buffer without TODOs removes the file from the answer.
        let cleared = TodoRequest {
            buffers: [("a.php".to_string(), "<?php".to_string())].into(),
            ..req
        };
        assert!(scan_todos(&cleared, [("a.php", "<?php // TODO")])
            .files
            .is_empty());
    }
}
