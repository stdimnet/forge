//! The Rust installation of this machine: rustup and its toolchains, the toolchain a project
//! folder uses (with its `rust-toolchain.toml`), standalone `rustc` installs, the sysroot
//! and the standard library's sources, installed components, tools and cargo plugins.
//!
//! Programs are started through a [`Runner`] (tests use recorded output). rustup runs with
//! `RUSTUP_AUTO_INSTALL=0`: a project's toolchain file must not make Forge install a
//! toolchain while it only looks.

use std::path::{Path, PathBuf};

use rustforge_lang_rust::toolchain::{
    parse_active_toolchain, parse_components, parse_rustc_version, parse_toolchain_list,
    ActiveToolchain, RustcVersion, Toolchain,
};
use rustforge_lang_rust::PrettyPrinters;
use serde::{Deserialize, Serialize};

/// Runs a program and returns its stdout when it exits successfully.
pub type Runner<'a> = dyn Fn(&Path, &[String], Option<&Path>) -> Option<String> + 'a;

/// The real runner: environment without auto-installs, stdin closed.
pub fn system_runner(program: &Path, args: &[String], cwd: Option<&Path>) -> Option<String> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args).env("RUSTUP_AUTO_INSTALL", "0");
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    // In Flatpak cargo / rustup run on the host.
    let mut cmd = forge_hostspawn::host_command(cmd);
    cmd.stdin(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Where to look.
#[derive(Debug, Clone, Default)]
pub struct RustEnv {
    pub path: Vec<PathBuf>,
    /// `CARGO_HOME` or `~/.cargo`.
    pub cargo_home: Option<PathBuf>,
    /// Forge's rust-analyzer downloads.
    pub forge_analyzer: Option<PathBuf>,
    pub windows: bool,
}

impl RustEnv {
    pub fn system() -> Self {
        let cargo_home = forge_hostspawn::env_var("CARGO_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".cargo")));
        RustEnv {
            path: forge_hostspawn::sandbox_path_var()
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
            cargo_home,
            forge_analyzer: crate::analyzer::default_dir(),
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

    /// A program in `CARGO_HOME/bin` or on the PATH.
    pub fn find(&self, name: &str) -> Option<PathBuf> {
        let file = self.exe(name);
        self.cargo_home
            .iter()
            .map(|h| h.join("bin"))
            .chain(self.path.iter().cloned())
            .map(|d| d.join(&file))
            .find(|p| p.is_file())
    }
}

/// A tool Forge uses and where it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    /// `rust-analyzer`, `rustfmt`, `clippy`, `cargo-nextest`, `cargo-llvm-cov`.
    pub tool: String,
    pub label: String,
    pub path: Option<String>,
    /// How Forge can install it: `component` (rustup) or `cargo` (cargo install).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<String>,
    pub licence: String,
}

/// The project's toolchain file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainFileInfo {
    /// Project relative.
    pub path: String,
    pub file: Option<rustforge_lang_rust::ToolchainFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RustStatus {
    pub rustup: Option<String>,
    pub cargo: Option<String>,
    pub toolchains: Vec<Toolchain>,
    /// The toolchain of the project folder and why.
    pub active: Option<ActiveToolchain>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolchain_file: Option<ToolchainFileInfo>,
    pub rustc: Option<RustcVersion>,
    pub sysroot: Option<String>,
    /// `<sysroot>/lib/rustlib/src/rust/library` (rust-src), for Go to Declaration into std.
    pub stdlib_sources: Option<String>,
    pub components: Vec<String>,
    pub tools: Vec<ToolStatus>,
    pub pretty_printers: Option<PrettyPrinters>,
}

impl RustStatus {
    pub fn tool(&self, id: &str) -> Option<&str> {
        self.tools
            .iter()
            .find(|t| t.tool == id)
            .and_then(|t| t.path.as_deref())
    }

    /// Whether the toolchain in use is a nightly.
    pub fn nightly(&self) -> bool {
        self.rustc.as_ref().is_some_and(|r| r.channel == "nightly")
    }
}

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Detect everything for a project folder. `toolchain` is Settings | Rust's choice (empty:
/// rustup's own, which honours `rust-toolchain.toml`).
pub fn detect(env: &RustEnv, project: Option<&Path>, toolchain: &str, run: &Runner) -> RustStatus {
    let rustup = env.find("rustup");
    let cargo = env.find("cargo");
    let rustc = env.find("rustc");
    let plus: Vec<String> = if toolchain.trim().is_empty() {
        Vec::new()
    } else {
        vec![format!("+{}", toolchain.trim().trim_start_matches('+'))]
    };
    let mut status = RustStatus {
        rustup: rustup.as_ref().map(|p| p.to_string_lossy().into_owned()),
        cargo: cargo.as_ref().map(|p| p.to_string_lossy().into_owned()),
        ..RustStatus::default()
    };
    if let Some(r) = &rustup {
        status.toolchains = run(r, &args(&["toolchain", "list", "-v"]), None)
            .map(|t| parse_toolchain_list(&t))
            .unwrap_or_default();
        status.active = if toolchain.trim().is_empty() {
            run(r, &args(&["show", "active-toolchain"]), project)
                .and_then(|t| parse_active_toolchain(&t))
        } else {
            Some(ActiveToolchain {
                name: toolchain.trim().to_string(),
                reason: rustforge_lang_rust::ActiveReason::Other {
                    text: "chosen in Settings | Rust".into(),
                },
            })
        };
        let active = status.active.as_ref().map(|a| a.name.clone());
        for t in &mut status.toolchains {
            t.active = active.as_deref() == Some(t.name.as_str())
                || active
                    .as_deref()
                    .is_some_and(|a| t.name.starts_with(&format!("{a}-")));
        }
    }
    if let Some(rc) = &rustc {
        let mut a = plus.clone();
        a.push("-vV".into());
        status.rustc = run(rc, &a, project).and_then(|t| parse_rustc_version(&t));
        let mut a = plus.clone();
        a.extend(args(&["--print", "sysroot"]));
        status.sysroot = run(rc, &a, project)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
    }
    if rustup.is_none() {
        // A standalone install (distribution packages, the official installers).
        if let (Some(rc), Some(v)) = (&rustc, &status.rustc) {
            status.toolchains.push(Toolchain {
                name: format!("{} ({})", v.release, v.host),
                channel: v.channel.clone(),
                host: v.host.clone(),
                path: rc
                    .parent()
                    .and_then(Path::parent)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                default: true,
                active: true,
                source: "path".into(),
            });
        }
    }
    if let Some(sysroot) = &status.sysroot {
        let root = Path::new(sysroot);
        let library = root.join("lib/rustlib/src/rust/library");
        if library.is_dir() {
            status.stdlib_sources = Some(library.to_string_lossy().into_owned());
        }
        let etc = root.join("lib/rustlib/etc");
        if etc.join("lldb_lookup.py").is_file()
            || etc.join("gdb_load_rust_pretty_printers.py").is_file()
        {
            status.pretty_printers = Some(PrettyPrinters {
                etc: etc.to_string_lossy().into_owned(),
                sources: if root.join("lib/rustlib/src/rust").is_dir() {
                    root.join("lib/rustlib/src/rust")
                        .to_string_lossy()
                        .into_owned()
                } else {
                    String::new()
                },
                commit_hash: status
                    .rustc
                    .as_ref()
                    .map(|r| r.commit_hash.clone())
                    .unwrap_or_default(),
            });
        }
    }
    let host = status
        .rustc
        .as_ref()
        .map(|r| r.host.clone())
        .unwrap_or_default();
    if let Some(r) = &rustup {
        let mut a = plus.clone();
        a.extend(args(&["component", "list", "--installed"]));
        // `rustup +tc component …` is not accepted: name the toolchain.
        let a = match status.active.as_ref() {
            Some(t) => args(&["component", "list", "--installed", "--toolchain"])
                .into_iter()
                .chain([t.name.clone()])
                .collect(),
            None => a,
        };
        status.components = run(r, &a, project)
            .map(|t| parse_components(&t, &host))
            .unwrap_or_default();
    }
    let which = |bin: &str| -> Option<String> {
        if let (Some(r), Some(t)) = (&rustup, status.active.as_ref()) {
            let a: Vec<String> = args(&["which", "--toolchain"])
                .into_iter()
                .chain([t.name.clone(), bin.to_string()])
                .collect();
            return run(r, &a, project)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
        }
        // Without rustup: next to rustc.
        rustc
            .as_ref()
            .and_then(|rc| rc.parent())
            .map(|d| d.join(env.exe(bin)))
            .filter(|p| p.is_file())
            .map(|p| p.to_string_lossy().into_owned())
    };
    let analyzer = which("rust-analyzer")
        .or_else(|| {
            env.forge_analyzer
                .as_deref()
                .and_then(crate::analyzer::installed)
                .map(|p| p.to_string_lossy().into_owned())
        })
        .or_else(|| {
            // A rust-analyzer on the PATH that is not rustup's proxy.
            env.path
                .iter()
                .map(|d| d.join(env.exe("rust-analyzer")))
                .find(|p| p.is_file() && !p.starts_with(env.cargo_home.clone().unwrap_or_default()))
                .map(|p| p.to_string_lossy().into_owned())
        });
    let plugin = |name: &str| env.find(name).map(|p| p.to_string_lossy().into_owned());
    status.tools = vec![
        ToolStatus {
            tool: "rust-analyzer".into(),
            label: "rust-analyzer".into(),
            path: analyzer,
            install: Some(
                if rustup.is_some() {
                    "component"
                } else {
                    "download"
                }
                .into(),
            ),
            licence: "MIT OR Apache-2.0".into(),
        },
        ToolStatus {
            tool: "rustfmt".into(),
            label: "rustfmt".into(),
            path: which("rustfmt"),
            install: rustup.is_some().then(|| "component".into()),
            licence: "MIT OR Apache-2.0".into(),
        },
        ToolStatus {
            tool: "clippy".into(),
            label: "Clippy".into(),
            path: which("cargo-clippy"),
            install: rustup.is_some().then(|| "component".into()),
            licence: "MIT OR Apache-2.0".into(),
        },
        ToolStatus {
            tool: "cargo-nextest".into(),
            label: "cargo-nextest".into(),
            path: plugin("cargo-nextest"),
            install: Some("cargo".into()),
            licence: "MIT OR Apache-2.0".into(),
        },
        ToolStatus {
            tool: "cargo-llvm-cov".into(),
            label: "cargo-llvm-cov".into(),
            path: plugin("cargo-llvm-cov"),
            install: Some("cargo".into()),
            licence: "MIT OR Apache-2.0".into(),
        },
    ];
    status
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
    }

    #[test]
    fn rustup_installs_with_recorded_output() {
        let home = tempfile::tempdir().unwrap();
        let cargo_home = home.path().join(".cargo");
        for bin in ["rustup", "cargo", "rustc", "cargo-nextest"] {
            touch(&cargo_home.join("bin").join(bin));
        }
        let sysroot = home
            .path()
            .join(".rustup/toolchains/nightly-2026-10-20-x86_64-unknown-linux-gnu");
        touch(&sysroot.join("lib/rustlib/etc/lldb_lookup.py"));
        touch(&sysroot.join("lib/rustlib/src/rust/library/core/src/lib.rs"));
        let env = RustEnv {
            path: vec![],
            cargo_home: Some(cargo_home.clone()),
            forge_analyzer: None,
            windows: false,
        };
        let replies: HashMap<&str, String> = HashMap::from([
            ("toolchain list -v", include_str!("../../lang-rust/tests/fixtures/rustup-toolchain-list.txt").to_string()),
            ("show active-toolchain", "nightly-2026-10-20-x86_64-unknown-linux-gnu (overridden by '/w/shop/rust-toolchain.toml')\n".to_string()),
            ("-vV", "rustc 1.99.0-nightly (abc 2026-10-19)\nrelease: 1.99.0-nightly\nhost: x86_64-unknown-linux-gnu\ncommit-hash: abc123\n".to_string()),
            ("--print sysroot", format!("{}\n", sysroot.display())),
            ("component list --installed --toolchain nightly-2026-10-20-x86_64-unknown-linux-gnu", include_str!("../../lang-rust/tests/fixtures/rustup-components.txt").to_string()),
            ("which --toolchain nightly-2026-10-20-x86_64-unknown-linux-gnu rustfmt", "/r/bin/rustfmt\n".to_string()),
            ("which --toolchain nightly-2026-10-20-x86_64-unknown-linux-gnu cargo-clippy", "/r/bin/cargo-clippy\n".to_string()),
        ]);
        let run =
            |_: &Path, a: &[String], _: Option<&Path>| replies.get(a.join(" ").as_str()).cloned();
        let s = detect(&env, Some(Path::new("/w/shop")), "", &run);
        assert_eq!(s.toolchains.len(), 5);
        let active: Vec<&str> = s
            .toolchains
            .iter()
            .filter(|t| t.active)
            .map(|t| t.name.as_str())
            .collect();
        assert_eq!(active, ["nightly-2026-10-20-x86_64-unknown-linux-gnu"]);
        assert!(s.nightly());
        assert_eq!(s.sysroot.as_deref(), Some(sysroot.to_str().unwrap()));
        assert!(s
            .stdlib_sources
            .as_deref()
            .unwrap()
            .ends_with("lib/rustlib/src/rust/library"));
        let pp = s.pretty_printers.as_ref().unwrap();
        assert_eq!(pp.commit_hash, "abc123");
        assert!(pp.sources.ends_with("lib/rustlib/src/rust"));
        assert!(s.components.contains(&"rust-src".to_string()));
        assert_eq!(s.tool("rustfmt"), Some("/r/bin/rustfmt"));
        assert_eq!(s.tool("rust-analyzer"), None);
        assert!(s
            .tool("cargo-nextest")
            .unwrap()
            .ends_with(".cargo/bin/cargo-nextest"));
        assert_eq!(s.tool("cargo-llvm-cov"), None);
        // A toolchain chosen in the settings.
        let chosen = detect(&env, None, "stable", &run);
        assert_eq!(chosen.active.unwrap().name, "stable");
        assert!(chosen
            .toolchains
            .iter()
            .any(|t| t.name.starts_with("stable-") && t.active));
    }

    #[test]
    fn standalone_rustc_without_rustup() {
        let dir = tempfile::tempdir().unwrap();
        touch(&dir.path().join("usr/bin/rustc"));
        touch(&dir.path().join("usr/bin/rustfmt"));
        let env = RustEnv {
            path: vec![dir.path().join("usr/bin")],
            cargo_home: None,
            forge_analyzer: None,
            windows: false,
        };
        let run = |_: &Path, a: &[String], _: Option<&Path>| {
            (a.join(" ") == "-vV")
                .then(|| "release: 1.85.0\nhost: aarch64-unknown-linux-gnu\n".to_string())
        };
        let s = detect(&env, None, "", &run);
        assert_eq!(s.toolchains.len(), 1);
        assert_eq!(s.toolchains[0].source, "path");
        assert_eq!(s.toolchains[0].name, "1.85.0 (aarch64-unknown-linux-gnu)");
        assert!(s.tool("rustfmt").unwrap().ends_with("usr/bin/rustfmt"));
        assert_eq!(s.tools[0].install.as_deref(), Some("download"));
    }

    /// Detection on this machine (needs rustup); prints what it finds.
    #[test]
    #[ignore]
    fn smoke_this_machine() {
        let s = detect(
            &RustEnv::system(),
            std::env::current_dir().ok().as_deref(),
            "",
            &system_runner,
        );
        println!("{s:#?}");
        assert!(s.cargo.is_some());
        assert!(!s.toolchains.is_empty());
    }
}
