//! Package manager detection, like WebStorm's "Package manager: auto": the `packageManager`
//! field (Corepack), else the lock file next to the `package.json` or in a parent directory
//! (workspaces), else npm.

use serde::{Deserialize, Serialize};

use crate::package_json::PackageJson;
use crate::source::Sources;
use crate::text::{join_path, parent_dir};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
    Bun,
}

impl PackageManager {
    pub const ALL: [PackageManager; 4] = [Self::Npm, Self::Pnpm, Self::Yarn, Self::Bun];

    pub fn id(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
            Self::Bun => "bun",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.id() == id.trim())
    }

    /// Lock files that identify the manager, most specific first.
    pub fn lock_files(self) -> &'static [&'static str] {
        match self {
            Self::Npm => &["package-lock.json", "npm-shrinkwrap.json"],
            Self::Pnpm => &["pnpm-lock.yaml"],
            Self::Yarn => &["yarn.lock"],
            Self::Bun => &["bun.lock", "bun.lockb"],
        }
    }

    /// Program and arguments that run a script: `npm run dev -- --port 3000`. npm needs
    /// `--` before script arguments; pnpm, yarn and bun pass them on as they are.
    pub fn run_script(self, script: &str, args: &[String]) -> Vec<String> {
        let mut out = vec![self.id().to_string(), "run".into(), script.to_string()];
        if !args.is_empty() {
            if self == Self::Npm {
                out.push("--".into());
            }
            out.extend(args.iter().cloned());
        }
        out
    }

    /// Program and arguments that install dependencies.
    pub fn install(self) -> Vec<String> {
        vec![self.id().to_string(), "install".into()]
    }
}

/// Why a package manager was chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Reason {
    /// The `web.packageManager` setting.
    Setting,
    /// `"packageManager"` in this or a parent `package.json`.
    Field { path: String, value: String },
    /// A lock file.
    LockFile { path: String },
    /// Nothing found.
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    pub manager: PackageManager,
    /// Version from the `packageManager` field (`9.15.0`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub reason: Reason,
}

impl Detection {
    /// "pnpm-lock.yaml", "packageManager field", "settings" or "default".
    pub fn describe(&self) -> String {
        match &self.reason {
            Reason::Setting => "settings".into(),
            Reason::Field { .. } => "packageManager field".into(),
            Reason::LockFile { path } => path.rsplit('/').next().unwrap_or(path).to_string(),
            Reason::Default => "default".into(),
        }
    }
}

/// The package manager for the `package.json` at `package_json` (workspace relative).
/// `setting` is the `web.packageManager` value (`auto` or a manager id).
pub fn detect(sources: &Sources, package_json: &str, setting: &str) -> Detection {
    if let Some(manager) = PackageManager::parse(setting) {
        return Detection {
            manager,
            version: None,
            reason: Reason::Setting,
        };
    }
    let mut dir = parent_dir(package_json).to_string();
    loop {
        let manifest = join_path(&dir, "package.json");
        if let Some(field) = sources
            .get(&manifest)
            .and_then(PackageJson::parse)
            .and_then(|p| p.package_manager)
        {
            let (name, version) = field.split_once('@').unwrap_or((field.as_str(), ""));
            if let Some(manager) = PackageManager::parse(name) {
                let version = version.split('+').next().unwrap_or("");
                return Detection {
                    manager,
                    version: (!version.is_empty()).then(|| version.to_string()),
                    reason: Reason::Field {
                        path: manifest,
                        value: field.clone(),
                    },
                };
            }
        }
        for manager in [
            PackageManager::Pnpm,
            PackageManager::Yarn,
            PackageManager::Bun,
            PackageManager::Npm,
        ] {
            if let Some(lock) = manager
                .lock_files()
                .iter()
                .map(|f| join_path(&dir, f))
                .find(|p| sources.exists(p))
            {
                return Detection {
                    manager,
                    version: None,
                    reason: Reason::LockFile { path: lock },
                };
            }
        }
        if dir.is_empty() {
            break;
        }
        dir = parent_dir(&dir).to_string();
    }
    Detection {
        manager: PackageManager::Npm,
        version: None,
        reason: Reason::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::SourceFile;

    fn sources(files: &[(&str, &str)], others: &[&str]) -> Sources {
        Sources::new(
            files
                .iter()
                .map(|(p, c)| SourceFile {
                    path: p.to_string(),
                    content: c.to_string(),
                })
                .collect(),
            others.iter().map(|s| s.to_string()),
        )
    }

    #[test]
    fn lock_files_decide() {
        for (lock, manager) in [
            ("pnpm-lock.yaml", PackageManager::Pnpm),
            ("yarn.lock", PackageManager::Yarn),
            ("bun.lockb", PackageManager::Bun),
            ("bun.lock", PackageManager::Bun),
            ("package-lock.json", PackageManager::Npm),
        ] {
            let s = sources(&[("package.json", "{}")], &[lock]);
            let d = detect(&s, "package.json", "auto");
            assert_eq!(d.manager, manager, "{lock}");
            assert_eq!(d.describe(), lock);
        }
        let d = detect(
            &sources(&[("package.json", "{}")], &[]),
            "package.json",
            "auto",
        );
        assert_eq!(
            (d.manager, d.reason),
            (PackageManager::Npm, Reason::Default)
        );
    }

    #[test]
    fn field_beats_lock_files_and_workspaces_inherit() {
        let s = sources(
            &[
                (
                    "package.json",
                    r#"{"packageManager": "yarn@4.6.0+sha512.abc"}"#,
                ),
                ("packages/ui/package.json", "{}"),
            ],
            &["package-lock.json"],
        );
        let d = detect(&s, "package.json", "auto");
        assert_eq!(
            (d.manager, d.version.as_deref()),
            (PackageManager::Yarn, Some("4.6.0"))
        );
        let d = detect(&s, "packages/ui/package.json", "auto");
        assert_eq!(d.manager, PackageManager::Yarn);

        let s = sources(&[("apps/web/package.json", "{}")], &["pnpm-lock.yaml"]);
        let d = detect(&s, "apps/web/package.json", "auto");
        assert_eq!(
            d.reason,
            Reason::LockFile {
                path: "pnpm-lock.yaml".into()
            }
        );
    }

    #[test]
    fn setting_overrides_detection() {
        let s = sources(&[("package.json", "{}")], &["yarn.lock"]);
        assert_eq!(
            detect(&s, "package.json", "bun").manager,
            PackageManager::Bun
        );
    }

    #[test]
    fn command_lines() {
        let args = vec!["--port".to_string(), "3000".to_string()];
        assert_eq!(
            PackageManager::Npm.run_script("dev", &args).join(" "),
            "npm run dev -- --port 3000"
        );
        assert_eq!(
            PackageManager::Pnpm.run_script("dev", &args).join(" "),
            "pnpm run dev --port 3000"
        );
        assert_eq!(
            PackageManager::Bun.run_script("test", &[]).join(" "),
            "bun run test"
        );
        assert_eq!(PackageManager::Yarn.install().join(" "), "yarn install");
    }
}
