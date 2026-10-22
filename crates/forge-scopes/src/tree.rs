//! The project tree under the pattern on the Scopes page: every file with whether the scope
//! contains it, and per folder how many of its files are in the scope.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{ParseError, Scope, ScopeFilter};

/// A file or folder of the preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNode {
    pub name: String,
    pub path: String,
    pub dir: bool,
    /// Files: in the scope. Folders: every file below is.
    pub matched: bool,
    /// Files in the scope below a folder (1 or 0 for a file).
    pub matched_files: usize,
    pub total_files: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeNode>,
}

/// The preview of a pattern.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeTree {
    pub nodes: Vec<TreeNode>,
    pub matched: usize,
    pub total: usize,
    /// The pattern does not parse; no file is marked then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ParseError>,
}

#[derive(Default)]
struct Dir {
    dirs: BTreeMap<String, Dir>,
    files: Vec<String>,
}

fn sort_key(name: &str) -> (String, String) {
    (name.to_lowercase(), name.to_string())
}

fn build_dir(prefix: &str, dir: Dir, scope: Option<&Scope>) -> Vec<TreeNode> {
    let mut dirs: Vec<(String, Dir)> = dir.dirs.into_iter().collect();
    dirs.sort_by_key(|(name, _)| sort_key(name));
    let mut out = Vec::new();
    for (name, sub) in dirs {
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let children = build_dir(&path, sub, scope);
        let matched_files: usize = children.iter().map(|c| c.matched_files).sum();
        let total_files: usize = children.iter().map(|c| c.total_files).sum();
        out.push(TreeNode {
            name,
            matched: total_files > 0 && matched_files == total_files,
            path,
            dir: true,
            matched_files,
            total_files,
            children,
        });
    }
    let mut files = dir.files;
    files.sort_by_key(|n| sort_key(n));
    for name in files {
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let matched = scope.is_some_and(|s| s.contains(&path));
        out.push(TreeNode {
            name,
            path,
            dir: false,
            matched,
            matched_files: usize::from(matched),
            total_files: 1,
            children: Vec::new(),
        });
    }
    out
}

/// The preview of `filter` over the project's `files`.
pub fn build(files: &[String], filter: &ScopeFilter) -> ScopeTree {
    let (scope, error) = match Scope::new(filter) {
        Ok(s) => (Some(s), None),
        Err(e) => (None, Some(e)),
    };
    let mut root = Dir::default();
    for path in files {
        let mut parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let Some(name) = parts.pop() else { continue };
        let mut dir = &mut root;
        for p in parts {
            dir = dir.dirs.entry(p.to_string()).or_default();
        }
        dir.files.push(name.to_string());
    }
    let nodes = build_dir("", root, scope.as_ref());
    ScopeTree {
        matched: nodes.iter().map(|n| n.matched_files).sum(),
        total: nodes.iter().map(|n| n.total_files).sum(),
        nodes,
        error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ScopeSpec;

    #[test]
    fn marks_files_and_counts_folders() {
        let files: Vec<String> = [
            "composer.json",
            "app/Models/User.php",
            "app/Models/Order.php",
            "app/helpers.php",
            "tests/Unit/UserTest.php",
        ]
        .map(String::from)
        .to_vec();
        let filter = ScopeFilter {
            scope: ScopeSpec::Pattern("file:app/Models//*".into()),
            ..Default::default()
        };
        let tree = build(&files, &filter);
        assert_eq!((tree.matched, tree.total), (2, 5));
        let names: Vec<&str> = tree.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["app", "tests", "composer.json"]);
        let app = &tree.nodes[0];
        assert_eq!(
            (app.matched_files, app.total_files, app.matched),
            (2, 3, false)
        );
        assert_eq!(app.children[0].name, "Models");
        assert!(app.children[0].matched);
        assert_eq!(app.children[0].children[0].name, "Order.php");
        let bad = build(
            &files,
            &ScopeFilter {
                scope: ScopeSpec::Pattern("file:a||".into()),
                ..Default::default()
            },
        );
        assert!(bad.error.is_some() && bad.matched == 0);
    }
}
