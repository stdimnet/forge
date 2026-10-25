//! Completion contexts in dependency files: a package name or a version after an operator,
//! in `requirements.txt` lines and in the dependency strings of pyproject.toml. The app
//! fills in the candidates (installed packages, PyPI names and releases).

use serde::{Deserialize, Serialize};

use crate::pep440::{Operator, Version};

/// What is being typed at the caret. Offsets are UTF-16, into the whole text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Context {
    Name {
        from: u32,
        prefix: String,
    },
    Version {
        from: u32,
        prefix: String,
        package: String,
    },
}

/// Byte offset of UTF-16 offset `utf16` in `text`.
pub fn byte_offset(text: &str, utf16: u32) -> usize {
    let mut units = 0u32;
    for (i, c) in text.char_indices() {
        if units >= utf16 {
            return i;
        }
        units += c.len_utf16() as u32;
    }
    text.len()
}

fn utf16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

/// The context in one requirement (`line` up to the caret), whose start is UTF-16 offset
/// `base` of the document.
fn in_requirement(before: &str, base: u32) -> Option<Context> {
    let lead = before.len() - before.trim_start().len();
    let rest = &before[lead..];
    let name_len = rest
        .bytes()
        .take_while(|c| c.is_ascii_alphanumeric() || b"-_.".contains(c))
        .count();
    let name = &rest[..name_len];
    let after = &rest[name_len..];
    if after.is_empty() {
        return Some(Context::Name {
            from: base + utf16_len(&before[..lead]),
            prefix: name.to_string(),
        });
    }
    if name.is_empty() || after.contains(';') || after.contains('@') {
        return None;
    }
    // Skip extras, then look for the last operator before the caret.
    let after = match after.find(']') {
        Some(p) if after.trim_start().starts_with('[') => &after[p + 1..],
        _ if after.trim_start().starts_with('[') => return None,
        _ => after,
    };
    let spec_start = before.len() - after.len();
    let last_part_start = after.rfind(',').map(|p| p + 1).unwrap_or(0);
    let part = &after[last_part_start..];
    let part = part.trim_start().trim_start_matches('(').trim_start();
    let (_, op_len) = Operator::prefix(part)?;
    let version = &part[op_len..];
    let version_trim = version.trim_start();
    let version_start = before.len() - version_trim.len();
    debug_assert!(version_start >= spec_start);
    Some(Context::Version {
        from: base + utf16_len(&before[..version_start]),
        prefix: version_trim.to_string(),
        package: name.to_string(),
    })
}

/// Completion context of a requirements file at UTF-16 `offset`.
pub fn requirements_context(text: &str, offset: u32) -> Option<Context> {
    let at = byte_offset(text, offset);
    let line_start = text[..at].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let before = &text[line_start..at];
    let t = before.trim_start();
    if t.starts_with('-') || t.starts_with('#') || before.contains(" #") {
        return None;
    }
    in_requirement(before, utf16_len(&text[..line_start]))
}

/// Completion context inside a dependency string of pyproject.toml (`"requests>=2.` in
/// `dependencies = [...]`, `optional-dependencies` and `dependency-groups`), or a key of
/// a Poetry dependency table.
pub fn pyproject_context(text: &str, offset: u32) -> Option<Context> {
    let at = byte_offset(text, offset);
    let line_start = text[..at].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let before = &text[line_start..at];
    let section = text[..line_start]
        .lines()
        .rev()
        .map(str::trim)
        .find(|l| l.starts_with('['))
        .map(|l| l.trim_matches(['[', ']']).trim().to_string())
        .unwrap_or_default();
    let in_list_section = section == "project"
        || section == "project.optional-dependencies"
        || section == "dependency-groups";
    if in_list_section {
        // Inside an open string?
        let quotes: Vec<usize> = before.match_indices(['"', '\'']).map(|(i, _)| i).collect();
        if quotes.len() % 2 == 1 {
            let open = *quotes.last()?;
            if section == "project" && !inside_dependencies(text, line_start, before) {
                return None;
            }
            let inner = &before[open + 1..];
            return in_requirement(inner, utf16_len(&text[..line_start + open + 1]));
        }
        return None;
    }
    let poetry = section.starts_with("tool.poetry.")
        && (section.ends_with("dependencies") || section.ends_with("dev-dependencies"));
    if poetry && !before.contains('=') && !before.trim().is_empty() {
        let lead = before.len() - before.trim_start().len();
        return Some(Context::Name {
            from: utf16_len(&text[..line_start + lead]),
            prefix: before.trim_start().to_string(),
        });
    }
    None
}

/// Whether the caret line belongs to the `dependencies = [` array of `[project]`.
fn inside_dependencies(text: &str, line_start: usize, before: &str) -> bool {
    if before.trim_start().starts_with("dependencies") {
        return true;
    }
    for l in text[..line_start].lines().rev() {
        let t = l.trim();
        if t.starts_with('[') {
            return false;
        }
        if t.ends_with(']') && !t.contains('[') {
            return false;
        }
        if let Some(key) = t.split('=').next() {
            if t.contains('=') && !t.starts_with('"') && !t.starts_with('\'') {
                return key.trim() == "dependencies";
            }
        }
    }
    false
}

/// Versions to offer for a package, newest first: final releases before pre-releases.
pub fn sort_versions(versions: &[String]) -> Vec<String> {
    let mut parsed: Vec<(Version, &String)> = versions
        .iter()
        .filter_map(|v| Version::parse(v).map(|p| (p, v)))
        .collect();
    parsed.sort_by(|a, b| {
        a.0.is_prerelease()
            .cmp(&b.0.is_prerelease())
            .then_with(|| b.0.cmp(&a.0))
    });
    parsed.into_iter().map(|(_, v)| v.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> (String, u32) {
        let pos = text.find('|').unwrap();
        let clean = text.replace('|', "");
        (clean, utf16_len(&text[..pos]))
    }

    #[test]
    fn requirements_contexts() {
        let (t, o) = at("flask\nreq|");
        assert_eq!(
            requirements_context(&t, o),
            Some(Context::Name {
                from: 6,
                prefix: "req".into()
            })
        );
        let (t, o) = at("requests[socks]>=2.3|");
        assert_eq!(
            requirements_context(&t, o),
            Some(Context::Version {
                from: 17,
                prefix: "2.3".into(),
                package: "requests".into()
            })
        );
        let (t, o) = at("Django >= 4.2, < 5|");
        assert_eq!(
            requirements_context(&t, o),
            Some(Context::Version {
                from: 17,
                prefix: "5".into(),
                package: "Django".into()
            })
        );
        let (t, o) = at("-r base|");
        assert_eq!(requirements_context(&t, o), None);
        let (t, o) = at("numpy; python_version|");
        assert_eq!(requirements_context(&t, o), None);
        let (t, o) = at("numpy |");
        assert_eq!(requirements_context(&t, o), None);
    }

    #[test]
    fn pyproject_contexts() {
        let (t, o) = at("[project]\nname = \"x\"\ndependencies = [\n    \"fastapi>=0.1|\",\n]\n");
        assert_eq!(
            pyproject_context(&t, o),
            Some(Context::Version {
                from: 52,
                prefix: "0.1".into(),
                package: "fastapi".into()
            })
        );
        let (t, o) = at("[project]\nname = \"sh|\"\n");
        assert_eq!(pyproject_context(&t, o), None);
        let (t, o) = at("[project.optional-dependencies]\ntest = [\"pyt|\"]\n");
        assert_eq!(
            pyproject_context(&t, o),
            Some(Context::Name {
                from: 41,
                prefix: "pyt".into()
            })
        );
        let (t, o) = at("[tool.poetry.dependencies]\nreq|");
        assert_eq!(
            pyproject_context(&t, o),
            Some(Context::Name {
                from: 27,
                prefix: "req".into()
            })
        );
    }

    #[test]
    fn sorts_versions() {
        let v: Vec<String> = ["1.0", "2.0rc1", "1.10", "1.2", "junk"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(sort_versions(&v), vec!["1.10", "1.2", "1.0", "2.0rc1"]);
    }
}
