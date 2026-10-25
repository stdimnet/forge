//! PHPForge CMS basics: Drupal, Joomla and Shopware in one plugin (`forge.cms`).
//!
//! - [`detect`]: which CMS a project is (a site or a single extension), its version and
//!   console (`drush`, `bin/console`).
//! - [`drupal`]: hook implementation completion (`<module>_<hook>`), implementations as
//!   gutter marks and Go to the hook docs; [`drupal_hooks`] is the bundled hook list.
//! - [`api`]: the queries the IDE sends (UTF-16 offsets), answered on a [`Snapshot`].
//!
//! File types and template languages (Drupal's `.module` / `.install` as PHP, Twig with
//! Shopware's `sw_extends`, Smarty for Shopware 5, Joomla XML manifests) come from the
//! plugin manifest and the editor. Pure Rust, shared with the browser mode via WebAssembly.

pub mod api;
pub mod detect;
pub mod drupal;
pub mod drupal_hooks;

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub use detect::CmsInfo;
pub use drupal::Extension;

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

/// What the IDE knows about the project's CMS.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub cms: Vec<CmsInfo>,
    /// Drupal modules, themes and profiles (folders with a `<name>.info.yml`).
    pub extensions: Vec<Extension>,
    /// `hook_…` functions of the project's `*.api.php` files.
    pub api_hooks: Vec<(String, Location)>,
}

impl Snapshot {
    pub fn uses(&self, id: &str) -> bool {
        self.cms.iter().any(|c| c.id == id)
    }
}

/// Folders never scanned for Drupal extensions and API files.
const SKIP_DIRS: &[&str] = &["vendor", "node_modules", "files", ".git", "tests"];
const MAX_FILES: usize = 50_000;

fn info_kind(src: &str) -> String {
    src.lines()
        .find_map(|l| l.strip_prefix("type:"))
        .map(|t| t.trim().trim_matches(['\'', '"']).to_string())
        .unwrap_or_else(|| "module".into())
}

fn add_file(snap: &mut Snapshot, rel: &str, read: &dyn Fn() -> Option<String>) {
    let (dir, file) = rel.rsplit_once('/').unwrap_or(("", rel));
    if let Some(name) = file.strip_suffix(".info.yml") {
        let kind = read()
            .map(|s| info_kind(&s))
            .unwrap_or_else(|| "module".into());
        snap.extensions.push(Extension {
            name: name.to_string(),
            dir: dir.to_string(),
            kind,
        });
    } else if file.ends_with(".api.php") {
        if let Some(src) = read() {
            snap.api_hooks.extend(drupal::api_hooks(rel, &src));
        }
    }
}

/// Scan the project at `root`.
pub fn scan(root: &Path) -> Snapshot {
    let cms = detect::detect(&detect::DirProbe(root), &find_on_path);
    let mut snap = Snapshot {
        cms,
        ..Snapshot::default()
    };
    if !snap.uses("drupal") {
        return snap;
    }
    let mut stack = vec![root.to_path_buf()];
    let mut seen = 0;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(kind) = e.file_type() else { continue };
            if kind.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                    stack.push(e.path());
                }
                continue;
            }
            seen += 1;
            if seen > MAX_FILES {
                break;
            }
            if name.ends_with(".info.yml") || name.ends_with(".api.php") {
                let rel = e
                    .path()
                    .strip_prefix(root)
                    .unwrap_or(&e.path())
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                let path = e.path();
                add_file(&mut snap, &rel, &|| std::fs::read_to_string(&path).ok());
            }
        }
    }
    snap.extensions.sort_by(|a, b| a.dir.cmp(&b.dir));
    snap
}

/// The same from in-memory files (browser mode).
pub fn scan_files(files: &HashMap<String, String>) -> Snapshot {
    let mut snap = Snapshot {
        cms: detect::detect(&detect::MapProbe(files), &|_| None),
        ..Snapshot::default()
    };
    if snap.uses("drupal") {
        let mut paths: Vec<&String> = files.keys().collect();
        paths.sort();
        for p in paths {
            add_file(&mut snap, p, &|| files.get(p).cloned());
        }
    }
    snap
}

/// Look `program` up on PATH.
pub fn find_on_path(program: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    let exts: &[&str] = if cfg!(windows) {
        &[".exe", ".bat", ".cmd", ""]
    } else {
        &[""]
    };
    std::env::split_paths(&path).find_map(|dir| {
        exts.iter()
            .map(|ext| dir.join(format!("{program}{ext}")))
            .find(|c| c.is_file())
            .map(|c| c.to_string_lossy().into_owned())
    })
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
    fn scans_drupal_extensions_and_api_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("web/core/lib/Drupal/Core/Form")).unwrap();
        std::fs::create_dir_all(root.join("web/modules/custom/acme")).unwrap();
        std::fs::create_dir_all(root.join("web/themes/custom/anvil")).unwrap();
        std::fs::write(
            root.join("web/core/lib/Drupal.php"),
            "<?php const VERSION = '11.0.1';",
        )
        .unwrap();
        std::fs::write(
            root.join("web/core/lib/Drupal/Core/Form/form.api.php"),
            "<?php\nfunction hook_form_alter(&$form) {}\n",
        )
        .unwrap();
        std::fs::write(
            root.join("web/modules/custom/acme/acme.info.yml"),
            "name: Acme\ntype: module\n",
        )
        .unwrap();
        std::fs::write(
            root.join("web/themes/custom/anvil/anvil.info.yml"),
            "name: Anvil\ntype: theme\n",
        )
        .unwrap();
        let snap = scan(root);
        assert_eq!(snap.cms[0].version.as_deref(), Some("11.0.1"));
        let ext: Vec<(&str, &str)> = snap
            .extensions
            .iter()
            .map(|e| (e.name.as_str(), e.kind.as_str()))
            .collect();
        assert_eq!(ext, vec![("acme", "module"), ("anvil", "theme")]);
        assert_eq!(snap.api_hooks[0].0, "hook_form_alter");
        assert!(scan_files(&HashMap::new()).cms.is_empty());
    }
}
