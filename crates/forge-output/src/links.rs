//! File references in console and terminal output (`app/User.php:42`,
//! `/abs/path/File.php on line 7`, `File.php(12)`, `#0 /path/x.php(31): …`) and web links,
//! so the UI can make them clickable.

use std::sync::OnceLock;

use regex_lite::Regex;
use serde::{Deserialize, Serialize};

/// A link in a line of text. `start`/`end` are byte offsets (end exclusive) unless a function
/// says otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileLink {
    /// A web link (`https://…`); `path` then holds the URL.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub url: bool,
    pub start: usize,
    pub end: usize,
    /// The path as written in the output.
    pub path: String,
    /// The path relative to the project root, for file links that resolve into it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// 1-based.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

const W: &str = "A-Za-z0-9_";

fn file_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let ext = "[A-Za-z][A-Za-z0-9]{0,6}";
        let path =
            format!(r"(?:[A-Za-z]:[\\/]|~?/|\.{{1,2}}/)?[{W}.@~-]+(?:[\\/][{W}.@~-]+)*\.{ext}");
        Regex::new(&format!(
            r"({path})(?::([0-9]+)(?::([0-9]+))?| on line ([0-9]+)|\(([0-9]+)\))?"
        ))
        .expect("file link regex")
    })
}

fn url_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let ws = r"\t\n\x0B\x0C\r ";
        Regex::new(&format!(
            r#"https?://[^{ws}<>"'`\]\[)]+[^{ws}<>"'`.,;:!?\]\[)]"#
        ))
        .expect("url regex")
    })
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn floor_boundary(s: &str, mut i: usize) -> usize {
    i = i.min(s.len());
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// File references in `text`, without checking that they exist.
pub fn find_file_links(text: &str) -> Vec<FileLink> {
    let mut out = Vec::new();
    for caps in file_re().captures_iter(text) {
        let whole = caps.get(0).expect("match");
        let path = &caps[1];
        let idx = whole.start();
        let before = text[..idx].chars().next_back();
        let line = caps
            .get(2)
            .or_else(|| caps.get(4))
            .or_else(|| caps.get(5))
            .and_then(|m| m.as_str().parse::<u32>().ok());
        // Skip URLs, e-mail addresses and bare names like `v1.2` or `README.md` without a line.
        if matches!(before, Some(':' | '/' | '@')) || path.starts_with(|c: char| c.is_ascii_digit())
        {
            continue;
        }
        let around =
            &text[floor_boundary(text, idx.saturating_sub(10))..floor_boundary(text, idx + 3)];
        if let Some(p) = around.find("://") {
            if p > 0 && around[..p].chars().all(is_word) {
                continue;
            }
        }
        if line.is_none() && !path.contains(['/', '\\']) {
            continue;
        }
        let stem = match path.rfind('.') {
            Some(dot) if path[dot + 1..].chars().all(|c| c.is_ascii_alphabetic()) => &path[..dot],
            _ => path,
        };
        if !stem.is_empty() && stem.chars().all(|c| c.is_ascii_digit() || c == '.') {
            continue;
        }
        out.push(FileLink {
            url: false,
            start: idx,
            end: whole.end(),
            path: path.to_string(),
            target: None,
            line,
            column: caps.get(3).and_then(|m| m.as_str().parse().ok()),
        });
    }
    out
}

fn find_urls(text: &str) -> Vec<FileLink> {
    url_re()
        .find_iter(text)
        .filter(|m| !text[..m.start()].chars().next_back().is_some_and(is_word))
        .map(|m| FileLink {
            url: true,
            start: m.start(),
            end: m.end(),
            path: m.as_str().to_string(),
            target: None,
            line: None,
            column: None,
        })
        .collect()
}

/// Where paths in output are resolved: the project root and the process's working directory.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkContext {
    #[serde(default)]
    pub root: Option<String>,
    /// Working directory relative to the root (empty = the root).
    #[serde(default)]
    pub cwd_rel: String,
}

impl LinkContext {
    /// `cwd` may be absolute (inside the root) or relative to it.
    pub fn new(root: Option<&str>, cwd: Option<&str>) -> Self {
        let cwd_rel = cwd
            .filter(|c| !c.is_empty())
            .and_then(|c| to_workspace_path(c, root, ""))
            .unwrap_or_default();
        LinkContext {
            root: root.map(str::to_string),
            cwd_rel,
        }
    }

    pub fn resolve(&self, path: &str) -> Option<String> {
        to_workspace_path(path, self.root.as_deref(), &self.cwd_rel)
    }
}

/// File links that resolve into the project (with `target` set) plus web links, sorted by
/// position.
pub fn find_links(text: &str, ctx: &LinkContext) -> Vec<FileLink> {
    let mut links = find_urls(text);
    let urls = links.len();
    for mut f in find_file_links(text) {
        if links[..urls]
            .iter()
            .any(|u| f.start < u.end && u.start < f.end)
        {
            continue;
        }
        if let Some(target) = ctx.resolve(&f.path) {
            f.target = Some(target);
            links.push(f);
        }
    }
    links.sort_by_key(|l| l.start);
    links
}

/// File links of a line of terminal output that resolve into the project, with offsets in
/// UTF-16 code units (JavaScript string indices).
pub fn file_links_utf16(text: &str, root: Option<&str>) -> Vec<FileLink> {
    find_links(text, &LinkContext::new(root, None))
        .into_iter()
        .filter(|l| !l.url)
        .map(|mut l| {
            l.start = crate::byte_to_utf16(text, l.start);
            l.end = crate::byte_to_utf16(text, l.end);
            l
        })
        .collect()
}

/// Map a path from output to a workspace-relative path: strips the project root, resolves
/// `./` and `..` and keeps relative paths (relative to `cwd_rel`, the root by default).
/// `None` for absolute paths outside the project.
pub fn to_workspace_path(path: &str, root: Option<&str>, cwd_rel: &str) -> Option<String> {
    let p = path.replace('\\', "/");
    if let Some(r) = root.map(|r| r.replace('\\', "/")) {
        let r = r.trim_end_matches('/');
        if !r.is_empty() {
            if p == r {
                return Some(String::new());
            }
            if let Some(rest) = p.strip_prefix(r).and_then(|x| x.strip_prefix('/')) {
                return Some(rest.to_string());
            }
        }
    }
    let b = p.as_bytes();
    let drive = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'/';
    if drive || p.starts_with('/') || p.starts_with("~/") {
        return None;
    }
    let p = p.strip_prefix("./").unwrap_or(&p);
    let base = cwd_rel.trim_matches('/');
    let joined = if base.is_empty() {
        p.to_string()
    } else {
        format!("{base}/{p}")
    };
    let mut out: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            ".." => {
                out.pop()?;
            }
            "." => {}
            _ => out.push(part),
        }
    }
    Some(out.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(links: &[FileLink]) -> Vec<(&str, Option<u32>)> {
        links.iter().map(|l| (l.path.as_str(), l.line)).collect()
    }

    #[test]
    fn finds_paths_with_lines_in_common_formats() {
        let text =
            "Error in /home/u/shop/app/Models/User.php on line 42 and tests/Unit/UserTest.php:21";
        let links = find_file_links(text);
        assert_eq!(
            paths(&links),
            [
                ("/home/u/shop/app/Models/User.php", Some(42)),
                ("tests/Unit/UserTest.php", Some(21))
            ]
        );
        assert_eq!(
            &text[links[1].start..links[1].end],
            "tests/Unit/UserTest.php:21"
        );
        let l = &find_file_links("#1 /p/vendor/x/Foo.php(31): bar()")[0];
        assert_eq!((l.path.as_str(), l.line), ("/p/vendor/x/Foo.php", Some(31)));
        let l = &find_file_links("src/main.ts:3:14")[0];
        assert_eq!((l.line, l.column), (Some(3), Some(14)));
    }

    #[test]
    fn ignores_urls_versions_and_bare_names() {
        assert!(find_file_links("see https://example.com/docs/a.html").is_empty());
        assert!(find_file_links("PHP 8.3.12 (cli) version 2.8.4").is_empty());
        assert!(find_file_links("README.md").is_empty());
        assert_eq!(
            paths(&find_file_links("modified:   app/Models/User.php")),
            [("app/Models/User.php", None)]
        );
    }

    #[test]
    fn finds_web_links_and_drops_file_links_outside_the_project() {
        let text =
            "Server on http://127.0.0.1:8000/app.js and /build/assets/app.js, see app/User.php:3.";
        let ctx = LinkContext::new(Some("/r"), None);
        let links = find_links(text, &ctx);
        let got: Vec<_> = links.iter().map(|l| (l.path.as_str(), l.url)).collect();
        assert_eq!(
            got,
            [
                ("http://127.0.0.1:8000/app.js", true),
                ("app/User.php", false)
            ]
        );
        assert_eq!(links[1].target.as_deref(), Some("app/User.php"));
        assert!(find_links("xhttps://a.b", &ctx).is_empty());
        let l = &file_links_utf16("é /r/a/B.php:3", Some("/r"))[0];
        assert_eq!(
            (l.start, l.end, l.target.as_deref()),
            (2, 14, Some("a/B.php"))
        );
    }

    #[test]
    fn maps_paths_into_the_workspace() {
        let w = |p: &str, r: &str, c: &str| to_workspace_path(p, Some(r), c);
        assert_eq!(
            w("/home/u/shop/app/User.php", "/home/u/shop", "").as_deref(),
            Some("app/User.php")
        );
        assert_eq!(
            w("C:\\work\\shop\\app\\User.php", "C:\\work\\shop", "").as_deref(),
            Some("app/User.php")
        );
        assert_eq!(w("./tests/A.php", "/r", "").as_deref(), Some("tests/A.php"));
        assert_eq!(w("../x.php", "/r", "sub").as_deref(), Some("x.php"));
        assert_eq!(w("/etc/passwd.txt", "/r", ""), None);
        assert_eq!(w("../../x.php", "/r", ""), None);
        assert_eq!(LinkContext::new(Some("/r"), Some("/r/sub")).cwd_rel, "sub");
    }
}
