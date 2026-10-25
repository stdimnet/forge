//! PHPForge WordPress plugin: what the IDE knows about a WordPress site, plugin or theme.
//!
//! - [`lexer`]: a small error tolerant PHP tokenizer.
//! - [`hooks`]: `add_action` / `add_filter` / `do_action` / `apply_filters` (and friends)
//!   in a file, dynamic hook names as patterns.
//! - [`index`]: the project's hooks and function declarations, plus the WordPress stubs.
//! - [`editor`]: hook name completion, Go to Declaration, gutter marks and Find Usages.
//! - [`yoda`]: the "Use Yoda condition" hint of the WordPress Coding Standards.
//! - [`detect`]: site / Composer install / plugin / theme detection, WP-CLI and stubs.
//! - [`api`]: the queries the IDE sends (UTF-16 offsets), answered on a [`Snapshot`].
//!
//! Pure Rust without system dependencies, so the browser mode runs the same code through
//! WebAssembly.

pub mod api;
pub mod core;
pub mod detect;
pub mod editor;
pub mod hooks;
pub mod index;
pub mod lexer;
pub mod yoda;

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub use detect::{WordPressInfo, WpCli};
pub use hooks::{HookKind, HookRef, HookRole};
pub use index::{HookIndex, HookSummary};

/// A position in a project file: `/`-separated relative path, 0-based line and byte column.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub path: String,
    pub line: u32,
    #[serde(default)]
    pub column: u32,
}

impl Location {
    pub fn new(path: impl Into<String>, line: u32, column: u32) -> Self {
        Self {
            path: path.into(),
            line,
            column,
        }
    }
}

/// Everything known about a WordPress project at one point in time.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// `None` when the project does not use WordPress.
    pub info: Option<WordPressInfo>,
    pub index: HookIndex,
}

impl Snapshot {
    fn finish(mut info: Option<WordPressInfo>, index: HookIndex) -> Self {
        if let Some(i) = info.as_mut() {
            i.hooks = index.refs.len();
            i.files = index.files;
        }
        Self { info, index }
    }
}

/// Scan the project at `root` (nothing is indexed unless it uses WordPress).
pub fn scan(root: &Path) -> Snapshot {
    let info = detect::detect(&detect::DirProbe(root), &detect::find_on_path);
    if info.is_none() {
        return Snapshot::default();
    }
    let mut index = HookIndex::scan(root);
    if let Some(stubs) = info.as_ref().and_then(|i| i.stubs.clone()) {
        if let Ok(src) = std::fs::read_to_string(root.join(&stubs)) {
            index.add_stubs(&stubs, &src);
        }
    }
    Snapshot::finish(info, index)
}

/// The same from in-memory files (browser mode); WP-CLI is never on a PATH there.
pub fn scan_files(files: &HashMap<String, String>) -> Snapshot {
    let info = detect::detect(&detect::MapProbe(files), &|_| None);
    if info.is_none() {
        return Snapshot::default();
    }
    let mut paths: Vec<&String> = files.keys().collect();
    paths.sort();
    let mut index = HookIndex::from_files(
        paths
            .into_iter()
            .filter(|p| !p.starts_with("vendor/"))
            .map(|p| (p.as_str(), files[p].as_str())),
    );
    if let Some(src) = info
        .as_ref()
        .and_then(|i| i.stubs.as_ref())
        .and_then(|s| files.get(s))
    {
        index.add_stubs(detect::STUBS_FILE, src);
    }
    Snapshot::finish(info, index)
}

/// Byte offset of UTF-16 offset `units` in `s` (clamped).
pub fn utf16_to_byte(s: &str, units: usize) -> usize {
    let mut n = 0;
    for (i, c) in s.char_indices() {
        if n >= units {
            return i;
        }
        n += c.len_utf16();
    }
    s.len()
}

/// UTF-16 offset of byte offset `byte` in `s`.
pub fn byte_to_utf16(s: &str, byte: usize) -> usize {
    let mut b = byte.min(s.len());
    while !s.is_char_boundary(b) {
        b -= 1;
    }
    s[..b].encode_utf16().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_only_wordpress_projects() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("index.php"), "<?php do_action('x');").unwrap();
        assert!(scan(dir.path()).info.is_none());
        std::fs::write(
            dir.path().join("acme.php"),
            "<?php\n/* Plugin Name: Acme */\nadd_action('init', 'acme_init');",
        )
        .unwrap();
        let snap = scan(dir.path());
        let info = snap.info.unwrap();
        assert_eq!((info.hooks, info.files), (2, 2));
    }

    #[test]
    fn converts_offsets() {
        let s = "aé😀b";
        assert_eq!(utf16_to_byte(s, 2), 3);
        assert_eq!(utf16_to_byte(s, 4), 7);
        assert_eq!(byte_to_utf16(s, 7), 4);
        assert_eq!(byte_to_utf16(s, 100), 5);
    }
}
