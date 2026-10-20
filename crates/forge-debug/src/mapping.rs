//! Server ↔ local path mappings (PHPStorm's "Servers > Use path mappings").
//!
//! Local paths are project relative with forward slashes (`app/Http/Kernel.php`), the
//! form the UI and the breakpoint store use. Server paths are absolute paths on the
//! machine running PHP (`/var/www/html/app/Http/Kernel.php`); DBGp transfers them as
//! `file://` URIs.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One mapping row: a project directory or file and its absolute path on the server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathMapping {
    /// Project relative path; empty for the project root.
    pub local: String,
    /// Absolute path on the server.
    pub remote: String,
}

fn trim_slashes(s: &str) -> String {
    let s = s.replace('\\', "/");
    let t = s.trim_end_matches('/');
    if t.is_empty() && s.starts_with('/') {
        "/".into()
    } else {
        t.to_string()
    }
}

fn is_hex(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() && is_hex(bytes[i + 1]) && is_hex(bytes[i + 2]) {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("00");
            out.push(u8::from_str_radix(hex, 16).unwrap_or(b'?'));
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `file:///var/www/a%20b.php` → `/var/www/a b.php`; `file:///C:/x.php` → `C:/x.php`.
/// Returns `None` for non-file URIs such as `dbgp://1` (eval'd code).
pub fn uri_to_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    let decoded = percent_decode(rest);
    // Windows drive: "/C:/..." → "C:/..."
    let b = decoded.as_bytes();
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
        return Some(decoded[1..].to_string());
    }
    Some(decoded)
}

/// Absolute path → `file://` URI.
pub fn path_to_uri(path: &str) -> String {
    let p = path.replace('\\', "/");
    let p = if p.starts_with('/') {
        p
    } else {
        format!("/{p}")
    };
    format!("file://{}", percent_encode(&p))
}

/// Translates between engine URIs and project relative paths.
#[derive(Debug, Clone, Default)]
pub struct PathMapper {
    root: PathBuf,
    root_str: String,
    mappings: Vec<PathMapping>,
}

impl PathMapper {
    /// Mapper without explicit mappings: server and local paths are the same.
    pub fn local(root: impl Into<PathBuf>) -> Self {
        Self::new(root, Vec::new())
    }

    pub fn new(root: impl Into<PathBuf>, mappings: Vec<PathMapping>) -> Self {
        let root = root.into();
        let root_str = trim_slashes(&root.to_string_lossy());
        let mut mappings: Vec<PathMapping> = mappings
            .into_iter()
            .filter(|m| !m.remote.trim().is_empty())
            .map(|m| PathMapping {
                local: relativize(&root_str, &trim_slashes(m.local.trim())),
                remote: trim_slashes(m.remote.trim()),
            })
            .collect();
        // Most specific first.
        mappings.sort_by_key(|m| std::cmp::Reverse(m.remote.len()));
        Self {
            root,
            root_str,
            mappings,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn mappings(&self) -> &[PathMapping] {
        &self.mappings
    }

    /// Project relative path of an engine URI, or `None` when the file is not mapped into
    /// the project.
    pub fn to_local(&self, uri: &str) -> Option<String> {
        let path = trim_slashes(&uri_to_path(uri)?);
        for m in &self.mappings {
            if let Some(rest) = strip_dir(&path, &m.remote) {
                return Some(join(&m.local, rest));
            }
        }
        strip_dir(&path, &self.root_str).map(|rest| rest.to_string())
    }

    /// Engine URI of a project relative path.
    pub fn to_remote(&self, local: &str) -> String {
        let local = trim_slashes(local);
        let mut by_local: Vec<&PathMapping> = self.mappings.iter().collect();
        by_local.sort_by_key(|m| std::cmp::Reverse(m.local.len()));
        for m in by_local {
            let rest = if m.local.is_empty() {
                Some(local.as_str())
            } else {
                strip_dir(&local, &m.local)
            };
            if let Some(rest) = rest {
                return path_to_uri(&join(&m.remote, rest));
            }
        }
        path_to_uri(&join(&self.root_str, &local))
    }
}

/// `dir/rest` → `rest` (also `dir` → ``).
fn strip_dir<'a>(path: &'a str, dir: &str) -> Option<&'a str> {
    if dir == "/" {
        return path.strip_prefix('/');
    }
    if path == dir {
        return Some("");
    }
    path.strip_prefix(dir)?.strip_prefix('/')
}

fn join(base: &str, rest: &str) -> String {
    match (base.is_empty(), rest.is_empty()) {
        (_, true) => base.to_string(),
        (true, false) => rest.to_string(),
        (false, false) if base.ends_with('/') => format!("{base}{rest}"),
        _ => format!("{base}/{rest}"),
    }
}

/// Absolute local paths inside the root become relative; relative ones stay.
fn relativize(root: &str, local: &str) -> String {
    match strip_dir(local, root) {
        Some(rest) => rest.to_string(),
        None => local.trim_start_matches("./").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_round_trip() {
        assert_eq!(
            uri_to_path("file:///var/www/my%20app/a.php").as_deref(),
            Some("/var/www/my app/a.php")
        );
        assert_eq!(
            uri_to_path("file:///C:/www/a.php").as_deref(),
            Some("C:/www/a.php")
        );
        assert_eq!(uri_to_path("dbgp://3"), None);
        assert_eq!(
            path_to_uri("/var/www/my app/ä.php"),
            "file:///var/www/my%20app/%C3%A4.php"
        );
        assert_eq!(path_to_uri("C:\\www\\a.php"), "file:///C:/www/a.php");
    }

    #[test]
    fn local_mapper_uses_project_root() {
        let m = PathMapper::local("/home/me/shop");
        assert_eq!(
            m.to_local("file:///home/me/shop/app/Models/User.php")
                .as_deref(),
            Some("app/Models/User.php")
        );
        assert_eq!(m.to_local("file:///usr/share/php/x.php"), None);
        assert_eq!(m.to_local("file:///home/me/shopping/x.php"), None);
        assert_eq!(
            m.to_remote("public/index.php"),
            "file:///home/me/shop/public/index.php"
        );
    }

    #[test]
    fn explicit_mappings_win_and_most_specific_first() {
        let m = PathMapper::new(
            "/home/me/shop",
            vec![
                PathMapping {
                    local: "".into(),
                    remote: "/var/www/html/".into(),
                },
                PathMapping {
                    local: "/home/me/shop/vendor".into(),
                    remote: "/opt/vendor".into(),
                },
            ],
        );
        assert_eq!(
            m.to_local("file:///var/www/html/app/Http/Controllers/OrderController.php")
                .as_deref(),
            Some("app/Http/Controllers/OrderController.php")
        );
        assert_eq!(
            m.to_local("file:///opt/vendor/laravel/framework/src/x.php")
                .as_deref(),
            Some("vendor/laravel/framework/src/x.php")
        );
        assert_eq!(m.to_local("file:///etc/php.ini"), None);
        assert_eq!(
            m.to_remote("vendor/autoload.php"),
            "file:///opt/vendor/autoload.php"
        );
        assert_eq!(
            m.to_remote("app/Models/User.php"),
            "file:///var/www/html/app/Models/User.php"
        );
    }
}
