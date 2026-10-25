//! The Remote Host tool window (Tools | Deployment | Browse Remote Host): the folders of a
//! server below its root path as tree nodes, with the project path each one is mapped to
//! (mapped folders are highlighted), excluded paths, and the names of New File / New
//! Directory / Rename checked before anything is sent to the server.

use serde::{Deserialize, Serialize};

use crate::compare::Entry;
use crate::config::{DeployServer, ExcludedSide};
use crate::mapping::{join, name, normalize, parent};

/// One file or folder of the remote tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteNode {
    /// Relative to the server's root path (`""` is the root).
    pub path: String,
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub modified: Option<i64>,
    /// The project path this file or folder is mapped to (Download from Here, Compare with
    /// Local Version), `None` when no mapping covers it.
    pub local: Option<String>,
    /// The deployment path of a mapping (shown highlighted, like PHPStorm's green folders).
    pub mapping_root: bool,
    /// Covered by an Excluded Paths entry of the deployment side.
    pub excluded: bool,
}

/// The tree node of a listed entry.
pub fn node(server: &DeployServer, entry: &Entry) -> RemoteNode {
    let path = normalize(&entry.path);
    let mapping_root = server
        .effective_mappings()
        .iter()
        .any(|m| normalize(&m.deployment) == path);
    RemoteNode {
        name: if path.is_empty() {
            server.display_path("")
        } else {
            name(&path).to_string()
        },
        local: server.local_path(&path),
        excluded: server.is_excluded(ExcludedSide::Deployment, &path),
        mapping_root,
        dir: entry.dir,
        size: if entry.dir { 0 } else { entry.size },
        modified: entry.modified,
        path,
    }
}

/// The nodes of a folder listing: folders first, then files, by name ignoring case.
pub fn nodes(server: &DeployServer, entries: &[Entry]) -> Vec<RemoteNode> {
    let mut out: Vec<RemoteNode> = entries.iter().map(|e| node(server, e)).collect();
    out.sort_by(|a, b| {
        b.dir
            .cmp(&a.dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

/// Problems with a file or folder name typed into New File, New Directory or Rename.
pub fn check_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Enter a name".into());
    }
    if trimmed == "." || trimmed == ".." {
        return Err(format!("{trimmed} is not a valid name"));
    }
    if trimmed.contains(['/', '\\']) {
        return Err("The name must not contain / or \\".into());
    }
    if trimmed.chars().any(char::is_control) {
        return Err("The name must not contain control characters".into());
    }
    Ok(())
}

/// The path of a new entry `name` in the folder `dir`.
pub fn child_path(dir: &str, name: &str) -> Result<String, String> {
    check_name(name)?;
    Ok(join(&normalize(dir), name.trim()))
}

/// The path of `path` renamed to `new_name` (same folder). The root is never renamed.
pub fn renamed_path(path: &str, new_name: &str) -> Result<String, String> {
    let path = normalize(path);
    if path.is_empty() {
        return Err("The root folder cannot be renamed".into());
    }
    child_path(parent(&path), new_name)
}

/// Copy Path: the absolute path on the server (or this machine for local servers).
pub fn absolute_path(root: &str, path: &str) -> String {
    let root = root.trim_end_matches(['/', '\\']);
    let path = normalize(path);
    match (root.is_empty(), path.is_empty()) {
        (true, true) => "/".to_string(),
        (true, false) => format!("/{path}"),
        (false, true) => root.to_string(),
        (false, false) => format!("{root}/{path}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ExcludedPath, Mapping};

    fn server() -> DeployServer {
        DeployServer {
            name: "shop".into(),
            root_path: "/var/www/shop".into(),
            mappings: vec![
                Mapping {
                    local: "".into(),
                    deployment: "/".into(),
                    web: "".into(),
                },
                Mapping {
                    local: "public".into(),
                    deployment: "/htdocs".into(),
                    web: "/".into(),
                },
            ],
            excluded: vec![ExcludedPath {
                side: ExcludedSide::Deployment,
                path: "htdocs/uploads".into(),
            }],
            ..DeployServer::default()
        }
    }

    #[test]
    fn nodes_carry_mappings_and_sort_folders_first() {
        let s = server();
        let entries = vec![
            Entry::file("htdocs/index.php", 42, Some(1)),
            Entry::dir("htdocs/uploads", None),
            Entry::dir("htdocs/Assets", None),
            Entry::file("htdocs/.htaccess", 7, None),
            Entry::dir("htdocs/css", None),
        ];
        let listed = nodes(&s, &entries);
        let names: Vec<&str> = listed.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Assets", "css", "uploads", ".htaccess", "index.php"]
        );
        let index = listed.iter().find(|n| n.name == "index.php").unwrap();
        assert_eq!(index.local.as_deref(), Some("public/index.php"));
        assert_eq!((index.size, index.modified), (42, Some(1)));
        let uploads = listed.iter().find(|n| n.name == "uploads").unwrap();
        assert!(uploads.excluded);
        assert_eq!(uploads.local, None);

        let top = nodes(&s, &[Entry::dir("htdocs", None), Entry::dir("app", None)]);
        assert!(
            top.iter()
                .find(|n| n.name == "htdocs")
                .unwrap()
                .mapping_root
        );
        let app = top.iter().find(|n| n.name == "app").unwrap();
        assert!(!app.mapping_root);
        assert_eq!(app.local.as_deref(), Some("app"));
        // The root node is named after the root path and is the project root's mapping.
        let root = node(&s, &Entry::dir("", None));
        assert_eq!(root.name, "/var/www/shop");
        assert!(root.mapping_root);
        assert_eq!(root.local.as_deref(), Some(""));
    }

    #[test]
    fn names_are_checked_before_they_reach_the_server() {
        assert_eq!(
            child_path("htdocs/", " new.php ").unwrap(),
            "htdocs/new.php"
        );
        assert_eq!(child_path("", "a").unwrap(), "a");
        assert!(child_path("x", "").is_err());
        assert!(child_path("x", "..").is_err());
        assert!(child_path("x", "a/b").is_err());
        assert!(child_path("x", "a\\b").is_err());
        assert!(child_path("x", "a\nb").is_err());
        assert_eq!(
            renamed_path("htdocs/a.php", "b.php").unwrap(),
            "htdocs/b.php"
        );
        assert_eq!(renamed_path("a.php", "b.php").unwrap(), "b.php");
        assert!(renamed_path("", "x").is_err());
    }

    #[test]
    fn absolute_paths_for_copy_path() {
        assert_eq!(
            absolute_path("/var/www/shop", "htdocs/a.php"),
            "/var/www/shop/htdocs/a.php"
        );
        assert_eq!(absolute_path("/var/www/shop/", ""), "/var/www/shop");
        assert_eq!(absolute_path("/", "etc"), "/etc");
        assert_eq!(absolute_path("", ""), "/");
    }
}
