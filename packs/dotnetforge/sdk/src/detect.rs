//! The .NET host and SDKs on this machine: the `dotnet` of Settings | .NET, `DOTNET_ROOT`,
//! the PATH and the default install folders (`/usr/share/dotnet`, `/usr/lib/dotnet`,
//! `~/.dotnet`, `C:\Program Files\dotnet`, Homebrew). SDKs and runtimes come from
//! `dotnet --list-sdks` / `--list-runtimes` (run outside the project, so its `global.json`
//! does not interfere), or from the host's `sdk/` and `shared/` folders when that fails;
//! `global.json` decides which SDK the project uses.

use std::path::{Path, PathBuf};

use dotnetforge_lang_dotnet::sdk::{
    install_options, parse_list_runtimes, parse_list_sdks, select_sdk, GlobalJson, InstallOption,
    Runtime, Sdk,
};
use serde::{Deserialize, Serialize};

/// A `dotnet` host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DotnetHost {
    /// The executable.
    pub path: PathBuf,
    /// Its folder (`DOTNET_ROOT`), with `sdk/` and `shared/`.
    pub root: PathBuf,
    /// Where it was found: `Settings`, `DOTNET_ROOT`, `PATH`, a folder.
    pub source: String,
}

/// Where to look; [`DetectEnv::system`] fills it from the machine.
#[derive(Debug, Clone, Default)]
pub struct DetectEnv {
    pub path: Vec<PathBuf>,
    pub home: Option<PathBuf>,
    pub dotnet_root: Option<PathBuf>,
    pub windows: bool,
    pub folders: Vec<PathBuf>,
}

impl DetectEnv {
    pub fn system() -> Self {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let mut folders: Vec<PathBuf> = [
            "/usr/share/dotnet",
            "/usr/lib/dotnet",
            "/usr/lib64/dotnet",
            "/usr/local/share/dotnet",
            "/opt/dotnet",
            "/snap/dotnet-sdk/current",
            "/opt/homebrew/opt/dotnet/libexec",
            "/usr/local/opt/dotnet/libexec",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        for k in ["ProgramFiles", "ProgramW6432"] {
            if let Some(p) = var(k) {
                folders.push(p.join("dotnet"));
            }
        }
        DetectEnv {
            path: std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
            home: dirs::home_dir(),
            dotnet_root: var("DOTNET_ROOT"),
            windows: cfg!(windows),
            folders,
        }
    }

    fn exe(&self) -> &'static str {
        if self.windows {
            "dotnet.exe"
        } else {
            "dotnet"
        }
    }
}

fn host_at(exe: PathBuf, source: &str) -> Option<DotnetHost> {
    if !exe.is_file() {
        return None;
    }
    // `/usr/bin/dotnet` is often a link into the install folder.
    let real = std::fs::canonicalize(&exe).unwrap_or_else(|_| exe.clone());
    let root = real.parent()?.to_path_buf();
    Some(DotnetHost {
        path: exe,
        root,
        source: source.to_string(),
    })
}

/// Every host, the preferred first: the custom path, `DOTNET_ROOT`, the PATH, the folders
/// and `~/.dotnet` (the dotnet-install script's default).
pub fn find_hosts(env: &DetectEnv, custom: &str) -> Vec<DotnetHost> {
    let mut out: Vec<DotnetHost> = Vec::new();
    let mut add = |h: Option<DotnetHost>| {
        if let Some(h) = h {
            if !out.iter().any(|o| o.root == h.root) {
                out.push(h);
            }
        }
    };
    let custom = custom.trim();
    if !custom.is_empty() {
        let p = PathBuf::from(custom);
        add(if p.is_dir() {
            host_at(p.join(env.exe()), "Settings")
        } else {
            host_at(p, "Settings")
        });
    }
    if let Some(r) = &env.dotnet_root {
        add(host_at(r.join(env.exe()), "DOTNET_ROOT"));
    }
    for dir in &env.path {
        add(host_at(dir.join(env.exe()), "PATH"));
    }
    for f in &env.folders {
        add(host_at(f.join(env.exe()), &f.to_string_lossy()));
    }
    if let Some(home) = &env.home {
        add(host_at(home.join(".dotnet").join(env.exe()), "~/.dotnet"));
    }
    out
}

/// SDK folders of a host without running it (`<root>/sdk/<version>`).
pub fn sdks_in(root: &Path) -> Vec<Sdk> {
    let mut out: Vec<Sdk> = std::fs::read_dir(root.join("sdk"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("dotnet.dll").is_file() || e.path().join("Sdks").is_dir())
        .filter_map(|e| {
            let v = e.file_name().to_string_lossy().into_owned();
            v.chars().next().filter(char::is_ascii_digit).map(|_| Sdk {
                path: e.path().to_string_lossy().into_owned(),
                version: v,
            })
        })
        .collect();
    out.sort_by(|a, b| dotnetforge_lang_dotnet::sdk::compare_versions(&a.version, &b.version));
    out
}

/// Runtimes of a host without running it (`<root>/shared/<name>/<version>`).
pub fn runtimes_in(root: &Path) -> Vec<Runtime> {
    let mut out = Vec::new();
    for fw in std::fs::read_dir(root.join("shared"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let name = fw.file_name().to_string_lossy().into_owned();
        for v in std::fs::read_dir(fw.path()).into_iter().flatten().flatten() {
            if v.path().is_dir() {
                out.push(Runtime {
                    name: name.clone(),
                    version: v.file_name().to_string_lossy().into_owned(),
                    path: v.path().to_string_lossy().into_owned(),
                });
            }
        }
    }
    out.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| dotnetforge_lang_dotnet::sdk::compare_versions(&a.version, &b.version))
    });
    out
}

/// What Settings | .NET and the status bar show.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DotnetStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<DotnetHost>,
    pub hosts: Vec<DotnetHost>,
    pub sdks: Vec<Sdk>,
    pub runtimes: Vec<Runtime>,
    /// The SDK the project builds with (global.json and its roll-forward policy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdk: Option<String>,
    /// The SDK `global.json` asks for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_sdk: Option<String>,
    pub problems: Vec<String>,
    /// How to install an SDK on this platform.
    pub install: Vec<InstallOption>,
}

/// Run `program args` and return its stdout (`None` when it fails).
pub type Runner<'a> = &'a dyn Fn(&Path, &[&str]) -> Option<String>;

/// Detect the host, its SDKs and runtimes, and the SDK the project uses.
pub fn detect(
    env: &DetectEnv,
    custom: &str,
    global: Option<&GlobalJson>,
    os: &str,
    run: Runner<'_>,
) -> DotnetStatus {
    let hosts = find_hosts(env, custom);
    let mut status = DotnetStatus {
        install: install_options(os, ""),
        ..DotnetStatus::default()
    };
    if !custom.trim().is_empty() && hosts.first().map(|h| h.source.as_str()) != Some("Settings") {
        status.problems.push(format!(
            "The dotnet executable of Settings | .NET was not found: {custom}"
        ));
    }
    let Some(host) = hosts.first().cloned() else {
        status
            .problems
            .push("No .NET SDK was found. Install one, then detect again.".into());
        status.hosts = hosts;
        return status;
    };
    let sdks = run(&host.path, &["--list-sdks"])
        .map(|t| parse_list_sdks(&t))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| sdks_in(&host.root));
    let runtimes = run(&host.path, &["--list-runtimes"])
        .map(|t| parse_list_runtimes(&t))
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| runtimes_in(&host.root));
    status.requested_sdk = global.and_then(|g| g.version.clone());
    status.sdk = select_sdk(&sdks, global).map(|s| s.version.clone());
    if sdks.is_empty() {
        status.problems.push(format!(
            "{} has no SDK, only runtimes: building needs the .NET SDK.",
            host.path.display()
        ));
    } else if status.sdk.is_none() {
        if let Some(g) = global {
            status.problems.push(format!(
                "global.json asks for SDK {} (rollForward {}), which is not installed.",
                g.version.as_deref().unwrap_or("?"),
                g.roll_forward
            ));
            let channel = g
                .version
                .as_deref()
                .and_then(|v| {
                    let mut it = v.split('.');
                    Some(format!("{}.{}", it.next()?, it.next()?))
                })
                .unwrap_or_default();
            status.install = install_options(os, &channel);
        }
    }
    status.host = Some(host);
    status.hosts = hosts;
    status.sdks = sdks;
    status.runtimes = runtimes;
    status
}

/// Runs a program with a timeout, outside any project (the temp folder), without a shell.
pub fn system_runner(program: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(program)
        .args(args)
        .current_dir(std::env::temp_dir())
        .env("DOTNET_NOLOGO", "1")
        .env("DOTNET_CLI_TELEMETRY_OPTOUT", "1")
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_host(dir: &Path, sdks: &[&str]) -> PathBuf {
        let root = dir.join("dotnet");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("dotnet"), "#!/bin/sh\n").unwrap();
        for v in sdks {
            std::fs::create_dir_all(root.join("sdk").join(v)).unwrap();
            std::fs::write(root.join("sdk").join(v).join("dotnet.dll"), "").unwrap();
        }
        std::fs::create_dir_all(root.join("shared/Microsoft.NETCore.App/8.0.11")).unwrap();
        root
    }

    #[test]
    fn finds_hosts_and_sdks_without_running() {
        let dir = tempfile::tempdir().unwrap();
        let root = fake_host(dir.path(), &["8.0.404", "9.0.101"]);
        let env = DetectEnv {
            folders: vec![root.clone()],
            ..DetectEnv::default()
        };
        let hosts = find_hosts(&env, "");
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].source, root.to_string_lossy());
        let no_run = |_: &Path, _: &[&str]| None;
        let s = detect(&env, "", None, "linux", &no_run);
        assert_eq!(s.sdks.len(), 2);
        assert_eq!(s.sdk.as_deref(), Some("9.0.101"));
        assert_eq!(s.runtimes.len(), 1);
        assert!(s.problems.is_empty());
        // global.json pins a missing SDK.
        let g = GlobalJson {
            version: Some("10.0.100".into()),
            roll_forward: "latestFeature".into(),
            ..GlobalJson::default()
        };
        let s = detect(&env, "", Some(&g), "linux", &no_run);
        assert_eq!(s.sdk, None);
        assert!(s.problems[0].contains("10.0.100"));
        assert!(s.install[0].command.contains("dotnet-sdk-10.0"));
        // The custom path wins; a wrong one is reported.
        let custom = detect(
            &env,
            &root.join("dotnet").to_string_lossy(),
            None,
            "linux",
            &no_run,
        );
        assert_eq!(custom.host.unwrap().source, "Settings");
        let wrong = detect(&env, "/nope/dotnet", None, "linux", &no_run);
        assert!(wrong.problems[0].contains("/nope/dotnet"));
    }

    #[test]
    fn uses_list_sdks_when_it_runs() {
        let dir = tempfile::tempdir().unwrap();
        let root = fake_host(dir.path(), &[]);
        let env = DetectEnv {
            dotnet_root: Some(root.clone()),
            ..DetectEnv::default()
        };
        let run = |_: &Path, args: &[&str]| match args {
            ["--list-sdks"] => Some("8.0.110 [/x/sdk]\n8.0.404 [/x/sdk]\n".to_string()),
            ["--list-runtimes"] => {
                Some("Microsoft.NETCore.App 8.0.11 [/x/shared/Microsoft.NETCore.App]\n".to_string())
            }
            _ => None,
        };
        let s = detect(&env, "", None, "macos", &run);
        assert_eq!(s.host.as_ref().unwrap().source, "DOTNET_ROOT");
        assert_eq!(s.sdks.len(), 2);
        assert_eq!(s.sdk.as_deref(), Some("8.0.404"));
        // Runtimes only: building needs an SDK.
        let none = |_: &Path, _: &[&str]| None;
        let s = detect(&env, "", None, "macos", &none);
        assert!(s.problems[0].contains("no SDK"));
        // No host at all.
        let empty = detect(&DetectEnv::default(), "", None, "windows", &none);
        assert!(empty.host.is_none());
        assert!(empty.install[0].command.starts_with("winget"));
    }
}
