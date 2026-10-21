//! EditorConfig (`.editorconfig`, <https://editorconfig.org>) support: parsing, section
//! globs and the properties that apply to one file.
//!
//! Supported properties: `indent_style`, `indent_size`, `tab_width`, `end_of_line`,
//! `charset`, `trim_trailing_whitespace`, `insert_final_newline` and `max_line_length`.

use serde::{Deserialize, Serialize};

use crate::encoding::Encoding;
use crate::eol::LineSeparator;

/// One `.editorconfig` file: its folder (workspace-relative, `""` for the project root) and
/// its content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorConfigFile {
    pub dir: String,
    pub content: String,
}

/// The EditorConfig properties that apply to a file (unset ones are `None`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorConfigProps {
    pub use_tabs: Option<bool>,
    pub indent_size: Option<u8>,
    pub tab_width: Option<u8>,
    pub end_of_line: Option<LineSeparator>,
    pub charset: Option<(Encoding, bool)>,
    pub trim_trailing_whitespace: Option<bool>,
    pub insert_final_newline: Option<bool>,
    pub max_line_length: Option<u32>,
}

impl EditorConfigProps {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

struct Section {
    glob: String,
    pairs: Vec<(String, String)>,
}

struct Parsed {
    root: bool,
    sections: Vec<Section>,
}

fn parse(content: &str) -> Parsed {
    let mut parsed = Parsed {
        root: false,
        sections: Vec::new(),
    };
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            parsed.sections.push(Section {
                glob: line[1..line.len() - 1].to_string(),
                pairs: Vec::new(),
            });
            continue;
        }
        let Some((key, value)) = line.split_once(['=', ':']) else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().to_string();
        match parsed.sections.last_mut() {
            Some(section) => section.pairs.push((key, value)),
            None if key == "root" => parsed.root = value.eq_ignore_ascii_case("true"),
            None => {}
        }
    }
    parsed
}

/// The properties for `path` (workspace-relative, `/` separators). Files closer to `path`
/// win; a file with `root = true` hides the ones above it.
pub fn properties_for(path: &str, files: &[EditorConfigFile]) -> EditorConfigProps {
    let mut applicable: Vec<&EditorConfigFile> = files
        .iter()
        .filter(|f| {
            let dir = f.dir.trim_matches('/');
            dir.is_empty() || path.starts_with(&format!("{dir}/"))
        })
        .collect();
    // Closest first, to find the root.
    applicable.sort_by_key(|f| std::cmp::Reverse(f.dir.trim_matches('/').len()));
    let mut chain = Vec::new();
    for f in applicable {
        let parsed = parse(&f.content);
        let root = parsed.root;
        chain.push((f.dir.trim_matches('/').to_string(), parsed));
        if root {
            break;
        }
    }
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (dir, parsed) in chain.iter().rev() {
        let rel = if dir.is_empty() {
            path
        } else {
            &path[dir.len() + 1..]
        };
        for section in &parsed.sections {
            if section_matches(&section.glob, rel) {
                pairs.extend(section.pairs.iter().cloned());
            }
        }
    }
    let mut props = EditorConfigProps::default();
    let mut indent_is_tab = false;
    for (key, value) in pairs {
        let v = value.to_ascii_lowercase();
        // `unset` removes a property set by an earlier section.
        let unset = v == "unset";
        match key.as_str() {
            "indent_style" => {
                props.use_tabs = match v.as_str() {
                    "tab" => Some(true),
                    "space" => Some(false),
                    _ => None,
                }
            }
            "indent_size" => {
                indent_is_tab = v == "tab";
                props.indent_size = if unset {
                    None
                } else {
                    v.parse().ok().filter(|n| *n > 0)
                };
            }
            "tab_width" => {
                props.tab_width = if unset {
                    None
                } else {
                    v.parse().ok().filter(|n| *n > 0)
                }
            }
            "end_of_line" => props.end_of_line = LineSeparator::parse(&v).filter(|_| v != "system"),
            "charset" => props.charset = Encoding::parse(&v),
            "trim_trailing_whitespace" => props.trim_trailing_whitespace = parse_bool(&v),
            "insert_final_newline" => props.insert_final_newline = parse_bool(&v),
            "max_line_length" => props.max_line_length = v.parse().ok(),
            _ => {}
        }
    }
    // `indent_size = tab` means "the tab width".
    if indent_is_tab {
        props.indent_size = props.tab_width;
    }
    props
}

fn parse_bool(v: &str) -> Option<bool> {
    match v {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Whether a section glob matches `rel` (relative to the `.editorconfig` folder). Globs
/// without `/` match the file name in any folder.
pub fn section_matches(glob: &str, rel: &str) -> bool {
    let glob = glob.trim();
    let pattern = if glob.contains('/') {
        glob.trim_start_matches('/').to_string()
    } else {
        format!("**/{glob}")
    };
    expand_braces(&pattern)
        .iter()
        .any(|p| glob_match(p.as_bytes(), rel.as_bytes()))
}

/// `{a,b}` alternatives expanded into separate patterns (nested braces included).
fn expand_braces(pattern: &str) -> Vec<String> {
    let bytes = pattern.as_bytes();
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_string()];
    };
    let mut depth = 0;
    let mut close = None;
    let mut commas = Vec::new();
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            b',' if depth == 1 => commas.push(i),
            _ => {}
        }
    }
    let Some(close) = close else {
        return vec![pattern.to_string()];
    };
    let (head, tail) = (&pattern[..open], &pattern[close + 1..]);
    let mut bounds = vec![open];
    bounds.extend(&commas);
    bounds.push(close);
    let mut out = Vec::new();
    for w in bounds.windows(2) {
        let alt = &pattern[w[0] + 1..w[1]];
        for expanded in expand_braces(&format!("{head}{alt}{tail}")) {
            out.push(expanded);
        }
    }
    out
}

fn glob_match(p: &[u8], s: &[u8]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some(b'*') if p.get(1) == Some(&b'*') => {
            // `**/` also matches no folder at all.
            let rest = &p[2..];
            if rest.first() == Some(&b'/') && glob_match(&rest[1..], s) {
                return true;
            }
            (0..=s.len()).any(|i| glob_match(rest, &s[i..]))
        }
        Some(b'*') => {
            let rest = &p[1..];
            for i in 0..=s.len() {
                if glob_match(rest, &s[i..]) {
                    return true;
                }
                if i < s.len() && s[i] == b'/' {
                    break;
                }
            }
            false
        }
        Some(b'?') => !s.is_empty() && s[0] != b'/' && glob_match(&p[1..], &s[1..]),
        Some(b'[') => {
            let Some(end) = p.iter().position(|&b| b == b']') else {
                return s.first() == Some(&b'[') && glob_match(&p[1..], &s[1..]);
            };
            let Some(&c) = s.first() else {
                return false;
            };
            let class = &p[1..end];
            let (negate, class) = match class.first() {
                Some(b'!') | Some(b'^') => (true, &class[1..]),
                _ => (false, class),
            };
            let mut hit = false;
            let mut i = 0;
            while i < class.len() {
                if i + 2 < class.len() && class[i + 1] == b'-' {
                    hit |= (class[i]..=class[i + 2]).contains(&c);
                    i += 3;
                } else {
                    hit |= class[i] == c;
                    i += 1;
                }
            }
            hit != negate && glob_match(&p[end + 1..], &s[1..])
        }
        Some(&c) => s.first() == Some(&c) && glob_match(&p[1..], &s[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(dir: &str, content: &str) -> EditorConfigFile {
        EditorConfigFile {
            dir: dir.into(),
            content: content.into(),
        }
    }

    const ROOT: &str = "root = true\n\n[*]\nindent_style = space\nindent_size = 4\nend_of_line = lf\ncharset = utf-8\ntrim_trailing_whitespace = true\ninsert_final_newline = true\n\n[*.{js,ts,json,yml}]\nindent_size = 2\n\n[Makefile]\nindent_style = tab\n\n[*.md]\ntrim_trailing_whitespace = false\n";

    #[test]
    fn applies_sections_in_order() {
        let files = [file("", ROOT)];
        let p = properties_for("app/User.php", &files);
        assert_eq!(p.use_tabs, Some(false));
        assert_eq!(p.indent_size, Some(4));
        assert_eq!(p.end_of_line, Some(LineSeparator::Lf));
        assert_eq!(p.charset, Some((Encoding::Utf8, false)));
        assert_eq!(p.trim_trailing_whitespace, Some(true));
        assert_eq!(properties_for("src/app.ts", &files).indent_size, Some(2));
        assert_eq!(properties_for("Makefile", &files).use_tabs, Some(true));
        assert_eq!(
            properties_for("docs/README.md", &files).trim_trailing_whitespace,
            Some(false)
        );
    }

    #[test]
    fn closer_files_win_and_root_stops_the_search() {
        let files = [
            file("", "[*]\nindent_size = 8\nmax_line_length = 100\n"),
            file("web", "root = true\n[*]\nindent_size = 3\n"),
            file("web/sub", "[*.css]\nindent_size = 1\n"),
        ];
        let p = properties_for("web/sub/a.css", &files);
        assert_eq!(p.indent_size, Some(1));
        // The project root file is above `root = true`.
        assert_eq!(p.max_line_length, None);
        assert_eq!(properties_for("web/a.js", &files).indent_size, Some(3));
        assert_eq!(properties_for("other/a.js", &files).indent_size, Some(8));
    }

    #[test]
    fn indent_size_tab_uses_tab_width() {
        let files = [file(
            "",
            "[*]\nindent_style = tab\nindent_size = tab\ntab_width = 3\n",
        )];
        let p = properties_for("a.go", &files);
        assert_eq!(
            (p.use_tabs, p.indent_size, p.tab_width),
            (Some(true), Some(3), Some(3))
        );
    }

    #[test]
    fn matches_globs() {
        assert!(section_matches("*.php", "app/Models/User.php"));
        assert!(section_matches("*", "a/b/c"));
        assert!(!section_matches("/*.php", "app/User.php"));
        assert!(section_matches("/*.php", "index.php"));
        assert!(section_matches("app/**.php", "app/Models/User.php"));
        assert!(section_matches("lib/**/*.js", "lib/a.js"));
        assert!(section_matches("{package.json,*.yml}", "package.json"));
        assert!(section_matches("*.{js,{ts,tsx}}", "src/a.tsx"));
        assert!(section_matches("file[0-9].txt", "file3.txt"));
        assert!(!section_matches("file[!0-9].txt", "file3.txt"));
        assert!(section_matches("?.c", "x.c"));
        assert!(!section_matches("*.php", "app/User.phpx"));
    }
}
