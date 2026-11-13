//! Recently opened projects, shown on the Welcome screen (like PHPStorm's project list).

use std::fs;
use std::path::PathBuf;

use forge_backend::RemoteHost;
use serde::{Deserialize, Serialize};

const MAX_RECENT: usize = 20;

/// Where a remote recent project lives.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentRemote {
    #[serde(flatten)]
    pub host: RemoteHost,
    /// Project folder on the host.
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub name: String,
    /// Local path, or `user@host:/path` for remote projects.
    pub root: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<RecentRemote>,
    /// Detected framework of a remote project (local ones are detected when listed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<forge_project::Framework>,
}

impl RecentProject {
    pub fn local(name: String, root: String) -> Self {
        Self {
            name,
            root,
            remote: None,
            primary: None,
        }
    }
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

fn store(list: &[RecentProject]) {
    let Some(path) = store_path() else { return };
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, serde_json::to_string_pretty(list)?)
    };
    if let Err(e) = write() {
        tracing::warn!("cannot save recent projects: {e}");
    }
}

/// Move `project` to the top of the list and persist it. Failures are only logged:
/// a missing recent list must never prevent opening a project.
pub fn remember(project: RecentProject) {
    let mut list = load();
    push_front(&mut list, project);
    store(&list);
}

/// Remember the detected framework of a remote project for its Welcome screen badge.
pub fn set_primary(root: &str, primary: Option<forge_project::Framework>) {
    let mut list = load();
    let Some(p) = list
        .iter_mut()
        .find(|p| p.root == root && p.remote.is_some())
    else {
        return;
    };
    if p.primary == primary {
        return;
    }
    p.primary = primary;
    store(&list);
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
        RecentProject::local(root.into(), root.into())
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

    #[test]
    fn old_lists_without_remote_entries_still_load() {
        let list: Vec<RecentProject> =
            serde_json::from_str(r#"[{"name":"shop","root":"/home/me/shop"}]"#).unwrap();
        assert_eq!(
            list,
            vec![RecentProject::local("shop".into(), "/home/me/shop".into())]
        );
        let remote = RecentProject {
            name: "shop".into(),
            root: "deploy@shop.example.com:/var/www/shop".into(),
            remote: Some(RecentRemote {
                host: RemoteHost {
                    host: "shop.example.com".into(),
                    port: 22,
                    user: "deploy".into(),
                    config_id: Some("c1".into()),
                    kind: Default::default(),
                },
                path: "/var/www/shop".into(),
            }),
            primary: None,
        };
        let json = serde_json::to_value(&remote).unwrap();
        assert_eq!(json["remote"]["host"], "shop.example.com");
        assert_eq!(json["remote"]["configId"], "c1");
        let back: RecentProject = serde_json::from_value(json).unwrap();
        assert_eq!(back, remote);
    }
}
