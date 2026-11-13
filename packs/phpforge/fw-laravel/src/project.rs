//! Project layout: Laravel version, PSR-4 autoloading, file helpers and locations.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// A place in a project file. `line` is 0-based, `character` counts UTF-16 units like the
/// editor does.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub path: String,
    pub line: u32,
    pub character: u32,
}

impl Location {
    /// Location of a byte offset in `src` (the content of `path`).
    pub fn at(path: &str, src: &str, byte: usize) -> Self {
        let byte = byte.min(src.len());
        let line_start = src[..byte].rfind('\n').map_or(0, |i| i + 1);
        let mut b = byte;
        while !src.is_char_boundary(b) {
            b -= 1;
        }
        Self {
            path: path.to_string(),
            line: crate::php::line_of(src, byte),
            character: src[line_start..b]
                .chars()
                .map(char::len_utf16)
                .sum::<usize>() as u32,
        }
    }

    pub fn file(path: &str) -> Self {
        Self {
            path: path.to_string(),
            line: 0,
            character: 0,
        }
    }
}

/// Project-relative path with forward slashes.
pub fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn read(root: &Path, rel: &str) -> Option<String> {
    std::fs::read_to_string(root.join(rel)).ok()
}

/// Files below `dir` (project relative) whose name ends with `suffix`, sorted.
pub fn files_under(root: &Path, dir: &str, suffix: &str) -> Vec<String> {
    let base = root.join(dir);
    if !base.is_dir() {
        return Vec::new();
    }
    let mut out: Vec<String> = walkdir::WalkDir::new(&base)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && e.file_name().to_string_lossy().ends_with(suffix))
        .map(|e| rel_path(root, e.path()))
        .collect();
    out.sort();
    out
}

/// Laravel version from `composer.lock` (`12.4.1`), else the `composer.json` constraint
/// (`^12.0` → `12.0`).
pub fn laravel_version(root: &Path) -> Option<String> {
    if let Some(lock) =
        read(root, "composer.lock").and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    {
        for key in ["packages", "packages-dev"] {
            let found = lock[key]
                .as_array()
                .into_iter()
                .flatten()
                .find(|p| p["name"] == "laravel/framework");
            if let Some(v) = found.and_then(|p| p["version"].as_str()) {
                return Some(v.trim_start_matches('v').to_string());
            }
        }
    }
    let json: serde_json::Value = serde_json::from_str(&read(root, "composer.json")?).ok()?;
    let constraint = json["require"]["laravel/framework"].as_str()?;
    let version: String = constraint
        .split('|')
        .next()?
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    (!version.is_empty()).then_some(version)
}

/// Whether `root` looks like a Laravel application.
pub fn is_laravel(root: &Path) -> bool {
    if root.join("artisan").is_file() && root.join("bootstrap").is_dir() {
        return true;
    }
    read(root, "composer.json").is_some_and(|s| s.contains("\"laravel/framework\""))
}

/// PSR-4 autoload prefixes from `composer.json` (`App\` → `app/`).
#[derive(Debug, Clone, Default)]
pub struct Psr4 {
    prefixes: Vec<(String, String)>,
}

impl Psr4 {
    pub fn load(root: &Path) -> Self {
        let mut prefixes = Vec::new();
        if let Some(json) = read(root, "composer.json")
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        {
            for section in ["autoload", "autoload-dev"] {
                if let Some(map) = json[section]["psr-4"].as_object() {
                    for (ns, dirs) in map {
                        let dirs: Vec<String> = match dirs {
                            serde_json::Value::String(s) => vec![s.clone()],
                            serde_json::Value::Array(a) => a
                                .iter()
                                .filter_map(|d| d.as_str().map(String::from))
                                .collect(),
                            _ => continue,
                        };
                        for d in dirs {
                            prefixes.push((ns.trim_end_matches('\\').to_string(), d));
                        }
                    }
                }
            }
        }
        if prefixes.is_empty() {
            prefixes.push(("App".into(), "app/".into()));
        }
        // Longest namespace first.
        prefixes.sort_by_key(|(ns, _)| std::cmp::Reverse(ns.len()));
        Self { prefixes }
    }

    /// Project-relative file of a class, if it exists.
    pub fn class_path(&self, root: &Path, fqn: &str) -> Option<String> {
        let fqn = fqn.trim_start_matches('\\');
        for (ns, dir) in &self.prefixes {
            let rest = if ns.is_empty() {
                Some(fqn)
            } else {
                fqn.strip_prefix(ns.as_str())
                    .and_then(|r| r.strip_prefix('\\'))
            };
            if let Some(rest) = rest {
                let dir = dir.trim_end_matches('/');
                let rel = if dir.is_empty() {
                    format!("{}.php", rest.replace('\\', "/"))
                } else {
                    format!("{dir}/{}.php", rest.replace('\\', "/"))
                };
                if root.join(&rel).is_file() {
                    return Some(rel);
                }
            }
        }
        None
    }

    /// Where the class's file goes (it need not exist yet).
    pub fn path_for(&self, fqn: &str) -> Option<String> {
        let fqn = fqn.trim_start_matches('\\');
        self.prefixes.iter().find_map(|(ns, dir)| {
            let rest = if ns.is_empty() {
                fqn
            } else {
                fqn.strip_prefix(ns.as_str())?.strip_prefix('\\')?
            };
            let dir = dir.trim_end_matches('/');
            Some(if dir.is_empty() {
                format!("{}.php", rest.replace('\\', "/"))
            } else {
                format!("{dir}/{}.php", rest.replace('\\', "/"))
            })
        })
    }

    /// Class name of a project file (`app/Models/User.php` → `App\Models\User`).
    pub fn class_of(&self, rel: &str) -> Option<String> {
        let stem = rel.strip_suffix(".php")?;
        self.prefixes
            .iter()
            .filter_map(|(ns, dir)| {
                let dir = dir.trim_end_matches('/');
                let rest = if dir.is_empty() {
                    stem
                } else {
                    stem.strip_prefix(dir)?.strip_prefix('/')?
                };
                let rest = rest.replace('/', "\\");
                Some(if ns.is_empty() {
                    rest
                } else {
                    format!("{ns}\\{rest}")
                })
            })
            .next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_counts_utf16() {
        let src = "a\nü😀x";
        let loc = Location::at("f", src, src.find('x').unwrap());
        assert_eq!((loc.line, loc.character), (1, 3));
    }
}
