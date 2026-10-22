//! File | Invalidate Caches…: what Forge caches, the dialog's options and the cleanup plan.
//!
//! Like PHPStorm, the indexes and caches are always cleared; the dialog's checkboxes add the
//! Local History, the downloaded shared indexes (PHP stubs), the embedded browser engine's
//! cache and the data of language servers. Forge's caches:
//!
//! | What | Where |
//! | --- | --- |
//! | PHP index | `<cache>/forge/php-index` |
//! | Search index | `<cache>/Forge/search` |
//! | Command line tools | `<cache>/forge/command-line-tools` |
//! | JSON schemas | `<cache>/forge/json-schemas` |
//! | Packagist | `<cache>/forge/packagist` |
//! | Shell integration scripts | `<cache>/forge/shell-integration` |
//! | Local History (option) | `<data>/forge/projects/*/local-history` |
//! | PHP stubs (option) | `<cache>/forge/php-stubs` |
//! | Browser engine (option) | the webview's cache directory |
//! | Language server data (option) | `<cache>/forge/intelephense`, `<cache>/forge/jdtls` |
//!
//! Installed language servers and runtimes (`<data>/forge/langservers`, `runtimes`) are not
//! caches and stay.

use serde::{Deserialize, Serialize};

/// An optional part of the invalidation (a checkbox of the dialog).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CacheOption {
    /// "Clear file system cache and Local History".
    FileSystemAndLocalHistory,
    /// "Clear downloaded shared indexes": the PHP stubs.
    SharedIndexes,
    /// "Delete embedded browser engine cache".
    BrowserEngine,
    /// "Delete language server data".
    LanguageServerData,
}

impl CacheOption {
    pub const ALL: [CacheOption; 4] = [
        CacheOption::FileSystemAndLocalHistory,
        CacheOption::SharedIndexes,
        CacheOption::BrowserEngine,
        CacheOption::LanguageServerData,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::FileSystemAndLocalHistory => "Clear file system cache and Local History",
            Self::SharedIndexes => "Clear downloaded shared indexes",
            Self::BrowserEngine => "Delete embedded browser engine cache",
            Self::LanguageServerData => "Delete language server data",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Self::FileSystemAndLocalHistory => "Local History of all projects",
            Self::SharedIndexes => "PHP stubs downloaded for the PHP index",
            Self::BrowserEngine => "Cached pages, scripts and images of the Forge window",
            Self::LanguageServerData => "Indexes and workspaces of Intelephense and Java",
        }
    }
}

/// One checkbox of the dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionInfo {
    pub id: CacheOption,
    pub label: &'static str,
    pub detail: &'static str,
}

/// Everything the Invalidate Caches dialog shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvalidateModel {
    pub title: &'static str,
    pub message: &'static str,
    /// Shown while the Local History option is selected.
    pub warning: &'static str,
    /// What is always cleared.
    pub always: Vec<&'static str>,
    pub options: Vec<OptionInfo>,
    pub invalidate_label: &'static str,
    pub just_restart_label: &'static str,
}

/// The caches that every invalidation clears: (label, path below the user cache directory).
pub const ALWAYS: [(&str, &str); 6] = [
    ("PHP index", "forge/php-index"),
    ("Search index", "Forge/search"),
    ("Command line tools", "forge/command-line-tools"),
    ("JSON schemas", "forge/json-schemas"),
    ("Packagist packages", "forge/packagist"),
    ("Shell integration scripts", "forge/shell-integration"),
];

/// The Invalidate Caches dialog.
pub fn invalidate_model() -> InvalidateModel {
    InvalidateModel {
        title: "Invalidate Caches",
        message: "The caches will be invalidated and rebuilt on the next startup.",
        warning: "WARNING: Local History will be also cleared.",
        always: ALWAYS.iter().map(|(label, _)| *label).collect(),
        options: CacheOption::ALL
            .iter()
            .map(|&id| OptionInfo {
                id,
                label: id.label(),
                detail: id.detail(),
            })
            .collect(),
        invalidate_label: "Invalidate and Restart",
        just_restart_label: "Just Restart",
    }
}

#[cfg(feature = "disk")]
pub use disk::*;

#[cfg(feature = "disk")]
mod disk {
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    use serde::Serialize;

    use super::{CacheOption, ALWAYS};
    use crate::dirs::ForgeDirs;

    /// One folder the invalidation deletes.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct CleanupItem {
        pub label: String,
        pub path: PathBuf,
    }

    /// The folders to delete for `options`. Only folders that exist are listed.
    pub fn cleanup_plan(dirs: &ForgeDirs, options: &[CacheOption]) -> Vec<CleanupItem> {
        let mut plan = Vec::new();
        let mut push = |label: &str, path: PathBuf| {
            if path.exists() && !plan.iter().any(|i: &CleanupItem| i.path == path) {
                plan.push(CleanupItem {
                    label: label.to_string(),
                    path,
                });
            }
        };
        if let Some(cache) = &dirs.cache {
            for (label, rel) in ALWAYS {
                push(label, cache.join(rel));
            }
        }
        for option in options {
            match option {
                CacheOption::FileSystemAndLocalHistory => {
                    let Some(projects) = dirs.forge_data().map(|d| d.join("projects")) else {
                        continue;
                    };
                    let Ok(read) = fs::read_dir(&projects) else {
                        continue;
                    };
                    let mut found: Vec<PathBuf> = read
                        .filter_map(Result::ok)
                        .map(|e| e.path().join("local-history"))
                        .collect();
                    found.sort();
                    for path in found {
                        push("Local History", path);
                    }
                }
                CacheOption::SharedIndexes => {
                    if let Some(cache) = dirs.forge_cache() {
                        push("PHP stubs", cache.join("php-stubs"));
                    }
                }
                CacheOption::BrowserEngine => {
                    if let Some(webview) = &dirs.webview_cache {
                        push("Embedded browser engine cache", webview.clone());
                    }
                }
                CacheOption::LanguageServerData => {
                    if let Some(cache) = dirs.forge_cache() {
                        push("Intelephense data", cache.join("intelephense"));
                        push("Java language server data", cache.join("jdtls"));
                    }
                }
            }
        }
        plan
    }

    /// What a cleanup did.
    #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct CleanupReport {
        pub deleted: Vec<CleanupItem>,
        /// Items that could not be deleted, with the reason.
        pub failed: Vec<(CleanupItem, String)>,
    }

    fn remove(path: &Path) -> io::Result<()> {
        if path.is_dir() {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        }
    }

    /// Delete everything of the plan; failures are reported, the rest goes on.
    pub fn run_cleanup(plan: Vec<CleanupItem>) -> CleanupReport {
        let mut report = CleanupReport::default();
        for item in plan {
            match remove(&item.path) {
                Ok(()) => report.deleted.push(item),
                Err(e) if e.kind() == io::ErrorKind::NotFound => report.deleted.push(item),
                Err(e) => report.failed.push((item, e.to_string())),
            }
        }
        report
    }
}

#[cfg(all(test, feature = "disk"))]
mod tests {
    use super::*;
    use crate::dirs::ForgeDirs;
    use std::fs;

    fn touch(path: &std::path::Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
    }

    #[test]
    fn model_lists_phpstorm_options() {
        let m = invalidate_model();
        assert_eq!(m.invalidate_label, "Invalidate and Restart");
        assert_eq!(
            m.options[0].label,
            "Clear file system cache and Local History"
        );
        assert_eq!(m.options.len(), 4);
        assert!(m.always.contains(&"PHP index"));
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["options"][1]["id"], "sharedIndexes");
    }

    #[test]
    fn plan_always_clears_indexes_and_adds_selected_options() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = ForgeDirs::under(tmp.path());
        let cache = tmp.path().join("cache");
        let data = tmp.path().join("data/forge");
        touch(&cache.join("forge/php-index/app-1.json"));
        touch(&cache.join("Forge/search/0001.bin"));
        touch(&cache.join("forge/packagist/laravel.json"));
        touch(&cache.join("forge/php-stubs/core.php"));
        touch(&cache.join("forge/intelephense/index"));
        touch(&data.join("projects/shop-1/local-history/log"));
        touch(&data.join("projects/shop-1/bookmarks.json"));
        touch(&data.join("projects/blog-2/local-history/log"));
        touch(&data.join("langservers/node_modules/x"));
        touch(&tmp.path().join("webview/WebKitCache/x"));

        let labels = |p: &[CleanupItem]| p.iter().map(|i| i.label.clone()).collect::<Vec<_>>();
        let plain = cleanup_plan(&dirs, &[]);
        assert_eq!(
            labels(&plain),
            ["PHP index", "Search index", "Packagist packages"]
        );

        let all = cleanup_plan(&dirs, &CacheOption::ALL);
        assert_eq!(
            labels(&all),
            [
                "PHP index",
                "Search index",
                "Packagist packages",
                "Local History",
                "Local History",
                "PHP stubs",
                "Embedded browser engine cache",
                "Intelephense data"
            ]
        );
        let report = run_cleanup(all);
        assert_eq!(report.deleted.len(), 8);
        assert!(report.failed.is_empty());
        assert!(!cache.join("forge/php-index").exists());
        assert!(!data.join("projects/shop-1/local-history").exists());
        // Other project data and installed servers stay.
        assert!(data.join("projects/shop-1/bookmarks.json").exists());
        assert!(data.join("langservers/node_modules/x").exists());
        assert!(cleanup_plan(&dirs, &CacheOption::ALL).is_empty());
    }
}
