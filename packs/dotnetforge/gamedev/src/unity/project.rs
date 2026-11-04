//! A Unity project as Forge sees it from its files. Reading runs nothing.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::package::{self, PACKAGE_NAME};

pub const PROJECT_VERSION: &str = "ProjectSettings/ProjectVersion.txt";
pub const MANIFEST: &str = "Packages/manifest.json";
pub const PACKAGES_LOCK: &str = "Packages/packages-lock.json";
/// Written by the Unity editor while it has the project open.
pub const EDITOR_INSTANCE: &str = "Library/EditorInstance.json";
/// Written by the Forge editor bridge (the `dev.forge.unity` package) while it listens.
pub const BRIDGE_INSTANCE: &str = "Library/ForgeBridge.json";

/// Unity's IDE integration packages; each writes the `.sln` / `.csproj` files.
pub const IDE_PACKAGES: &[(&str, &str)] = &[
    ("com.unity.ide.visualstudio", "Visual Studio Editor"),
    ("com.unity.ide.rider", "JetBrains Rider Editor"),
    ("com.unity.ide.vscode", "Visual Studio Code Editor"),
];

pub const TEST_FRAMEWORK: &str = "com.unity.test-framework";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    pub name: String,
    /// A version, a `file:` path or a Git URL.
    pub version: String,
}

/// The project's editor version: `m_EditorVersion` and the changeset of
/// `m_EditorVersionWithRevision`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorVersion {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
}

/// `ProjectSettings/ProjectVersion.txt`.
pub fn parse_project_version(text: &str) -> Option<EditorVersion> {
    let mut out: Option<EditorVersion> = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "m_EditorVersion" if !value.is_empty() => {
                out.get_or_insert_with(Default::default).version = value.to_string();
            }
            "m_EditorVersionWithRevision" => {
                // `2022.3.10f1 (ff3792e53c62)`
                let (v, rev) = match value.split_once('(') {
                    Some((v, r)) => (v.trim(), Some(r.trim_end_matches(')').trim().to_string())),
                    None => (value, None),
                };
                let e = out.get_or_insert_with(Default::default);
                if e.version.is_empty() {
                    e.version = v.to_string();
                }
                e.revision = rev.filter(|r| !r.is_empty());
            }
            _ => {}
        }
    }
    out.filter(|e| !e.version.is_empty())
}

/// The `dependencies` of `Packages/manifest.json`, in file order.
pub fn parse_manifest(text: &str) -> Vec<Package> {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    v.get("dependencies")
        .and_then(Value::as_object)
        .map(|deps| {
            deps.iter()
                .map(|(name, version)| Package {
                    name: name.clone(),
                    version: version.as_str().unwrap_or_default().to_string(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `Library/EditorInstance.json`: the editor that has the project open.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorInstance {
    pub process_id: u32,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub app_path: String,
}

pub fn parse_editor_instance(text: &str) -> Option<EditorInstance> {
    let v: Value = serde_json::from_str(text).ok()?;
    let pid = v.get("process_id")?.as_u64()?;
    Some(EditorInstance {
        process_id: u32::try_from(pid).ok()?,
        version: v
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
        app_path: v
            .get("app_path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .into(),
    })
}

/// The Forge bridge package in the project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeInstall {
    /// `embedded` (in `Packages/dev.forge.unity`), `manifest` (a dependency) or `none`.
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The bundled package is newer than the installed one.
    #[serde(default)]
    pub update: bool,
}

/// What Forge knows about a Unity project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnityProject {
    /// The editor version the project was last saved with.
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    pub packages: Vec<Package>,
    /// The IDE packages of the manifest (`com.unity.ide.rider`, …).
    pub ide_packages: Vec<String>,
    /// The Unity Test Framework version, when the manifest has it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_framework: Option<String>,
    pub bridge: BridgeInstall,
    /// Solutions in the project folder (Unity's IDE packages write `<Project>.sln`).
    pub solutions: Vec<String>,
    /// C# project files in the project folder (`Assembly-CSharp.csproj`, one per asmdef).
    pub csprojects: Vec<String>,
}

impl UnityProject {
    pub fn has_package(&self, name: &str) -> bool {
        self.packages.iter().any(|p| p.name == name)
    }

    /// Whether project files must be generated before C# code insight works.
    pub fn needs_project_files(&self) -> bool {
        self.solutions.is_empty() || self.csprojects.is_empty()
    }
}

/// Read a Unity project. `read` returns a project file's text, `root_files` lists the file
/// names in the project folder. `None` when the folder is not a Unity project.
pub fn read_project(
    read: &dyn Fn(&str) -> Option<String>,
    root_files: &[String],
) -> Option<UnityProject> {
    let version = parse_project_version(&read(PROJECT_VERSION)?)?;
    let packages = read(MANIFEST)
        .map(|t| parse_manifest(&t))
        .unwrap_or_default();
    let ide_packages = IDE_PACKAGES
        .iter()
        .filter(|(id, _)| packages.iter().any(|p| p.name == *id))
        .map(|(id, _)| id.to_string())
        .collect();
    let test_framework = packages
        .iter()
        .find(|p| p.name == TEST_FRAMEWORK)
        .map(|p| p.version.clone());
    let embedded = read(&format!("Packages/{PACKAGE_NAME}/package.json"))
        .and_then(|t| package::manifest_version(&t));
    let bridge = match (embedded, packages.iter().find(|p| p.name == PACKAGE_NAME)) {
        (Some(v), _) => BridgeInstall {
            kind: "embedded".into(),
            update: package::is_older(&v),
            version: Some(v),
        },
        (None, Some(p)) => BridgeInstall {
            kind: "manifest".into(),
            version: Some(p.version.clone()),
            update: false,
        },
        (None, None) => BridgeInstall {
            kind: "none".into(),
            ..BridgeInstall::default()
        },
    };
    let mut solutions: Vec<String> = root_files
        .iter()
        .filter(|f| f.to_ascii_lowercase().ends_with(".sln"))
        .cloned()
        .collect();
    solutions.sort();
    let mut csprojects: Vec<String> = root_files
        .iter()
        .filter(|f| f.to_ascii_lowercase().ends_with(".csproj"))
        .cloned()
        .collect();
    csprojects.sort();
    Some(UnityProject {
        version: version.version,
        revision: version.revision,
        packages,
        ide_packages,
        test_framework,
        bridge,
        solutions,
        csprojects,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn files() -> HashMap<&'static str, String> {
        let mut m = HashMap::new();
        m.insert(
            PROJECT_VERSION,
            "m_EditorVersion: 2022.3.10f1\nm_EditorVersionWithRevision: 2022.3.10f1 (ff3792e53c62)\n"
                .to_string(),
        );
        m.insert(
            MANIFEST,
            r#"{
  "dependencies": {
    "com.unity.ide.rider": "3.0.24",
    "com.unity.ide.visualstudio": "2.0.20",
    "com.unity.test-framework": "1.1.33",
    "com.unity.textmeshpro": "3.0.6",
    "com.unity.modules.physics": "1.0.0"
  }
}"#
            .to_string(),
        );
        m
    }

    #[test]
    fn reads_versions_packages_and_project_files() {
        let f = files();
        let read = |p: &str| f.get(p).cloned();
        let roots = vec![
            "Shooter.sln".to_string(),
            "Assembly-CSharp.csproj".into(),
            "README.md".into(),
        ];
        let p = read_project(&read, &roots).unwrap();
        assert_eq!(p.version, "2022.3.10f1");
        assert_eq!(p.revision.as_deref(), Some("ff3792e53c62"));
        assert_eq!(p.packages.len(), 5);
        assert_eq!(
            p.ide_packages,
            vec!["com.unity.ide.visualstudio", "com.unity.ide.rider"]
        );
        assert_eq!(p.test_framework.as_deref(), Some("1.1.33"));
        assert_eq!(p.bridge.kind, "none");
        assert!(!p.needs_project_files());
        assert!(read_project(&read, &[]).unwrap().needs_project_files());
        assert!(read_project(&|_| None, &roots).is_none());
    }

    #[test]
    fn bridge_package_and_instances() {
        let mut f = files();
        f.insert(
            "Packages/dev.forge.unity/package.json",
            r#"{"name": "dev.forge.unity", "version": "0.9.0"}"#.into(),
        );
        let read = |p: &str| f.get(p).cloned();
        let p = read_project(&read, &[]).unwrap();
        assert_eq!(p.bridge.kind, "embedded");
        assert_eq!(p.bridge.version.as_deref(), Some("0.9.0"));
        assert!(p.bridge.update);

        let e = parse_editor_instance(
            r#"{"process_id": 4242, "version": "2022.3.10f1", "app_path": "/opt/Unity/Editor/Unity", "app_contents_path": "/opt/Unity/Editor/Data"}"#,
        )
        .unwrap();
        assert_eq!((e.process_id, e.version.as_str()), (4242, "2022.3.10f1"));
        assert!(parse_editor_instance("{}").is_none());
        assert_eq!(
            parse_project_version("m_EditorVersion: 6000.0.23f1\n").unwrap(),
            EditorVersion {
                version: "6000.0.23f1".into(),
                revision: None
            }
        );
        assert!(parse_manifest("not json").is_empty());
    }
}
