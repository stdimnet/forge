//! Project structure, like PHPStorm's Settings | Directories and the Project view's
//! "Mark Directory as": excluded folders, resource roots and the "Exclude files" patterns.
//!
//! - [`StructureConfig`] is what the project settings store (`projectStructure.folders`,
//!   `projectStructure.excludePatterns`).
//! - [`ExcludeSet`] is it compiled: the one helper every walker, index, watcher and search
//!   asks whether a path is excluded (Find in Files, Search Everywhere, the PHP and search
//!   indexes, inspections, local history, the Project view's orange folders).
//! - [`mark`] applies a "Mark Directory as" action to the stored folders, [`state`] tells
//!   the menu and the Settings page how a folder is marked and why.
//!
//! Like PHPStorm, `vendor/` is not excluded (it is a library, see
//! [`crate::is_library_path`]), `node_modules` and a few tool folders are excluded by
//! default (their exclusion can be cancelled), and VCS metadata (`.git`) is never shown.

use serde::{Deserialize, Serialize};

/// Names hidden everywhere: never listed, walked or watched.
pub const HIDDEN_NAMES: &[&str] = &[".git", ".hg", ".svn", ".DS_Store"];

/// Folder names excluded by default anywhere in the tree (like PHPStorm's automatic
/// exclusion of `node_modules`); "Cancel Exclusion" keeps one of them in the project.
pub const DEFAULT_EXCLUDED: &[&str] = &[
    "node_modules",
    ".idea",
    ".forge",
    "target",
    ".venv",
    "venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".tox",
    ".next",
];

/// Settings keys (project scope).
pub const FOLDERS_KEY: &str = "projectStructure.folders";
pub const PATTERNS_KEY: &str = "projectStructure.excludePatterns";

/// How a folder is marked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FolderKind {
    Excluded,
    Resource,
    TestResource,
    /// "Cancel Exclusion" of a folder excluded by default or by a pattern.
    Included,
}

/// One entry of `projectStructure.folders`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderMark {
    /// Workspace-relative folder path with `/` separators.
    pub path: String,
    pub kind: FolderKind,
}

/// The stored project structure.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StructureConfig {
    pub folders: Vec<FolderMark>,
    /// PHPStorm's "Exclude files" field: name patterns (`*`, `?`) separated by `;`.
    pub exclude_patterns: String,
}

impl StructureConfig {
    /// Read the configuration from settings values (`get(key)` returns the JSON value).
    pub fn from_settings(get: impl Fn(&str) -> Option<serde_json::Value>) -> Self {
        let folders = get(FOLDERS_KEY)
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| serde_json::from_value::<FolderMark>(v).ok())
            .map(|m| FolderMark {
                path: clean(&m.path),
                kind: m.kind,
            })
            .filter(|m| !m.path.is_empty())
            .collect();
        let exclude_patterns = get(PATTERNS_KEY)
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default();
        Self {
            folders,
            exclude_patterns,
        }
    }

    /// The folders of one kind.
    pub fn paths(&self, kind: FolderKind) -> Vec<String> {
        self.folders
            .iter()
            .filter(|m| m.kind == kind)
            .map(|m| m.path.clone())
            .collect()
    }
}

/// Trim `./`, leading and trailing slashes and backslashes.
pub fn clean(path: &str) -> String {
    let p = path.trim().replace('\\', "/");
    let p = p.strip_prefix("./").unwrap_or(&p);
    p.trim_matches('/').to_string()
}

/// Why a path is excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExcludeReason {
    /// Marked as Excluded by the user.
    User,
    /// A folder name excluded by default (`node_modules`).
    Default,
    /// Matched by an "Exclude files" pattern.
    Pattern,
    /// Inside an excluded folder.
    Parent,
}

/// [`StructureConfig`] compiled for fast path checks. Equal sets exclude the same paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludeSet {
    excluded: Vec<String>,
    included: Vec<String>,
    patterns: Vec<String>,
}

impl Default for ExcludeSet {
    fn default() -> Self {
        Self::new(&StructureConfig::default())
    }
}

impl ExcludeSet {
    pub fn new(config: &StructureConfig) -> Self {
        let mut excluded = config.paths(FolderKind::Excluded);
        excluded.sort();
        excluded.dedup();
        let mut included = config.paths(FolderKind::Included);
        included.sort();
        included.dedup();
        let mut patterns: Vec<String> = config
            .exclude_patterns
            .split([';', '\n'])
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect();
        patterns.dedup();
        Self {
            excluded,
            included,
            patterns,
        }
    }

    /// Folders the user marked as excluded.
    pub fn user_folders(&self) -> &[String] {
        &self.excluded
    }

    /// Default-excluded folder names that were not cancelled anywhere (for consumers that
    /// only understand names, like local history).
    pub fn default_names(&self) -> Vec<String> {
        DEFAULT_EXCLUDED
            .iter()
            .filter(|n| {
                !self
                    .included
                    .iter()
                    .any(|i| i.rsplit('/').next() == Some(**n))
            })
            .map(|n| n.to_string())
            .collect()
    }

    /// Whether `name` is never shown (`.git`).
    pub fn is_hidden_name(name: &str) -> bool {
        HIDDEN_NAMES.contains(&name)
    }

    /// Why the entry at `rel` itself is excluded, not looking at its parents.
    pub fn own_reason(&self, rel: &str, is_dir: bool) -> Option<ExcludeReason> {
        let rel = rel.trim_matches('/');
        if rel.is_empty() {
            return None;
        }
        let name = rel.rsplit('/').next().unwrap_or(rel);
        if is_dir && self.excluded.iter().any(|e| e == rel) {
            return Some(ExcludeReason::User);
        }
        let cancelled = || is_dir && self.included.iter().any(|i| i == rel);
        if is_dir && DEFAULT_EXCLUDED.contains(&name) && !cancelled() {
            return Some(ExcludeReason::Default);
        }
        if self.patterns.iter().any(|p| glob_matches(p, name)) && !cancelled() {
            return Some(ExcludeReason::Pattern);
        }
        None
    }

    /// Why `rel` is excluded: itself or one of its folders.
    pub fn reason(&self, rel: &str, is_dir: bool) -> Option<ExcludeReason> {
        let rel = rel.trim_matches('/');
        let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
        for i in 1..parts.len() {
            let dir = parts[..i].join("/");
            if Self::is_hidden_name(parts[i - 1]) || self.own_reason(&dir, true).is_some() {
                return Some(ExcludeReason::Parent);
            }
        }
        self.own_reason(rel, is_dir)
    }

    /// Whether `rel` is excluded (itself or a folder above it) or hidden.
    pub fn excludes(&self, rel: &str, is_dir: bool) -> bool {
        let name = rel.trim_matches('/').rsplit('/').next().unwrap_or("");
        Self::is_hidden_name(name) || self.reason(rel, is_dir).is_some()
    }

    /// For walkers that descend from the root: skip this entry (and, for a folder, all
    /// below it). Its parents were already checked.
    pub fn skips(&self, rel: &str, name: &str, is_dir: bool) -> bool {
        Self::is_hidden_name(name) || self.own_reason(rel, is_dir).is_some()
    }
}

/// A glob over one name: `*` any run of characters, `?` one; case-sensitive like
/// PHPStorm on Linux.
pub fn glob_matches(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let s: Vec<char> = name.chars().collect();
    let (mut pi, mut si) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while si < s.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == s[si]) {
            pi += 1;
            si += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = si;
            pi += 1;
        } else if let Some(st) = star {
            pi = st + 1;
            mark += 1;
            si = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// A "Mark Directory as" action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MarkAction {
    Excluded,
    CancelExclusion,
    Resource,
    TestResource,
    /// Unmark as Resource Root / Test Resources Root.
    Unmark,
}

/// The stored folders after `action` on the folder `path`.
pub fn mark(config: &StructureConfig, path: &str, action: MarkAction) -> Vec<FolderMark> {
    let path = clean(path);
    let mut folders: Vec<FolderMark> = config
        .folders
        .iter()
        .filter(|m| clean(&m.path) != path)
        .cloned()
        .collect();
    if path.is_empty() {
        return config.folders.clone();
    }
    let rest = StructureConfig {
        folders: folders.clone(),
        exclude_patterns: config.exclude_patterns.clone(),
    };
    let excluded_without_mark = ExcludeSet::new(&rest).own_reason(&path, true).is_some();
    let push = |folders: &mut Vec<FolderMark>, kind| {
        folders.push(FolderMark {
            path: path.clone(),
            kind,
        })
    };
    match action {
        MarkAction::Excluded => {
            if !excluded_without_mark {
                push(&mut folders, FolderKind::Excluded);
            }
        }
        MarkAction::CancelExclusion | MarkAction::Unmark => {
            if excluded_without_mark {
                push(&mut folders, FolderKind::Included);
            }
        }
        MarkAction::Resource | MarkAction::TestResource => {
            if excluded_without_mark {
                push(&mut folders, FolderKind::Included);
            }
            push(
                &mut folders,
                if action == MarkAction::Resource {
                    FolderKind::Resource
                } else {
                    FolderKind::TestResource
                },
            );
        }
    }
    folders
}

/// How a folder is marked, for the menu and the tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderState {
    /// `excluded`, `resource` or `testResource`.
    pub kind: Option<FolderKind>,
    /// Why it is excluded.
    pub reason: Option<ExcludeReason>,
}

/// The mark of the entry at `path`.
pub fn state(config: &StructureConfig, path: &str, is_dir: bool) -> FolderState {
    let path = clean(path);
    let set = ExcludeSet::new(config);
    if let Some(reason) = set.reason(&path, is_dir) {
        return FolderState {
            kind: Some(FolderKind::Excluded),
            reason: Some(reason),
        };
    }
    let kind = config
        .folders
        .iter()
        .find(|m| {
            m.path == path && matches!(m.kind, FolderKind::Resource | FolderKind::TestResource)
        })
        .map(|m| m.kind);
    FolderState { kind, reason: None }
}

/// One folder of the Settings | Directories lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderRow {
    pub path: String,
    pub kind: FolderKind,
}

/// The folders the Directories page lists, by kind and path (cancelled default exclusions
/// as `included`).
pub fn describe(config: &StructureConfig) -> Vec<FolderRow> {
    let mut rows: Vec<FolderRow> = config
        .folders
        .iter()
        .map(|m| FolderRow {
            path: clean(&m.path),
            kind: m.kind,
        })
        .collect();
    let rank = |k: FolderKind| match k {
        FolderKind::Resource => 0,
        FolderKind::TestResource => 1,
        FolderKind::Excluded => 2,
        FolderKind::Included => 3,
    };
    rows.sort_by(|a, b| (rank(a.kind), &a.path).cmp(&(rank(b.kind), &b.path)));
    rows.dedup();
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(folders: &[(&str, FolderKind)], patterns: &str) -> StructureConfig {
        StructureConfig {
            folders: folders
                .iter()
                .map(|(p, k)| FolderMark {
                    path: p.to_string(),
                    kind: *k,
                })
                .collect(),
            exclude_patterns: patterns.into(),
        }
    }

    #[test]
    fn globs_match_names() {
        assert!(glob_matches("*.min.js", "app.min.js"));
        assert!(!glob_matches("*.min.js", "app.js"));
        assert!(glob_matches("tmp", "tmp"));
        assert!(glob_matches("?.log", "a.log"));
        assert!(!glob_matches("?.log", "ab.log"));
        assert!(glob_matches("*", "anything"));
        assert!(glob_matches("a*b*c", "aXXbYYc"));
        assert!(!glob_matches("a*b*c", "aXXbYY"));
    }

    #[test]
    fn defaults_exclude_node_modules_but_not_vendor_and_hide_git() {
        let set = ExcludeSet::default();
        assert_eq!(
            set.reason("node_modules", true),
            Some(ExcludeReason::Default)
        );
        assert_eq!(
            set.reason("web/node_modules/vue/index.js", false),
            Some(ExcludeReason::Parent)
        );
        assert!(!set.excludes("vendor/laravel/x.php", false));
        assert!(set.excludes(".git", true));
        assert!(set.excludes(".git/config", false));
        assert!(!set.excludes("app/Models/User.php", false));
        // A file called like a default folder is not excluded.
        assert_eq!(set.own_reason("target", false), None);
    }

    #[test]
    fn user_folders_patterns_and_cancelled_defaults() {
        let set = ExcludeSet::new(&config(
            &[
                ("storage/logs", FolderKind::Excluded),
                ("node_modules", FolderKind::Included),
            ],
            "*.min.js; tmp ;",
        ));
        assert_eq!(set.reason("storage/logs", true), Some(ExcludeReason::User));
        assert_eq!(
            set.reason("storage/logs/laravel.log", false),
            Some(ExcludeReason::Parent)
        );
        assert!(!set.excludes("storage/app/a.txt", false));
        assert!(!set.excludes("node_modules/vue/index.js", false));
        assert!(set.excludes("web/node_modules", true));
        assert_eq!(
            set.reason("public/app.min.js", false),
            Some(ExcludeReason::Pattern)
        );
        assert_eq!(set.reason("src/tmp", true), Some(ExcludeReason::Pattern));
        assert!(set.skips("src/tmp", "tmp", true));
        assert!(!set.skips("src", "src", true));
        assert!(!set.default_names().contains(&"node_modules".to_string()));
    }

    #[test]
    fn marking_and_cancelling_exclusion() {
        let empty = StructureConfig::default();
        let folders = mark(&empty, "storage/logs/", MarkAction::Excluded);
        assert_eq!(
            folders,
            vec![FolderMark {
                path: "storage/logs".into(),
                kind: FolderKind::Excluded
            }]
        );
        let cfg = StructureConfig {
            folders,
            ..empty.clone()
        };
        assert!(mark(&cfg, "storage/logs", MarkAction::CancelExclusion).is_empty());

        // A default exclusion is cancelled with an `included` entry and restored by
        // removing it.
        let folders = mark(&empty, "node_modules", MarkAction::CancelExclusion);
        assert_eq!(folders[0].kind, FolderKind::Included);
        let cfg = StructureConfig {
            folders,
            ..empty.clone()
        };
        assert_eq!(state(&cfg, "node_modules", true).kind, None);
        assert!(mark(&cfg, "node_modules", MarkAction::Excluded).is_empty());

        // Marking an excluded folder as resource root keeps it in the project.
        let res = mark(&empty, "node_modules", MarkAction::Resource);
        assert_eq!(res.len(), 2);
        let cfg = StructureConfig {
            folders: res,
            ..empty
        };
        assert_eq!(
            state(&cfg, "node_modules", true).kind,
            Some(FolderKind::Resource)
        );
        let unmarked = mark(&cfg, "node_modules", MarkAction::Unmark);
        assert_eq!(unmarked[0].kind, FolderKind::Included);
    }

    #[test]
    fn reads_settings_and_describes_rows() {
        let cfg = StructureConfig::from_settings(|k| match k {
            FOLDERS_KEY => Some(serde_json::json!([
                { "path": "./tmp/", "kind": "excluded" },
                { "path": "resources", "kind": "resource" },
                { "path": "", "kind": "excluded" },
                { "path": "x", "kind": "unknown" },
            ])),
            PATTERNS_KEY => Some(serde_json::json!("*.log")),
            _ => None,
        });
        assert_eq!(cfg.folders.len(), 2);
        assert_eq!(cfg.folders[0].path, "tmp");
        let rows = describe(&cfg);
        assert_eq!(rows[0].kind, FolderKind::Resource);
        assert_eq!(rows[1].path, "tmp");
        assert_eq!(
            state(&cfg, "storage/x.log", false).reason,
            Some(ExcludeReason::Pattern)
        );
    }
}
