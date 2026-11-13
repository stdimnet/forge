//! Go tools Forge uses — gopls (BSD-3), Delve (MIT), goimports (BSD-3), golangci-lint
//! (GPL-3, never installed by Forge, only found) and gofmt (part of the SDK) — where they
//! are, and the `go install <module>@<pinned version>` that installs them with the user's
//! Go toolchain (module proxy and checksum database) into Forge's tools folder.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::detect::version_tuple;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GoTool {
    Gopls,
    Dlv,
    Goimports,
    GolangciLint,
}

impl GoTool {
    pub const ALL: [GoTool; 4] = [
        GoTool::Gopls,
        GoTool::Dlv,
        GoTool::Goimports,
        GoTool::GolangciLint,
    ];

    pub fn binary(self) -> &'static str {
        match self {
            GoTool::Gopls => "gopls",
            GoTool::Dlv => "dlv",
            GoTool::Goimports => "goimports",
            GoTool::GolangciLint => "golangci-lint",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GoTool::Gopls => "gopls",
            GoTool::Dlv => "Delve",
            GoTool::Goimports => "goimports",
            GoTool::GolangciLint => "golangci-lint",
        }
    }

    pub fn licence(self) -> &'static str {
        match self {
            GoTool::Dlv => "MIT",
            GoTool::GolangciLint => "GPL-3.0",
            _ => "BSD-3-Clause",
        }
    }

    /// The setting with the user's executable.
    pub fn setting(self) -> &'static str {
        match self {
            GoTool::Gopls => "langservers.servers.gopls.path",
            GoTool::Dlv => "go.delve.path",
            GoTool::Goimports => "go.goimports.path",
            GoTool::GolangciLint => "go.golangciLint.path",
        }
    }

    /// The package `go install` builds and the version Forge pins for an SDK version;
    /// `None` for tools Forge does not install.
    pub fn install_target(self, go_version: &str) -> Option<String> {
        let (_, minor, _) = version_tuple(go_version);
        let (module, version) = match self {
            // gopls builds with the newest toolchain (GOTOOLCHAIN=auto fetches it).
            GoTool::Gopls if minor >= 25 => ("golang.org/x/tools/gopls", "v0.23.0"),
            GoTool::Gopls => ("golang.org/x/tools/gopls", "v0.20.0"),
            // Delve debugs programs of the Go versions it supports: pick by the SDK.
            GoTool::Dlv if minor >= 25 => ("github.com/go-delve/delve/cmd/dlv", "v1.27.2"),
            GoTool::Dlv => ("github.com/go-delve/delve/cmd/dlv", "v1.25.2"),
            GoTool::Goimports => ("golang.org/x/tools/cmd/goimports", "v0.30.0"),
            GoTool::GolangciLint => return None,
        };
        Some(format!("{module}@{version}"))
    }
}

/// `<data dir>/forge/go-tools/bin` (`GOBIN` of Forge's installs).
pub fn default_tools_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("go-tools").join("bin"))
}

/// Where tools are looked for, in this order.
#[derive(Debug, Clone, Default)]
pub struct ToolDirs {
    /// Forge's `GOBIN`.
    pub forge: Option<PathBuf>,
    /// `GOBIN`, then `<GOPATH>/bin` (`~/go/bin`).
    pub go_bins: Vec<PathBuf>,
    /// The SDK's `bin` (gofmt).
    pub sdk_bin: Option<PathBuf>,
    pub path: Vec<PathBuf>,
    pub windows: bool,
}

impl ToolDirs {
    pub fn system(gopath: Option<&str>, goroot: Option<&Path>) -> Self {
        let var = |k: &str| {
            forge_hostspawn::env_var(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let mut go_bins = Vec::new();
        go_bins.extend(var("GOBIN"));
        let gopaths: Vec<PathBuf> = match gopath.map(str::trim).filter(|g| !g.is_empty()) {
            Some(g) => std::env::split_paths(g).collect(),
            None => var("GOPATH")
                .map(|g| std::env::split_paths(&g).collect())
                .unwrap_or_else(|| dirs::home_dir().map(|h| h.join("go")).into_iter().collect()),
        };
        go_bins.extend(gopaths.into_iter().map(|g| g.join("bin")));
        ToolDirs {
            forge: default_tools_dir(),
            go_bins,
            sdk_bin: goroot.map(|r| r.join("bin")),
            path: forge_hostspawn::sandbox_path_var()
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
            windows: cfg!(windows),
        }
    }

    fn exe(&self, name: &str) -> String {
        if self.windows {
            format!("{name}.exe")
        } else {
            name.to_string()
        }
    }

    /// The tool's executable: the user's path when it exists, else Forge's folder, the Go
    /// bin folders, the SDK and the PATH.
    pub fn locate(&self, tool: GoTool, user: Option<&str>) -> Option<PathBuf> {
        self.locate_binary(tool.binary(), user)
    }

    pub fn locate_binary(&self, name: &str, user: Option<&str>) -> Option<PathBuf> {
        if let Some(u) = user.map(str::trim).filter(|u| !u.is_empty()) {
            let p = PathBuf::from(u);
            return p.is_file().then_some(p);
        }
        let exe = self.exe(name);
        self.forge
            .iter()
            .chain(self.go_bins.iter())
            .chain(self.sdk_bin.iter())
            .chain(self.path.iter())
            .map(|d| d.join(&exe))
            .find(|p| p.is_file())
    }
}

/// `go install <target>` with `GOBIN` set to Forge's tools folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// `golang.org/x/tools/gopls@v0.23.0`.
    pub target: String,
}

pub fn install_command(
    tool: GoTool,
    go: &str,
    go_version: &str,
    gobin: &Path,
) -> Option<InstallCommand> {
    let target = tool.install_target(go_version)?;
    Some(InstallCommand {
        program: go.to_string(),
        args: vec!["install".into(), target.clone()],
        env: vec![
            ("GOBIN".into(), gobin.to_string_lossy().into_owned()),
            // Fetch a newer toolchain when the tool needs one; never skip the checksum
            // database.
            ("GOTOOLCHAIN".into(), "auto".into()),
            ("GOFLAGS".into(), "-mod=mod".into()),
        ],
        target,
    })
}

/// What Settings | Go shows about a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub tool: GoTool,
    pub label: String,
    pub path: Option<PathBuf>,
    /// What "Install" would run (`None`: Forge does not install it).
    pub install: Option<String>,
    pub licence: String,
}

pub fn status(
    dirs: &ToolDirs,
    go_version: &str,
    user: impl Fn(GoTool) -> Option<String>,
) -> Vec<ToolStatus> {
    GoTool::ALL
        .iter()
        .map(|t| ToolStatus {
            tool: *t,
            label: t.label().into(),
            path: dirs.locate(*t, user(*t).as_deref()),
            install: t.install_target(go_version),
            licence: t.licence().into(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_versions_by_sdk() {
        assert_eq!(
            GoTool::Dlv.install_target("1.24.7").as_deref(),
            Some("github.com/go-delve/delve/cmd/dlv@v1.25.2")
        );
        assert_eq!(
            GoTool::Dlv.install_target("1.26.1").as_deref(),
            Some("github.com/go-delve/delve/cmd/dlv@v1.27.2")
        );
        assert!(GoTool::Gopls
            .install_target("1.25.0")
            .unwrap()
            .ends_with("@v0.23.0"));
        assert_eq!(GoTool::GolangciLint.install_target("1.25.0"), None);
        let c = install_command(
            GoTool::Gopls,
            "/sdk/bin/go",
            "1.24.7",
            Path::new("/data/go-tools/bin"),
        )
        .unwrap();
        assert_eq!(c.args, ["install", "golang.org/x/tools/gopls@v0.20.0"]);
        assert!(c
            .env
            .contains(&("GOBIN".into(), "/data/go-tools/bin".into())));
    }

    #[test]
    fn locates_tools_in_order() {
        let t = tempfile::tempdir().unwrap();
        let mk = |p: &str| {
            let f = t.path().join(p);
            std::fs::create_dir_all(f.parent().unwrap()).unwrap();
            std::fs::write(&f, "").unwrap();
            f
        };
        let gopath_dlv = mk("gopath/bin/dlv");
        let forge_gopls = mk("forge/gopls");
        let path_gopls = mk("usr/bin/gopls");
        let sdk_gofmt = mk("sdk/bin/gofmt");
        let dirs = ToolDirs {
            forge: Some(t.path().join("forge")),
            go_bins: vec![t.path().join("gopath/bin")],
            sdk_bin: Some(t.path().join("sdk/bin")),
            path: vec![t.path().join("usr/bin")],
            windows: false,
        };
        assert_eq!(dirs.locate(GoTool::Gopls, None), Some(forge_gopls));
        assert_eq!(dirs.locate(GoTool::Dlv, Some("  ")), Some(gopath_dlv));
        assert_eq!(dirs.locate_binary("gofmt", None), Some(sdk_gofmt));
        assert_eq!(dirs.locate(GoTool::Goimports, None), None);
        assert_eq!(
            dirs.locate(GoTool::Gopls, Some(&path_gopls.to_string_lossy())),
            Some(path_gopls.clone())
        );
        assert_eq!(dirs.locate(GoTool::Gopls, Some("/nope/gopls")), None);
        let s = status(&dirs, "1.25.0", |_| None);
        assert_eq!(s.len(), 4);
        assert!(s[3].install.is_none() && s[3].licence == "GPL-3.0");
    }
}
