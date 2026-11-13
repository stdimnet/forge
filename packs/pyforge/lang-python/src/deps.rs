//! Declared dependencies: `requirements*.txt` (pip's format: options, `-r` includes, line
//! continuations, comments) and `pyproject.toml` (PEP 621 `project.dependencies` and
//! `optional-dependencies`, PEP 735 `dependency-groups`, Poetry's
//! `tool.poetry.*dependencies`), each with the position of its name for the editor.
//!
//! Positions are 0-based lines and UTF-16 columns, like CodeMirror and LSP.

use serde::{Deserialize, Serialize};

use crate::pep440::SpecifierSet;
use crate::pep508::{self, normalize};

/// Where a dependency is declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DeclaredIn {
    Requirements,
    Pep621,
    Poetry,
    DependencyGroup,
}

/// One declared dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Declared {
    pub name: String,
    pub normalized: String,
    /// PEP 440 specifiers (Poetry constraints are converted); empty = any version.
    pub specifier: String,
    /// The constraint as written, for messages (`^2.31` for Poetry).
    pub written: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<String>,
    /// Installed from a URL, path or VCS: the version is not checked.
    pub direct: bool,
    pub source: DeclaredIn,
    /// Optional group / extra (`dev`, `test`); `None` for the main dependencies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub line: u32,
    pub start: u32,
    pub end: u32,
}

impl Declared {
    pub fn specifiers(&self) -> Option<SpecifierSet> {
        SpecifierSet::parse(&self.specifier).ok()
    }
}

/// A syntax problem in a dependency file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepError {
    pub line: u32,
    pub start: u32,
    pub end: u32,
    pub message: String,
}

/// The dependencies of one file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepFile {
    pub deps: Vec<Declared>,
    pub errors: Vec<DepError>,
    /// `-r other.txt` / `-c constraints.txt` of a requirements file (as written).
    pub includes: Vec<String>,
    /// pyproject.toml: the project's own name (its imports are not third-party).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    /// pyproject.toml: `requires-python` (or Poetry's `python` dependency).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_python: Option<String>,
    /// pyproject.toml: PEP 621 `project.dependencies` exists (the target of "Add to
    /// pyproject"); `poetry` when only Poetry's table exists.
    pub has_pep621: bool,
    pub has_poetry: bool,
}

/// UTF-16 column of byte offset `byte` in `line`.
pub fn utf16_col(line: &str, byte: usize) -> u32 {
    line[..byte.min(line.len())].encode_utf16().count() as u32
}

/// Whether a path names a requirements file (`requirements.txt`, `requirements-dev.txt`,
/// `requirements/base.txt`, `constraints.txt`).
pub fn is_requirements_file(path: &str) -> bool {
    let p = path.replace('\\', "/");
    let name = p.rsplit('/').next().unwrap_or("");
    if !name.ends_with(".txt") && !name.ends_with(".in") {
        return false;
    }
    name.starts_with("requirements")
        || name.starts_with("constraints")
        || p.contains("requirements/")
}

/// Parse a requirements file.
pub fn requirements_txt(text: &str) -> DepFile {
    let mut out = DepFile::default();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let first = i;
        let raw = lines[i];
        // Continuation lines: the requirement is taken from the first physical line, the rest
        // is appended for parsing only.
        let mut joined = raw.to_string();
        while joined.trim_end().ends_with('\\') && i + 1 < lines.len() {
            let t = joined.trim_end();
            joined = format!("{} {}", &t[..t.len() - 1], lines[i + 1].trim());
            i += 1;
        }
        i += 1;
        let content = strip_comment(&joined);
        let trimmed = content.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('-') {
            let mut parts = trimmed.splitn(2, |c: char| c.is_whitespace() || c == '=');
            let opt = parts.next().unwrap_or("");
            let arg = parts.next().unwrap_or("").trim();
            if matches!(opt, "-r" | "--requirement" | "-c" | "--constraint") && !arg.is_empty() {
                out.includes.push(arg.to_string());
            }
            continue;
        }
        // Paths, URLs and archives are installed as they are.
        if trimmed.starts_with('.')
            || trimmed.starts_with('/')
            || trimmed.contains("://")
            || trimmed.ends_with(".whl")
            || trimmed.ends_with(".tar.gz")
        {
            continue;
        }
        // Per-requirement options (`--hash=…`) follow the requirement.
        let req_text = match trimmed.find(" --") {
            Some(p) => &trimmed[..p],
            None => trimmed,
        };
        let line_no = first as u32;
        let offset = raw.len() - raw.trim_start().len();
        match pep508::parse(req_text) {
            Ok(r) => {
                let in_first = |b: usize| (offset + b).min(raw.len());
                out.deps.push(Declared {
                    normalized: normalize(&r.name),
                    specifier: r.specifier.clone(),
                    written: r.specifier.clone(),
                    marker: r.marker.clone(),
                    direct: r.url.is_some(),
                    source: DeclaredIn::Requirements,
                    group: None,
                    line: line_no,
                    start: utf16_col(raw, in_first(r.name_start)),
                    end: utf16_col(raw, in_first(r.name_end)),
                    name: r.name,
                });
            }
            Err(message) => out.errors.push(DepError {
                line: line_no,
                start: utf16_col(raw, offset),
                end: utf16_col(raw, raw.trim_end().len()),
                message,
            }),
        }
    }
    out
}

/// Text before a `#` comment (a `#` at the start or after whitespace).
fn strip_comment(line: &str) -> &str {
    let b = line.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        if c == b'#' && (i == 0 || b[i - 1].is_ascii_whitespace()) {
            return &line[..i];
        }
    }
    line
}

/// Byte offsets of the line starts.
fn line_starts(text: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

/// Line and UTF-16 column of a byte offset.
fn position(text: &str, starts: &[usize], byte: usize) -> (u32, u32) {
    let line = starts.partition_point(|&s| s <= byte).saturating_sub(1);
    let line_text = text[starts[line]..].lines().next().unwrap_or("");
    (line as u32, utf16_col(line_text, byte - starts[line]))
}

/// TOML table headers: `(name, byte offset of the header line, end of the section)`.
fn sections(text: &str) -> Vec<(String, usize, usize)> {
    let starts = line_starts(text);
    let mut out: Vec<(String, usize, usize)> = Vec::new();
    for &s in &starts {
        let line = text[s..].lines().next().unwrap_or("").trim();
        if line.starts_with('[') {
            let name = line
                .trim_start_matches('[')
                .split(']')
                .next()
                .unwrap_or("")
                .trim()
                .replace([' ', '"', '\''], "");
            if let Some(last) = out.last_mut() {
                last.2 = s;
            }
            out.push((name, s, text.len()));
        }
    }
    out
}

fn section<'a>(
    all: &'a [(String, usize, usize)],
    name: &str,
) -> Option<&'a (String, usize, usize)> {
    all.iter().find(|(n, _, _)| n == name)
}

/// Byte offset of `key =` at a line start within `[from, to)`.
fn find_key(text: &str, from: usize, to: usize, key: &str) -> Option<usize> {
    let starts = line_starts(text);
    starts
        .iter()
        .filter(|&&s| s >= from && s < to)
        .find_map(|&s| {
            let line = text[s..].lines().next().unwrap_or("");
            let lead = line.len() - line.trim_start().len();
            let rest = line.trim_start();
            let rest_key = rest.trim_start_matches(['"', '\'']);
            let quoted = rest.len() - rest_key.len();
            let after = rest_key.strip_prefix(key)?;
            let after = after.trim_start_matches(['"', '\'']);
            after
                .trim_start()
                .starts_with('=')
                .then_some(s + lead + quoted)
        })
}

/// Byte offset of the quoted string `s` at or after `from`.
fn find_string(text: &str, from: usize, s: &str) -> Option<usize> {
    let rest = text.get(from..)?;
    ["\"", "'"]
        .iter()
        .filter_map(|q| rest.find(&format!("{q}{s}{q}")).map(|p| from + p + 1))
        .min()
}

/// Poetry constraint → PEP 440 specifiers (`^1.2` → `>=1.2,<2`); `None` for alternatives
/// (`||`) that a specifier set cannot express.
pub fn poetry_to_pep440(constraint: &str) -> Option<String> {
    let c = constraint.trim();
    if c.contains("||") {
        return None;
    }
    if c.is_empty() || c == "*" {
        return Some(String::new());
    }
    let mut out: Vec<String> = Vec::new();
    for part in c.split(',').flat_map(|p| {
        // `>=1.0 <2.0` (space separated) is also accepted by Poetry.
        let p = p.trim();
        let mut pieces = Vec::new();
        let mut cur = String::new();
        for tok in p.split_whitespace() {
            if crate::pep440::Operator::prefix(tok).is_some() || tok.starts_with(['^', '~']) {
                if !cur.is_empty() {
                    pieces.push(std::mem::take(&mut cur));
                }
                cur.push_str(tok);
            } else {
                cur.push_str(tok);
            }
        }
        if !cur.is_empty() {
            pieces.push(cur);
        }
        pieces
    }) {
        let bump = |release: &[u64], at: usize| -> String {
            let mut r: Vec<u64> = release[..=at].to_vec();
            r[at] += 1;
            r.iter().map(u64::to_string).collect::<Vec<_>>().join(".")
        };
        if let Some(v) = part.strip_prefix('^') {
            let ver = crate::pep440::Version::parse(v)?;
            let r = &ver.release;
            let at = r.iter().position(|&n| n != 0).unwrap_or(r.len() - 1);
            out.push(format!(">={v}"));
            out.push(format!("<{}", bump(r, at)));
        } else if let Some(v) = part.strip_prefix("~=") {
            out.push(format!("~={v}"));
        } else if let Some(v) = part.strip_prefix('~') {
            let ver = crate::pep440::Version::parse(v)?;
            let r = &ver.release;
            let at = if r.len() >= 2 { 1 } else { 0 };
            out.push(format!(">={v}"));
            out.push(format!("<{}", bump(r, at)));
        } else if crate::pep440::Operator::prefix(&part).is_some() {
            out.push(part.replace(' ', ""));
        } else {
            out.push(format!("=={part}"));
        }
    }
    let joined = out.join(",");
    SpecifierSet::parse(&joined).ok().map(|_| joined)
}

/// Parse a pyproject.toml.
pub fn pyproject(text: &str) -> DepFile {
    let mut out = DepFile::default();
    let doc: toml::Table = match text.parse() {
        Ok(t) => t,
        Err(e) => {
            let (line, col) = e
                .span()
                .map(|s| position(text, &line_starts(text), s.start))
                .unwrap_or((0, 0));
            out.errors.push(DepError {
                line,
                start: col,
                end: col + 1,
                message: e.message().to_string(),
            });
            return out;
        }
    };
    let starts = line_starts(text);
    let secs = sections(text);
    let project = doc.get("project").and_then(toml::Value::as_table);
    out.project_name = project
        .and_then(|p| p.get("name"))
        .and_then(toml::Value::as_str)
        .map(String::from);
    out.requires_python = project
        .and_then(|p| p.get("requires-python"))
        .and_then(toml::Value::as_str)
        .map(String::from);

    let push_pep508 =
        |out: &mut DepFile, s: &str, at: Option<usize>, source, group: Option<&str>| {
            let Some(at) = at else { return };
            let (line, col) = position(text, &starts, at);
            match pep508::parse(s) {
                Ok(r) => {
                    let line_text = text[starts[line as usize]..].lines().next().unwrap_or("");
                    let base = at - starts[line as usize];
                    out.deps.push(Declared {
                        normalized: normalize(&r.name),
                        specifier: r.specifier.clone(),
                        written: r.specifier.clone(),
                        marker: r.marker.clone(),
                        direct: r.url.is_some(),
                        source,
                        group: group.map(String::from),
                        line,
                        start: utf16_col(line_text, base + r.name_start),
                        end: utf16_col(line_text, base + r.name_end),
                        name: r.name,
                    });
                }
                Err(message) => out.errors.push(DepError {
                    line,
                    start: col,
                    end: col + s.encode_utf16().count() as u32,
                    message,
                }),
            }
        };

    // PEP 621.
    if let Some(project) = project {
        let (from, to) = section(&secs, "project")
            .map(|(_, s, e)| (*s, *e))
            .unwrap_or((0, text.len()));
        if let Some(deps) = project.get("dependencies").and_then(toml::Value::as_array) {
            out.has_pep621 = true;
            let mut cursor = find_key(text, from, to, "dependencies").unwrap_or(from);
            for d in deps.iter().filter_map(toml::Value::as_str) {
                let at = find_string(text, cursor, d);
                if let Some(a) = at {
                    cursor = a + d.len();
                }
                push_pep508(&mut out, d, at, DeclaredIn::Pep621, None);
            }
        }
        if let Some(extras) = project
            .get("optional-dependencies")
            .and_then(toml::Value::as_table)
        {
            let (from, to) = section(&secs, "project.optional-dependencies")
                .map(|(_, s, e)| (*s, *e))
                .unwrap_or((from, to));
            for (extra, deps) in extras {
                let mut cursor = find_key(text, from, to, extra).unwrap_or(from);
                for d in deps
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(toml::Value::as_str)
                {
                    let at = find_string(text, cursor, d);
                    if let Some(a) = at {
                        cursor = a + d.len();
                    }
                    push_pep508(&mut out, d, at, DeclaredIn::Pep621, Some(extra));
                }
            }
        }
    }
    // PEP 735.
    if let Some(groups) = doc.get("dependency-groups").and_then(toml::Value::as_table) {
        let (from, to) = section(&secs, "dependency-groups")
            .map(|(_, s, e)| (*s, *e))
            .unwrap_or((0, text.len()));
        for (group, deps) in groups {
            let mut cursor = find_key(text, from, to, group).unwrap_or(from);
            for d in deps
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(toml::Value::as_str)
            {
                let at = find_string(text, cursor, d);
                if let Some(a) = at {
                    cursor = a + d.len();
                }
                push_pep508(&mut out, d, at, DeclaredIn::DependencyGroup, Some(group));
            }
        }
    }
    // Poetry.
    let poetry = doc
        .get("tool")
        .and_then(|t| t.get("poetry"))
        .and_then(toml::Value::as_table);
    if let Some(poetry) = poetry {
        if out.project_name.is_none() {
            out.project_name = poetry
                .get("name")
                .and_then(toml::Value::as_str)
                .map(String::from);
        }
        let mut tables: Vec<(String, Option<String>, &toml::Table)> = Vec::new();
        if let Some(t) = poetry.get("dependencies").and_then(toml::Value::as_table) {
            out.has_poetry = true;
            tables.push(("tool.poetry.dependencies".into(), None, t));
        }
        if let Some(t) = poetry
            .get("dev-dependencies")
            .and_then(toml::Value::as_table)
        {
            tables.push(("tool.poetry.dev-dependencies".into(), Some("dev".into()), t));
        }
        if let Some(groups) = poetry.get("group").and_then(toml::Value::as_table) {
            for (g, v) in groups {
                if let Some(t) = v.get("dependencies").and_then(toml::Value::as_table) {
                    tables.push((
                        format!("tool.poetry.group.{g}.dependencies"),
                        Some(g.clone()),
                        t,
                    ));
                }
            }
        }
        for (header, group, table) in tables {
            let Some((_, from, to)) = section(&secs, &header) else {
                continue;
            };
            for (name, value) in table {
                if name == "python" {
                    if out.requires_python.is_none() {
                        out.requires_python = value.as_str().map(String::from);
                    }
                    continue;
                }
                let Some(at) = find_key(text, *from, *to, name) else {
                    continue;
                };
                let (line, start) = position(text, &starts, at);
                let (written, direct) = match value {
                    toml::Value::String(s) => (s.clone(), false),
                    toml::Value::Table(t) => (
                        t.get("version")
                            .and_then(toml::Value::as_str)
                            .unwrap_or("*")
                            .to_string(),
                        t.contains_key("git") || t.contains_key("path") || t.contains_key("url"),
                    ),
                    _ => ("*".to_string(), false),
                };
                let specifier = poetry_to_pep440(&written).unwrap_or_default();
                out.deps.push(Declared {
                    name: name.clone(),
                    normalized: normalize(name),
                    specifier,
                    written,
                    marker: None,
                    direct,
                    source: DeclaredIn::Poetry,
                    group: group.clone(),
                    line,
                    start,
                    end: start + name.encode_utf16().count() as u32,
                });
            }
        }
    }
    out
}

/// Add `requirement` (`requests>=2.31`) to a requirements file: a new last line.
pub fn add_to_requirements(text: &str, requirement: &str) -> String {
    let mut out = text.to_string();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(requirement);
    out.push('\n');
    out
}

/// Add `requirement` to pyproject.toml: into PEP 621 `project.dependencies` (created in
/// `[project]` when missing), or Poetry's `[tool.poetry.dependencies]` as `name = "constraint"`
/// when the project only has that. `None` when there is no place for it.
pub fn add_to_pyproject(text: &str, requirement: &str) -> Option<String> {
    let parsed = pyproject(text);
    let secs = sections(text);
    if !parsed.has_pep621 && parsed.has_poetry {
        let req = pep508::parse(requirement).ok()?;
        let (_, _, end) = section(&secs, "tool.poetry.dependencies")?;
        let constraint = match req.specifier.trim() {
            "" => "*".to_string(),
            s => s.to_string(),
        };
        let line = format!("{} = \"{}\"\n", req.name, constraint);
        return Some(insert_at_section_end(text, *end, &line));
    }
    let (_, from, to) = section(&secs, "project")?;
    let item = format!("\"{}\"", requirement.replace('"', "\\\""));
    let Some(key) = find_key(text, *from, *to, "dependencies") else {
        // No dependencies yet: a new key at the end of [project].
        let line = format!("dependencies = [\n    {item},\n]\n");
        return Some(insert_at_section_end(text, *to, &line));
    };
    let open = key + text[key..].find('[')?;
    let close = matching_bracket(text, open)?;
    let inner = &text[open + 1..close];
    if inner.trim().is_empty() {
        return Some(format!(
            "{}[\n    {item},\n]{}",
            &text[..open],
            &text[close + 1..]
        ));
    }
    if inner.contains('\n') {
        // Multi-line array: a new line before `]`, keeping the indentation of the items.
        let body = inner.trim_end();
        let indent = inner
            .lines()
            .find(|l| !l.trim().is_empty())
            .map(|l| &l[..l.len() - l.trim_start().len()])
            .unwrap_or("    ");
        let comma = if body.ends_with(',') { "" } else { "," };
        let insert_at = open + 1 + body.len();
        return Some(format!(
            "{}{comma}\n{indent}{item},\n{}",
            &text[..insert_at],
            &text[close..]
        ));
    }
    let body = inner.trim_end().trim_end_matches(',');
    Some(format!(
        "{}[{body}, {item}]{}",
        &text[..open],
        &text[close + 1..]
    ))
}

/// Insert `line` before the blank lines that end a section.
fn insert_at_section_end(text: &str, end: usize, line: &str) -> String {
    let head = text[..end].trim_end_matches(['\n', '\r', ' ']);
    let tail = &text[end..];
    let sep = if tail.is_empty() { "" } else { "\n" };
    format!("{head}\n{line}{sep}{tail}")
}

/// The `]` closing the `[` at `open` (strings skipped).
fn matching_bracket(text: &str, open: usize) -> Option<usize> {
    let b = text.as_bytes();
    let mut depth = 0;
    let mut quote: Option<u8> = None;
    let mut i = open;
    while i < b.len() {
        let c = b[i];
        match quote {
            Some(q) if c == b'\\' && q == b'"' => i += 1,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                b'"' | b'\'' => quote = Some(c),
                b'#' => {
                    while i < b.len() && b[i] != b'\n' {
                        i += 1;
                    }
                }
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            },
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const REQS: &str = "# Web\nDjango>=4.2,<5.0  # LTS\nrequests[socks]==2.32.3 \\\n    --hash=sha256:abc\n-r requirements-dev.txt\n-e .\n./local/pkg\ngit+https://github.com/a/b.git\nnumpy ; python_version >= \"3.10\"\nbroken >=\n";

    #[test]
    fn parses_requirements_files() {
        let f = requirements_txt(REQS);
        let names: Vec<&str> = f.deps.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["Django", "requests", "numpy"]);
        assert_eq!(f.deps[0].line, 1);
        assert_eq!((f.deps[0].start, f.deps[0].end), (0, 6));
        assert_eq!(f.deps[0].specifier, ">=4.2,<5.0");
        assert_eq!(f.deps[1].normalized, "requests");
        assert_eq!(f.deps[1].specifier, "==2.32.3");
        assert_eq!(
            f.deps[2].marker.as_deref(),
            Some("python_version >= \"3.10\"")
        );
        assert_eq!(f.includes, vec!["requirements-dev.txt"]);
        assert_eq!(f.errors.len(), 1);
        assert_eq!(f.errors[0].line, 9);
        assert!(is_requirements_file("requirements-dev.txt"));
        assert!(is_requirements_file("deps/requirements/base.txt"));
        assert!(!is_requirements_file("notes.txt"));
    }

    const PEP621: &str = r#"[project]
name = "shop-api"
requires-python = ">=3.11"
dependencies = [
    "fastapi>=0.115",
    "SQLAlchemy[asyncio]~=2.0",
    'pydantic-settings',
]

[project.optional-dependencies]
test = ["pytest>=8", "httpx"]

[dependency-groups]
lint = ["ruff"]

[tool.ruff]
line-length = 100
"#;

    #[test]
    fn parses_pep621_and_groups() {
        let f = pyproject(PEP621);
        assert!(f.has_pep621);
        assert_eq!(f.project_name.as_deref(), Some("shop-api"));
        assert_eq!(f.requires_python.as_deref(), Some(">=3.11"));
        let got: Vec<(&str, u32, u32, Option<&str>)> = f
            .deps
            .iter()
            .map(|d| (d.name.as_str(), d.line, d.start, d.group.as_deref()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("fastapi", 4, 5, None),
                ("SQLAlchemy", 5, 5, None),
                ("pydantic-settings", 6, 5, None),
                ("pytest", 10, 9, Some("test")),
                ("httpx", 10, 22, Some("test")),
                ("ruff", 13, 9, Some("lint")),
            ]
        );
    }

    const POETRY: &str = r#"[tool.poetry]
name = "billing"

[tool.poetry.dependencies]
python = "^3.11"
requests = "^2.31"
celery = { version = "~5.3", extras = ["redis"] }
internal = { git = "https://example.com/internal.git" }

[tool.poetry.group.dev.dependencies]
pytest = "*"
"#;

    #[test]
    fn parses_poetry_tables() {
        let f = pyproject(POETRY);
        assert!(f.has_poetry && !f.has_pep621);
        assert_eq!(f.requires_python.as_deref(), Some("^3.11"));
        let dep = |n: &str| f.deps.iter().find(|d| d.name == n).unwrap();
        let r = dep("requests");
        assert_eq!((r.line, r.start, r.end), (5, 0, 8));
        assert_eq!(r.specifier, ">=2.31,<3");
        assert_eq!(dep("celery").specifier, ">=5.3,<5.4");
        assert_eq!(dep("celery").line, 6);
        assert!(dep("internal").direct);
        assert_eq!(dep("pytest").group.as_deref(), Some("dev"));
        assert_eq!(f.deps.len(), 4);
    }

    #[test]
    fn converts_poetry_constraints() {
        assert_eq!(poetry_to_pep440("^1.2.3").unwrap(), ">=1.2.3,<2");
        assert_eq!(poetry_to_pep440("^0.2.3").unwrap(), ">=0.2.3,<0.3");
        assert_eq!(poetry_to_pep440("~1.2.3").unwrap(), ">=1.2.3,<1.3");
        assert_eq!(poetry_to_pep440("*").unwrap(), "");
        assert_eq!(poetry_to_pep440("1.4").unwrap(), "==1.4");
        assert_eq!(poetry_to_pep440(">=1.0 <2.0").unwrap(), ">=1.0,<2.0");
        assert_eq!(poetry_to_pep440(">=1, <2").unwrap(), ">=1,<2");
        assert_eq!(poetry_to_pep440("^1 || ^2"), None);
    }

    #[test]
    fn reports_toml_errors() {
        let f = pyproject("[project\nname = 1");
        assert_eq!(f.errors.len(), 1);
    }

    #[test]
    fn adds_requirements() {
        assert_eq!(
            add_to_requirements("flask", "requests"),
            "flask\nrequests\n"
        );
        let added = add_to_pyproject(PEP621, "rich>=13").unwrap();
        assert!(
            added.contains("    'pydantic-settings',\n    \"rich>=13\",\n]"),
            "{added}"
        );
        assert!(pyproject(&added).deps.iter().any(|d| d.name == "rich"));

        let inline = "[project]\nname = \"x\"\ndependencies = [\"a\"]\n";
        assert_eq!(
            add_to_pyproject(inline, "b").unwrap(),
            "[project]\nname = \"x\"\ndependencies = [\"a\", \"b\"]\n"
        );
        let none = "[project]\nname = \"x\"\n\n[tool.x]\ny = 1\n";
        let added = add_to_pyproject(none, "b").unwrap();
        assert_eq!(
            added,
            "[project]\nname = \"x\"\ndependencies = [\n    \"b\",\n]\n\n[tool.x]\ny = 1\n"
        );
        let poetry = add_to_pyproject(POETRY, "rich>=13").unwrap();
        assert!(poetry.contains("internal = { git = \"https://example.com/internal.git\" }\nrich = \">=13\"\n\n[tool.poetry.group.dev.dependencies]"), "{poetry}");
        assert!(add_to_pyproject("[tool.black]\n", "x").is_none());
    }
}
