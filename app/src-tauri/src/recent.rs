//! Recently opened projects, shown on the Welcome screen (like PHPStorm's project list).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const MAX_RECENT: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecentProject {
    pub name: String,
    pub root: String,
}

fn store_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("forge").join("recent-projects.json"))
}

pub fn load() -> Vec<RecentProject> {
    store_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Move `project` to the top of the list and persist it. Failures are only logged:
/// a missing recent list must never prevent opening a project.
pub fn remember(project: RecentProject) {
    let mut list = load();
    push_front(&mut list, project);
    let Some(path) = store_path() else { return };
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, serde_json::to_string_pretty(&list)?)
    };
    if let Err(e) = write() {
        tracing::warn!("cannot save recent projects: {e}");
    }
}

fn push_front(list: &mut Vec<RecentProject>, project: RecentProject) {
    list.retain(|p| p.root != project.root);
    list.insert(0, project);
    list.truncate(MAX_RECENT);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(root: &str) -> RecentProject {
        RecentProject {
            name: root.into(),
            root: root.into(),
        }
    }

    #[test]
    fn most_recent_first_without_duplicates() {
        let mut list = vec![p("a"), p("b")];
        push_front(&mut list, p("b"));
        assert_eq!(list, vec![p("b"), p("a")]);
    }

    #[test]
    fn caps_the_list() {
        let mut list = Vec::new();
        for i in 0..30 {
            push_front(&mut list, p(&i.to_string()));
        }
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(list[0].root, "29");
    }
}
