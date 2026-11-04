//! Checking a plugin's backend calls against the permissions the user granted, and the
//! permission summary shown before install.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

use crate::manifest::{EditorAccess, Permissions};

/// The roots file system scopes start from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsRoot {
    /// The open project's folder.
    Project,
    /// The plugin's own folder in the user data directory (`<data>/forge/plugin-data/<id>`).
    PluginData,
}

/// A parsed scope: `${project}`, `${pluginData}/cache`, …
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsScope {
    pub root: FsRoot,
    /// Sub folder below the root (`/` separated, may be empty).
    pub sub: String,
}

impl FsScope {
    pub fn parse(s: &str) -> Option<Self> {
        let (root, rest) = if let Some(r) = s.strip_prefix("${project}") {
            (FsRoot::Project, r)
        } else if let Some(r) = s.strip_prefix("${pluginData}") {
            (FsRoot::PluginData, r)
        } else {
            return None;
        };
        let sub = match rest {
            "" | "/" => String::new(),
            r => {
                let r = r.strip_prefix('/')?.trim_end_matches('/');
                if !crate::manifest::is_package_path(r) {
                    return None;
                }
                r.to_string()
            }
        };
        Some(Self { root, sub })
    }
}

/// Where the scope roots are for one call.
#[derive(Debug, Clone)]
pub struct Roots {
    pub project: Option<PathBuf>,
    pub plugin_data: PathBuf,
}

/// Why a call was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Denied(pub String);

impl std::fmt::Display for Denied {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The granted permissions of one plugin.
pub struct Guard<'a> {
    pub plugin_id: &'a str,
    pub granted: &'a Permissions,
}

/// Resolve `.` and `..` without touching the disk.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// The real location of `path`: symlinks of its longest existing ancestor are resolved, so
/// a link inside an allowed folder cannot point outside of it.
fn real(path: &Path) -> PathBuf {
    let path = normalize(path);
    let mut existing = path.as_path();
    let mut rest = Vec::new();
    loop {
        if let Ok(canon) = existing.canonicalize() {
            let mut out = canon;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (existing.parent(), existing.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_os_string());
                existing = parent;
            }
            _ => return path,
        }
    }
}

impl Guard<'_> {
    fn deny(&self, what: String) -> Denied {
        Denied(format!(
            "plugin `{}` has no permission to {what}",
            self.plugin_id
        ))
    }

    /// The real path of `path` when a granted scope covers it.
    pub fn check_fs(&self, path: &Path, write: bool, roots: &Roots) -> Result<PathBuf, Denied> {
        let verb = if write { "write" } else { "read" };
        if !path.is_absolute() {
            return Err(Denied(format!(
                "{} is not an absolute path",
                path.display()
            )));
        }
        let target = real(path);
        let scopes = if write {
            &self.granted.fs.write
        } else {
            // Writable folders are readable too.
            &self.granted.fs.read
        };
        let extra = if write {
            None
        } else {
            Some(&self.granted.fs.write)
        };
        let allowed = scopes
            .iter()
            .chain(extra.into_iter().flatten())
            .filter_map(|s| FsScope::parse(s))
            .filter_map(|scope| {
                let base = match scope.root {
                    FsRoot::Project => roots.project.clone()?,
                    FsRoot::PluginData => roots.plugin_data.clone(),
                };
                let base = if scope.sub.is_empty() {
                    base
                } else {
                    base.join(&scope.sub)
                };
                Some(real(&base))
            })
            .any(|base| target.starts_with(&base));
        if allowed {
            Ok(target)
        } else {
            Err(self.deny(format!("{verb} {}", path.display())))
        }
    }

    /// `url` must be http(s) to a granted host.
    pub fn check_network(&self, url: &str) -> Result<(), Denied> {
        let parsed = url::Url::parse(url).map_err(|e| Denied(format!("invalid URL {url}: {e}")))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(Denied(format!(
                "only http and https URLs are allowed: {url}"
            )));
        }
        let host = parsed.host_str().unwrap_or_default().to_ascii_lowercase();
        let ok = self.granted.network.iter().any(|pattern| {
            let pattern = pattern.to_ascii_lowercase();
            match pattern.strip_prefix("*.") {
                Some(domain) => host.ends_with(&format!(".{domain}")),
                None => host == pattern,
            }
        });
        if ok {
            Ok(())
        } else {
            Err(self.deny(format!("connect to {host}")))
        }
    }

    pub fn check_process(&self, program: &str) -> Result<(), Denied> {
        if self.granted.process.iter().any(|p| p == program) {
            Ok(())
        } else {
            Err(self.deny(format!("start `{program}`")))
        }
    }

    /// The plugin's own settings (`<id>.…`) are always allowed.
    pub fn check_setting(&self, key: &str) -> Result<(), Denied> {
        let own = key.starts_with(self.plugin_id)
            && key.as_bytes().get(self.plugin_id.len()) == Some(&b'.');
        let granted = self
            .granted
            .settings
            .iter()
            .any(|p| match p.strip_suffix(".*") {
                Some(prefix) => key.starts_with(prefix) && key[prefix.len()..].starts_with('.'),
                None => key == p,
            });
        if own || granted {
            Ok(())
        } else {
            Err(self.deny(format!("access the setting `{key}`")))
        }
    }

    pub fn check_editor(&self, access: EditorAccess) -> Result<(), Denied> {
        if self.granted.editor.contains(&access) {
            Ok(())
        } else {
            let what = match access {
                EditorAccess::Read => "read the editor",
                EditorAccess::Write => "edit the editor",
            };
            Err(self.deny(what.to_string()))
        }
    }
}

/// How much a permission exposes, for the install prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Risk {
    Low,
    Medium,
    High,
}

/// One line of the permission prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionLine {
    /// `fs.read`, `fs.write`, `network`, `process`, `settings`, `editor.read`, `editor.write`.
    pub kind: String,
    pub label: String,
    pub detail: String,
    pub risk: Risk,
}

fn scope_label(s: &str) -> String {
    match FsScope::parse(s) {
        Some(scope) => {
            let root = match scope.root {
                FsRoot::Project => "the open project",
                FsRoot::PluginData => "its own data folder",
            };
            if scope.sub.is_empty() {
                root.to_string()
            } else {
                format!("{root} ({})", scope.sub)
            }
        }
        None => s.to_string(),
    }
}

/// What the install prompt lists, most dangerous first.
pub fn summary(p: &Permissions) -> Vec<PermissionLine> {
    let mut out = Vec::new();
    let line = |kind: &str, label: &str, detail: String, risk: Risk| PermissionLine {
        kind: kind.into(),
        label: label.into(),
        detail,
        risk,
    };
    if !p.process.is_empty() {
        out.push(line(
            "process",
            "Start programs",
            p.process.join(", "),
            Risk::High,
        ));
    }
    if !p.fs.write.is_empty() {
        let project =
            p.fs.write
                .iter()
                .any(|s| FsScope::parse(s).is_some_and(|f| f.root == FsRoot::Project));
        out.push(line(
            "fs.write",
            "Write files",
            p.fs.write
                .iter()
                .map(|s| scope_label(s))
                .collect::<Vec<_>>()
                .join(", "),
            if project { Risk::High } else { Risk::Low },
        ));
    }
    if !p.network.is_empty() {
        out.push(line(
            "network",
            "Connect to the internet",
            p.network.join(", "),
            Risk::Medium,
        ));
    }
    if p.editor.contains(&EditorAccess::Write) {
        out.push(line(
            "editor.write",
            "Edit the open file",
            "Changes the text of the active editor".into(),
            Risk::Medium,
        ));
    }
    if !p.fs.read.is_empty() {
        out.push(line(
            "fs.read",
            "Read files",
            p.fs.read
                .iter()
                .map(|s| scope_label(s))
                .collect::<Vec<_>>()
                .join(", "),
            Risk::Medium,
        ));
    }
    if !p.settings.is_empty() {
        out.push(line(
            "settings",
            "Read and change IDE settings",
            p.settings.join(", "),
            Risk::Medium,
        ));
    }
    if p.editor.contains(&EditorAccess::Read) {
        out.push(line(
            "editor.read",
            "Read the open file",
            "Text, selection and file name of the active editor".into(),
            Risk::Low,
        ));
    }
    out
}

/// Permissions in `wanted` that `granted` does not cover (an update asking for more).
pub fn added(granted: &Permissions, wanted: &Permissions) -> Permissions {
    let new = |a: &[String], b: &[String]| -> Vec<String> {
        b.iter().filter(|x| !a.contains(x)).cloned().collect()
    };
    Permissions {
        fs: crate::manifest::FsPermissions {
            read: new(&granted.fs.read, &wanted.fs.read),
            write: new(&granted.fs.write, &wanted.fs.write),
        },
        network: new(&granted.network, &wanted.network),
        process: new(&granted.process, &wanted.process),
        settings: new(&granted.settings, &wanted.settings),
        editor: wanted
            .editor
            .iter()
            .filter(|e| !granted.editor.contains(e))
            .copied()
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::FsPermissions;

    fn perms() -> Permissions {
        Permissions {
            fs: FsPermissions {
                read: vec!["${project}/docs".into()],
                write: vec!["${pluginData}".into()],
            },
            network: vec!["api.example.com".into(), "*.cdn.example.org".into()],
            process: vec!["git".into()],
            settings: vec!["editor.font.*".into(), "appearance.theme".into()],
            editor: vec![EditorAccess::Read],
        }
    }

    #[test]
    fn file_system_scopes() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        let data = dir.path().join("data");
        std::fs::create_dir_all(project.join("docs")).unwrap();
        std::fs::create_dir_all(project.join("src")).unwrap();
        std::fs::create_dir_all(&data).unwrap();
        let roots = Roots {
            project: Some(project.clone()),
            plugin_data: data.clone(),
        };
        let p = perms();
        let g = Guard {
            plugin_id: "acme.tool",
            granted: &p,
        };
        assert!(g
            .check_fs(&project.join("docs/a.md"), false, &roots)
            .is_ok());
        assert!(g
            .check_fs(&project.join("src/a.php"), false, &roots)
            .is_err());
        assert!(g
            .check_fs(&project.join("docs/../src/a.php"), false, &roots)
            .is_err());
        assert!(g
            .check_fs(&project.join("docs/a.md"), true, &roots)
            .is_err());
        assert!(g
            .check_fs(&data.join("cache/new.json"), true, &roots)
            .is_ok());
        // Writable folders are readable.
        assert!(g.check_fs(&data.join("x"), false, &roots).is_ok());
        assert!(g.check_fs(Path::new("docs/a.md"), false, &roots).is_err());
        let no_project = Roots {
            project: None,
            plugin_data: data.clone(),
        };
        assert!(g
            .check_fs(&project.join("docs/a.md"), false, &no_project)
            .is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(project.join("src"), project.join("docs/escape")).unwrap();
            let e = g
                .check_fs(&project.join("docs/escape/a.php"), false, &roots)
                .unwrap_err();
            assert!(e.0.contains("acme.tool"), "{e}");
        }
    }

    #[test]
    fn network_process_settings_editor() {
        let p = perms();
        let g = Guard {
            plugin_id: "acme.tool",
            granted: &p,
        };
        assert!(g.check_network("https://api.example.com/v1").is_ok());
        assert!(g.check_network("https://API.example.com:8443/").is_ok());
        assert!(g
            .check_network("https://evil.com/?api.example.com")
            .is_err());
        assert!(g.check_network("https://a.cdn.example.org/x").is_ok());
        assert!(g.check_network("https://cdn.example.org/x").is_err());
        assert!(g.check_network("file:///etc/passwd").is_err());
        assert!(g.check_process("git").is_ok());
        assert!(g.check_process("sh").is_err());
        assert!(g.check_setting("acme.tool.enabled").is_ok());
        assert!(g.check_setting("acme.toolbox.enabled").is_err());
        assert!(g.check_setting("editor.font.size").is_ok());
        assert!(g.check_setting("editor.fontX").is_err());
        assert!(g.check_setting("appearance.theme").is_ok());
        assert!(g.check_setting("ai.providers").is_err());
        assert!(g.check_editor(EditorAccess::Read).is_ok());
        assert!(g.check_editor(EditorAccess::Write).is_err());
    }

    #[test]
    fn summary_orders_by_risk_and_added_finds_new_permissions() {
        let lines = summary(&perms());
        let kinds: Vec<&str> = lines.iter().map(|l| l.kind.as_str()).collect();
        assert_eq!(
            kinds,
            [
                "process",
                "fs.write",
                "network",
                "fs.read",
                "settings",
                "editor.read"
            ]
        );
        assert_eq!(lines[1].risk, Risk::Low);
        assert_eq!(lines[3].detail, "the open project (docs)");
        let mut more = perms();
        more.network.push("upload.example.com".into());
        more.editor.push(EditorAccess::Write);
        let new = added(&perms(), &more);
        assert_eq!(new.network, ["upload.example.com"]);
        assert_eq!(new.editor, [EditorAccess::Write]);
        assert!(added(&perms(), &perms()).is_empty());
        assert!(summary(&Permissions::default()).is_empty());
    }
}
