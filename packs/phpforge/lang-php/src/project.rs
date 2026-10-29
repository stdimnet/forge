//! Project settings the PHP index needs where the project lives: the include paths
//! (Settings | PHP | Include Path, "External Libraries") and the source roots (composer.json
//! PSR-4 rules with the user's Mark Directory as changes). The app reads them from its
//! settings and hands them to the backend before indexing.

use std::path::{Path, PathBuf};

use forge_file_templates::psr4::{self, Psr4Root, SourceRootSetting};
use serde::{Deserialize, Serialize};

/// Setting keys (see `app/src-tauri/plugins/php.toml`).
pub const INCLUDE_PATHS_KEY: &str = "php.includePaths";
pub const SOURCE_ROOTS_KEY: &str = "php.sourceRoots";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PhpProjectSettings {
    /// Folders indexed as libraries (absolute, `~/…`, or relative to the project).
    pub include_paths: Vec<String>,
    /// The `php.sourceRoots` setting.
    pub source_roots: Vec<SourceRootSetting>,
    /// composer.json, relative to the project root (`php.composer.json`).
    pub composer_json: String,
}

impl Default for PhpProjectSettings {
    fn default() -> Self {
        Self {
            include_paths: Vec::new(),
            source_roots: Vec::new(),
            composer_json: "composer.json".into(),
        }
    }
}

impl PhpProjectSettings {
    /// From settings values by key.
    pub fn from_settings(get: impl Fn(&str) -> Option<serde_json::Value>) -> Self {
        let mut s = Self::default();
        if let Some(v) = get(INCLUDE_PATHS_KEY) {
            s.include_paths = serde_json::from_value::<Vec<String>>(v)
                .unwrap_or_default()
                .into_iter()
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();
        }
        if let Some(v) = get(SOURCE_ROOTS_KEY) {
            s.source_roots = serde_json::from_value(v).unwrap_or_default();
        }
        if let Some(p) = get(crate::composer::JSON_KEY)
            .and_then(|v| v.as_str().map(str::to_string))
            .filter(|p| !p.trim().is_empty())
        {
            s.composer_json = p;
        }
        s
    }

    /// The include paths as absolute folders on the project's machine.
    pub fn include_dirs(&self, root: &Path, home: Option<&Path>) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        for p in &self.include_paths {
            let path = match (p.strip_prefix("~/"), home) {
                (Some(rest), Some(h)) => h.join(rest),
                _ => {
                    let p = PathBuf::from(p);
                    if p.is_absolute() {
                        p
                    } else {
                        root.join(p)
                    }
                }
            };
            if !out.contains(&path) {
                out.push(path);
            }
        }
        out
    }

    /// The composer.json rules with the user's changes, reading composer.json from `root`.
    pub fn effective_roots(&self, root: &Path) -> Vec<Psr4Root> {
        let composer = std::fs::read_to_string(root.join(&self.composer_json)).ok();
        let base = composer
            .as_deref()
            .map(|t| psr4::roots(t, &psr4::composer_dir(&self.composer_json)))
            .unwrap_or_default();
        psr4::apply_overrides(&base, &self.source_roots)
    }
}

/// A folder of "External Libraries" (an include path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryRoot {
    /// The folder's name.
    pub name: String,
    /// Absolute path on the project's machine.
    pub path: String,
    pub exists: bool,
}

/// An entry of a library folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntry {
    pub name: String,
    /// Absolute path.
    pub path: String,
    pub dir: bool,
}

fn slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

/// The include paths as library roots.
pub fn library_roots(dirs: &[PathBuf]) -> Vec<LibraryRoot> {
    dirs.iter()
        .map(|d| LibraryRoot {
            name: d
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| slash(d)),
            path: slash(d),
            exists: d.is_dir(),
        })
        .collect()
}

/// The include path `path` is inside (after resolving `..` and links), if any.
pub fn inside_library(dirs: &[PathBuf], path: &Path) -> bool {
    let Ok(real) = path.canonicalize() else {
        return false;
    };
    dirs.iter()
        .filter_map(|d| d.canonicalize().ok())
        .any(|d| real.starts_with(d))
}

/// The entries of a folder inside an include path: folders first, then files, by name.
/// `None` when the folder is not inside one.
pub fn list_library(dirs: &[PathBuf], path: &Path) -> Option<Vec<LibraryEntry>> {
    if !inside_library(dirs, path) {
        return None;
    }
    let mut out: Vec<LibraryEntry> = std::fs::read_dir(path)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| {
            let p = e.path();
            LibraryEntry {
                name: e.file_name().to_string_lossy().into_owned(),
                dir: p.is_dir(),
                path: slash(&p),
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Some(out)
}

/// A file inside an include path; `None` outside them.
pub fn read_library(dirs: &[PathBuf], path: &Path) -> Option<String> {
    if !inside_library(dirs, path) || !path.is_file() {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_settings_and_resolves_include_dirs() {
        let s = PhpProjectSettings::from_settings(|k| match k {
            "php.includePaths" => Some(json!(["/usr/share/php", " ", "~/lib", "stubs"])),
            "php.sourceRoots" => {
                Some(json!([{ "path": "src", "prefix": "Acme", "kind": "source" }]))
            }
            "php.composer.json" => Some(json!("backend/composer.json")),
            _ => None,
        });
        assert_eq!(s.include_paths, vec!["/usr/share/php", "~/lib", "stubs"]);
        assert_eq!(s.source_roots[0].prefix, "Acme");
        assert_eq!(s.composer_json, "backend/composer.json");
        let dirs = s.include_dirs(Path::new("/p"), Some(Path::new("/home/u")));
        assert_eq!(
            dirs,
            vec![
                PathBuf::from("/usr/share/php"),
                PathBuf::from("/home/u/lib"),
                PathBuf::from("/p/stubs")
            ]
        );
        assert_eq!(
            PhpProjectSettings::from_settings(|_| None),
            PhpProjectSettings::default()
        );
    }

    #[test]
    fn libraries_are_listed_and_read_only_inside_include_paths() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("lib");
        std::fs::create_dir_all(lib.join("Legacy")).unwrap();
        std::fs::write(lib.join("Legacy/Helper.php"), "<?php\n").unwrap();
        std::fs::write(lib.join("a.php"), "<?php\n").unwrap();
        std::fs::write(dir.path().join("secret.txt"), "no").unwrap();
        let dirs = vec![lib.clone()];
        let roots = library_roots(&dirs);
        assert_eq!(roots[0].name, "lib");
        assert!(roots[0].exists);
        let entries = list_library(&dirs, &lib).unwrap();
        let names: Vec<(&str, bool)> = entries.iter().map(|e| (e.name.as_str(), e.dir)).collect();
        assert_eq!(names, vec![("Legacy", true), ("a.php", false)]);
        assert_eq!(
            read_library(&dirs, &lib.join("Legacy/Helper.php")).as_deref(),
            Some("<?php\n")
        );
        assert_eq!(read_library(&dirs, &dir.path().join("secret.txt")), None);
        assert_eq!(read_library(&dirs, &lib.join("../secret.txt")), None);
        assert!(list_library(&dirs, dir.path()).is_none());
    }

    #[test]
    fn effective_roots_apply_the_users_changes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("composer.json"),
            r#"{"autoload":{"psr-4":{"App\\":"app/"}}}"#,
        )
        .unwrap();
        let s = PhpProjectSettings {
            source_roots: vec![SourceRootSetting {
                path: "modules".into(),
                prefix: "Modules".into(),
                kind: "source".into(),
            }],
            ..Default::default()
        };
        let roots = s.effective_roots(dir.path());
        assert_eq!(roots.len(), 2);
        assert_eq!(
            psr4::namespace_for_dir(&roots, "modules/Billing").as_deref(),
            Some("Modules\\Billing")
        );
        assert_eq!(
            psr4::namespace_for_dir(&roots, "app/Models").as_deref(),
            Some("App\\Models")
        );
    }
}
