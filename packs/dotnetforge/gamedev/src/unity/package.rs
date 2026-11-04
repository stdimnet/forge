//! The Forge editor bridge package (`dev.forge.unity`, MIT), shipped with Forge as C#
//! source (`unity-package/dev.forge.unity`). Forge copies it into the project's
//! `Packages/` folder (an embedded package) only when the user agrees; Unity compiles it.
//! Nothing is downloaded.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PACKAGE_NAME: &str = "dev.forge.unity";
/// The version of the bundled package.
pub const PACKAGE_VERSION: &str = "1.0.0";
/// Where the package goes in a project.
pub const INSTALL_DIR: &str = "Packages/dev.forge.unity";

macro_rules! package_file {
    ($path:literal) => {
        (
            $path,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/unity-package/dev.forge.unity/",
                $path
            )),
        )
    };
}

/// The package's files: path inside the package → content.
pub const FILES: &[(&str, &str)] = &[
    package_file!("package.json"),
    package_file!("LICENSE.md"),
    package_file!("Editor/Forge.Unity.Editor.asmdef"),
    package_file!("Editor/ForgeBridge.cs"),
    package_file!("Editor/ForgeCodeEditor.cs"),
    package_file!("Editor/ForgeProjectFiles.cs"),
];

/// A file to write into the project (project-relative path).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageFile {
    pub path: String,
    pub content: String,
}

/// The files of an install or update, below [`INSTALL_DIR`].
pub fn install_plan() -> Vec<PackageFile> {
    FILES
        .iter()
        .map(|(p, c)| PackageFile {
            path: format!("{INSTALL_DIR}/{p}"),
            content: c.replace("\r\n", "\n"),
        })
        .collect()
}

/// The `version` of a `package.json`.
pub fn manifest_version(text: &str) -> Option<String> {
    let v: Value = serde_json::from_str(text).ok()?;
    if v.get("name").and_then(Value::as_str) != Some(PACKAGE_NAME) {
        return None;
    }
    v.get("version").and_then(Value::as_str).map(str::to_string)
}

fn semver(v: &str) -> Option<(u32, u32, u32)> {
    let mut it = v.trim().split(['.', '-', '+']);
    Some((
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next().unwrap_or("0").parse().ok()?,
    ))
}

/// Whether an installed version is older than the bundled one.
pub fn is_older(installed: &str) -> bool {
    match (semver(installed), semver(PACKAGE_VERSION)) {
        (Some(a), Some(b)) => a < b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_package_is_consistent() {
        let package = FILES.iter().find(|(p, _)| *p == "package.json").unwrap().1;
        assert_eq!(manifest_version(package).as_deref(), Some(PACKAGE_VERSION));
        let v: Value = serde_json::from_str(package).unwrap();
        assert_eq!(v["license"], "MIT");
        let asmdef: Value = serde_json::from_str(
            FILES
                .iter()
                .find(|(p, _)| p.ends_with(".asmdef"))
                .unwrap()
                .1,
        )
        .unwrap();
        assert_eq!(asmdef["includePlatforms"][0], "Editor");
        let bridge = FILES
            .iter()
            .find(|(p, _)| p.ends_with("ForgeBridge.cs"))
            .unwrap()
            .1;
        // The C# side and the Rust side agree on the protocol.
        assert!(bridge.contains(&format!(
            "public const int Protocol = {};",
            super::super::bridge::PROTOCOL
        )));
        assert!(bridge.contains(super::super::project::BRIDGE_INSTANCE));
        for cmd in [
            "play",
            "pause",
            "resume",
            "stop",
            "step",
            "refresh",
            "syncSolution",
        ] {
            assert!(bridge.contains(&format!("case \"{cmd}\":")), "{cmd}");
        }
        let plan = install_plan();
        assert_eq!(plan.len(), FILES.len());
        assert!(plan
            .iter()
            .all(|f| f.path.starts_with("Packages/dev.forge.unity/")));
        assert!(plan.iter().all(|f| !f.content.contains('\r')));
    }

    #[test]
    fn versions() {
        assert!(is_older("0.9.0"));
        assert!(!is_older(PACKAGE_VERSION));
        assert!(!is_older("2.0.0"));
        assert!(!is_older("file:../forge"));
        assert!(manifest_version(r#"{"name": "other", "version": "1.0.0"}"#).is_none());
    }
}
