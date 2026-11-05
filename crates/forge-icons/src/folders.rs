//! Folder options that do not depend on the theme: the user's folder colors
//! (`fileIcons.folderColors`) and which folders hold open files (hollow folders).

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::{glob_match, is_hex_color, relative_path, Problem};

/// One row of the `fileIcons.folderColors` table: folders matching `pattern` are drawn in
/// `color`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderColor {
    /// A folder name or glob (`vendor`, `*-cache`), or a path glob relative to the project
    /// root when it contains a `/` (`app/Http/*`; a leading `/` anchors it at the root,
    /// otherwise it matches at any depth).
    pub pattern: String,
    /// `#rgb` or `#rrggbb`.
    pub color: String,
}

/// Matches folders against the user's folder colors, first row first, caching per path.
#[derive(Debug, Default)]
pub struct FolderColors {
    rules: Vec<FolderColor>,
    root: Option<String>,
    cache: HashMap<String, Option<String>>,
}

impl FolderColors {
    /// Rows with an empty pattern or a color that is not a hex color are skipped and
    /// reported.
    pub fn new(rows: &[FolderColor], project_root: Option<&str>) -> (Self, Vec<Problem>) {
        let mut problems = Vec::new();
        let mut rules = Vec::new();
        for (i, r) in rows.iter().enumerate() {
            let pattern = r.pattern.trim();
            if pattern.is_empty() {
                problems.push(format!("folderColors[{i}]: the folder is empty"));
            } else if !is_hex_color(r.color.trim()) {
                problems.push(format!(
                    "folderColors[{i}]: `{}` is not a color like #e0a040",
                    r.color
                ));
            } else {
                rules.push(FolderColor {
                    pattern: pattern.to_string(),
                    color: r.color.trim().to_string(),
                });
            }
        }
        let colors = FolderColors {
            rules,
            root: project_root.map(str::to_string),
            cache: HashMap::new(),
        };
        (colors, problems)
    }

    /// The color of the folder at `path`, if a row matches.
    pub fn color_of(&mut self, path: &str) -> Option<String> {
        if self.rules.is_empty() || path.is_empty() {
            return None;
        }
        if let Some(hit) = self.cache.get(path) {
            return hit.clone();
        }
        let rel = relative_path(path, self.root.as_deref());
        let name = rel.rsplit('/').next().unwrap_or(&rel).to_lowercase();
        let hit = self
            .rules
            .iter()
            .find(|r| {
                if r.pattern.contains('/') {
                    path_glob(&r.pattern, &rel)
                } else {
                    glob_match(&r.pattern.to_lowercase(), &name)
                }
            })
            .map(|r| r.color.clone());
        self.cache.insert(path.to_string(), hit.clone());
        hit
    }
}

/// Match a path glob against a path relative to the project root: a leading `/` anchors
/// it at the root, otherwise it may match at any folder depth (`Controllers/*.php` matches
/// `app/Http/Controllers/UserController.php`).
pub fn path_glob(pattern: &str, rel: &str) -> bool {
    match pattern.strip_prefix('/') {
        Some(anchored) => glob_match(anchored, rel),
        None => glob_match(pattern, rel) || glob_match(&format!("**/{pattern}"), rel),
    }
}

/// The folders (relative to `project_root`, `/` separated) that contain at least one of
/// `open_files`, at any depth. Files outside the project count with their own path.
pub fn folders_with_open_files(
    open_files: &[String],
    project_root: Option<&str>,
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for f in open_files {
        let rel = relative_path(f, project_root);
        let mut rest = rel.as_str();
        while let Some((dir, _)) = rest.rsplit_once('/') {
            if dir.is_empty() || !out.insert(dir.to_string()) {
                break;
            }
            rest = dir;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pattern: &str, color: &str) -> FolderColor {
        FolderColor {
            pattern: pattern.into(),
            color: color.into(),
        }
    }

    #[test]
    fn folder_colors_match_names_globs_and_paths_first_row_first() {
        let (mut c, problems) = FolderColors::new(
            &[
                row("vendor", "#888"),
                row("app/Http/*", "#3366ff"),
                row("*-cache", "#aa0000"),
                row("/tests", "#00aa00"),
                row("", "#fff"),
                row("x", "blue"),
            ],
            Some("/work/shop"),
        );
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert_eq!(c.color_of("vendor").as_deref(), Some("#888"));
        assert_eq!(c.color_of("/work/shop/Vendor").as_deref(), Some("#888"));
        assert_eq!(
            c.color_of("app/Http/Controllers").as_deref(),
            Some("#3366ff")
        );
        assert_eq!(
            c.color_of("mod/app/Http/Middleware").as_deref(),
            Some("#3366ff")
        );
        assert_eq!(c.color_of("storage/view-cache").as_deref(), Some("#aa0000"));
        assert_eq!(c.color_of("tests").as_deref(), Some("#00aa00"));
        assert_eq!(c.color_of("pkg/tests"), None, "anchored at the root");
        assert_eq!(c.color_of("src"), None);
    }

    #[test]
    fn open_files_mark_every_folder_above_them() {
        let open = vec![
            "/work/shop/app/Http/Kernel.php".to_string(),
            "app/Models/User.php".to_string(),
            "README.md".to_string(),
        ];
        let dirs: Vec<String> = folders_with_open_files(&open, Some("/work/shop"))
            .into_iter()
            .collect();
        assert_eq!(dirs, vec!["app", "app/Http", "app/Models"]);
    }
}
