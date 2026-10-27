//! Where DotNetForge's external tools are: netcoredbg (Settings | .NET, Forge's download,
//! the PATH) and the Roslyn language server (Settings | .NET, Forge's download).

use std::path::{Path, PathBuf};

use forge_langservers::managed::netcoredbg::installed_netcoredbg;
use forge_langservers::managed::roslyn::{inspect_roslyn, installed_roslyn};
use serde::{Deserialize, Serialize};

/// A located tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolLocation {
    pub path: PathBuf,
    /// `setting`, `managed` or `path`.
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
}

/// netcoredbg: the setting (a file or its folder), Forge's download, the PATH.
pub fn find_netcoredbg(
    custom: &str,
    managed: Option<&Path>,
    on_path: &dyn Fn(&str) -> Option<PathBuf>,
) -> Result<ToolLocation, String> {
    let custom = custom.trim();
    if !custom.is_empty() {
        let p = PathBuf::from(custom);
        let p = if p.is_dir() {
            ["netcoredbg", "netcoredbg.exe"]
                .iter()
                .map(|n| p.join(n))
                .find(|x| x.is_file())
                .unwrap_or(p)
        } else {
            p
        };
        return if p.is_file() {
            Ok(ToolLocation {
                path: p,
                source: "setting".into(),
                version: String::new(),
            })
        } else {
            Err(format!("netcoredbg was not found at {custom}"))
        };
    }
    if let Some(i) = managed.and_then(installed_netcoredbg) {
        return Ok(ToolLocation {
            path: i.binary,
            source: "managed".into(),
            version: i.version,
        });
    }
    on_path("netcoredbg")
        .map(|p| ToolLocation {
            path: p,
            source: "path".into(),
            version: String::new(),
        })
        .ok_or_else(|| "netcoredbg is not installed".into())
}

/// The Roslyn server assembly: the setting (folder or dll), else Forge's download.
pub fn find_roslyn(custom: &str, managed: Option<&Path>) -> Result<ToolLocation, String> {
    let custom = custom.trim();
    if !custom.is_empty() {
        return inspect_roslyn(Path::new(custom))
            .map(|i| ToolLocation {
                path: i.dll,
                source: "setting".into(),
                version: i.version,
            })
            .ok_or_else(|| format!("Microsoft.CodeAnalysis.LanguageServer.dll was not found in {custom}"));
    }
    managed
        .and_then(installed_roslyn)
        .map(|i| ToolLocation {
            path: i.dll,
            source: "managed".into(),
            version: i.version,
        })
        .ok_or_else(|| "The Roslyn language server is not installed".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locations() {
        let dir = tempfile::tempdir().unwrap();
        let none = |_: &str| None;
        assert!(find_netcoredbg("", Some(dir.path()), &none).is_err());
        let on_path = |n: &str| Some(PathBuf::from(format!("/usr/bin/{n}")));
        assert_eq!(find_netcoredbg("", None, &on_path).unwrap().source, "path");
        let custom = dir.path().join("netcoredbg");
        std::fs::write(&custom, "").unwrap();
        let found = find_netcoredbg(&dir.path().to_string_lossy(), None, &none).unwrap();
        assert_eq!((found.path, found.source.as_str()), (custom, "setting"));
        assert!(find_netcoredbg("/nope", None, &on_path).is_err());

        assert!(find_roslyn("", Some(dir.path())).is_err());
        std::fs::write(dir.path().join("Microsoft.CodeAnalysis.LanguageServer.dll"), "").unwrap();
        let r = find_roslyn(&dir.path().to_string_lossy(), None).unwrap();
        assert_eq!(r.source, "setting");
        assert!(find_roslyn("/nope", None).is_err());
    }
}
