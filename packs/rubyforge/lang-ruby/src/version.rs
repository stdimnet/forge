//! The Ruby version a project asks for, from the files version managers read:
//! `.ruby-version` (rbenv, chruby, rvm, asdf with legacy files, mise), `.tool-versions`
//! (asdf, mise), `mise.toml` / `.mise.toml` and the Gemfile's `ruby "3.3.6"` (or
//! `ruby file: ".ruby-version"`) line, in that order. Also version comparison and the
//! match of a request (`3.3`, `ruby-3.3.6`, `3.3.6-p1`) against an installed version.

use serde::{Deserialize, Serialize};

/// A requested version and the file that asked for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRequest {
    /// As written, without the `ruby-` prefix (`3.3.6`, `3.3`, `jruby-9.4.8.0`).
    pub version: String,
    /// Project-relative file (`.ruby-version`, `.tool-versions`, `Gemfile`).
    pub file: String,
}

/// Files that may name the project's Ruby, in the order they are consulted.
pub const VERSION_FILES: [&str; 5] = [
    ".ruby-version",
    ".tool-versions",
    "mise.toml",
    ".mise.toml",
    "Gemfile",
];

fn clean(v: &str) -> Option<String> {
    let v = v.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    let v = v.strip_prefix("ruby-").unwrap_or(v);
    let v = v.strip_prefix("ruby ").unwrap_or(v).trim();
    (!v.is_empty() && !v.starts_with('#')).then(|| v.to_string())
}

/// `.ruby-version`: the first non-comment line.
pub fn parse_ruby_version(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .and_then(clean)
}

/// `.tool-versions`: `ruby 3.3.6 3.2.4` → the first version.
pub fn parse_tool_versions(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let l = l.split('#').next().unwrap_or("");
        let mut parts = l.split_whitespace();
        (parts.next()? == "ruby")
            .then(|| parts.next().and_then(clean))
            .flatten()
    })
}

/// `mise.toml`: `ruby = "3.3"` (or `ruby = ["3.3", "3.2"]`) under `[tools]`.
pub fn parse_mise_toml(text: &str) -> Option<String> {
    let mut in_tools = false;
    for l in text.lines() {
        let t = l.split('#').next().unwrap_or("").trim();
        if t.starts_with('[') {
            in_tools = t == "[tools]";
            continue;
        }
        if !in_tools {
            continue;
        }
        if let Some((k, v)) = t.split_once('=') {
            if k.trim().trim_matches('"') == "ruby" {
                let v = v.trim().trim_start_matches('[');
                return v.split(',').next().and_then(clean);
            }
        }
    }
    None
}

/// The Gemfile's `ruby "3.3.6"` / `ruby "~> 3.3.0"` / `ruby file: ".ruby-version"`:
/// `Ok(version)`, or `Err(file)` when it points to a version file.
pub fn parse_gemfile_ruby(text: &str) -> Option<Result<String, String>> {
    for l in text.lines() {
        let t = l.trim();
        let Some(rest) = t.strip_prefix("ruby ").or_else(|| t.strip_prefix("ruby(")) else {
            continue;
        };
        let rest = rest.trim().trim_end_matches(')');
        if let Some(f) = rest
            .strip_prefix("file:")
            .or_else(|| rest.strip_prefix(":file =>"))
        {
            return clean(f).map(Err);
        }
        let first = rest.split(',').next().unwrap_or("");
        let v = first
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .trim_start_matches(['~', '>', '=', '<', ' '])
            .trim();
        if let Some(v) = clean(v) {
            return Some(Ok(v));
        }
    }
    None
}

/// The project's requested version from its files (`(name, text)` pairs, `read` answers
/// for any project-relative path).
pub fn requested_version(read: &dyn Fn(&str) -> Option<String>) -> Option<VersionRequest> {
    let found = |file: &str, v: Option<String>| {
        v.map(|version| VersionRequest {
            version,
            file: file.into(),
        })
    };
    if let Some(r) = found(
        ".ruby-version",
        read(".ruby-version").and_then(|t| parse_ruby_version(&t)),
    ) {
        return Some(r);
    }
    if let Some(r) = found(
        ".tool-versions",
        read(".tool-versions").and_then(|t| parse_tool_versions(&t)),
    ) {
        return Some(r);
    }
    for f in ["mise.toml", ".mise.toml"] {
        if let Some(r) = found(f, read(f).and_then(|t| parse_mise_toml(&t))) {
            return Some(r);
        }
    }
    match read("Gemfile").and_then(|t| parse_gemfile_ruby(&t))? {
        Ok(v) => found("Gemfile", Some(v)),
        Err(file) => found(&file, read(&file).and_then(|t| parse_ruby_version(&t))),
    }
}

/// Numeric parts of a version (`3.3.6-p1` → `[3, 3, 6, 1]`), ignoring an engine prefix.
pub fn parts(v: &str) -> Vec<u64> {
    let v = v
        .rsplit_once('-')
        .filter(|(e, _)| e.chars().all(|c| c.is_ascii_alphabetic()))
        .map(|(_, r)| r)
        .unwrap_or(v);
    v.split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect()
}

/// The engine of a version name (`jruby-9.4` → `jruby`, `3.3.6` → `ruby`).
pub fn engine(v: &str) -> &str {
    match v.split_once('-') {
        Some((e, rest))
            if e.chars().all(|c| c.is_ascii_alphabetic())
                && rest.starts_with(|c: char| c.is_ascii_digit()) =>
        {
            e
        }
        _ => "ruby",
    }
}

/// Whether an installed `version` satisfies a request: same engine and the requested
/// parts as a prefix (`3.3` matches `3.3.6`).
pub fn matches(request: &str, version: &str) -> bool {
    let request = request.trim();
    if request.is_empty() || request == "system" {
        return false;
    }
    if engine(request) != engine(version) {
        return false;
    }
    let want = parts(request);
    let have = parts(version);
    !want.is_empty() && have.len() >= want.len() && have[..want.len()] == want[..]
}

/// Compare two versions numerically.
pub fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    parts(a).cmp(&parts(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn version_files() {
        assert_eq!(
            parse_ruby_version("# pinned\nruby-3.3.6\n"),
            Some("3.3.6".into())
        );
        assert_eq!(
            parse_tool_versions("nodejs 22.1.0\nruby 3.2.4 3.3.0 # two\n"),
            Some("3.2.4".into())
        );
        assert_eq!(
            parse_mise_toml(
                "[env]\nruby = \"x\"\n[tools]\nnode = \"22\"\nruby = [\"3.3\", \"3.2\"]\n"
            ),
            Some("3.3".into())
        );
        assert_eq!(
            parse_gemfile_ruby("source \"https://rubygems.org\"\nruby \"~> 3.3.0\"\n"),
            Some(Ok("3.3.0".into()))
        );
        assert_eq!(
            parse_gemfile_ruby("ruby file: \".ruby-version\"\n"),
            Some(Err(".ruby-version".into()))
        );
        assert_eq!(parse_gemfile_ruby("gem \"rubocop\"\n"), None);
    }

    #[test]
    fn requested_version_follows_the_managers_order() {
        let mut files = BTreeMap::new();
        files.insert("Gemfile", "ruby file: \".ruby-version\"\n");
        files.insert(".tool-versions", "ruby 3.2.4\n");
        let read = |f: &str| files.get(f).map(|s| s.to_string());
        assert_eq!(
            requested_version(&read),
            Some(VersionRequest {
                version: "3.2.4".into(),
                file: ".tool-versions".into()
            })
        );
        files.insert(".ruby-version", "3.3.6\n");
        let read = |f: &str| files.get(f).map(|s| s.to_string());
        assert_eq!(requested_version(&read).unwrap().file, ".ruby-version");
        let none: BTreeMap<&str, &str> = BTreeMap::new();
        assert_eq!(
            requested_version(&|f| none.get(f).map(|s| s.to_string())),
            None
        );
    }

    #[test]
    fn matching_and_ordering() {
        assert!(matches("3.3", "3.3.6"));
        assert!(matches("ruby-3.3.6", "3.3.6"));
        assert!(!matches("3.3.6", "3.3"));
        assert!(!matches("3.2", "3.3.6"));
        assert!(matches("jruby-9.4", "jruby-9.4.8.0"));
        assert!(!matches("9.4", "jruby-9.4.8.0"));
        assert_eq!(engine("truffleruby-24.1.1"), "truffleruby");
        assert_eq!(compare("3.10.0", "3.9.9"), std::cmp::Ordering::Greater);
        assert_eq!(parts("3.3.6-p1"), vec![3, 3, 6, 1]);
    }
}
