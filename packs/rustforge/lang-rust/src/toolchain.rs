//! Toolchains as rustup and rustc describe them, parsed without running anything:
//!
//! - `rustup toolchain list -v`: installed toolchains, the default and the active one, with
//!   their folders,
//! - `rustup show active-toolchain`: the toolchain of a folder and why (default, a
//!   `rust-toolchain.toml` override, a directory override, `RUSTUP_TOOLCHAIN`),
//! - `rust-toolchain.toml` / `rust-toolchain`: channel, components, targets and profile,
//! - `rustc -vV`: release, channel, host and commit hash (for the standard library's sources
//!   and the debugger's `/rustc/<hash>` path mapping).

use serde::{Deserialize, Serialize};

/// A rustup toolchain (or a standalone one: `source` `path` / `custom`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Toolchain {
    /// `stable-x86_64-unknown-linux-gnu`, `nightly-2026-10-20-…`, `1.85.0-…`, `my-rust`.
    pub name: String,
    /// `stable`, `beta`, `nightly`, a version (`1.85.0`) or the custom name.
    pub channel: String,
    /// The host triple in the name (empty for custom toolchains).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub host: String,
    /// The folder (`~/.rustup/toolchains/<name>`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub active: bool,
    /// `rustup`, `path` (rustc on the PATH without rustup), `custom` (a folder).
    pub source: String,
}

impl Toolchain {
    pub fn is_nightly(&self) -> bool {
        self.channel == "nightly"
    }
}

/// Host triples are four (or three) dash-separated parts at the end of a name.
const ARCHS: &[&str] = &[
    "x86_64",
    "aarch64",
    "i686",
    "i586",
    "armv7",
    "arm",
    "riscv64gc",
    "powerpc64le",
    "s390x",
    "loongarch64",
    "wasm32",
];

/// Split `stable-x86_64-unknown-linux-gnu` into (`stable`, `x86_64-unknown-linux-gnu`) and
/// `nightly-2026-10-20-aarch64-apple-darwin` into (`nightly`, `aarch64-apple-darwin`).
pub fn split_name(name: &str) -> (String, String) {
    let parts: Vec<&str> = name.split('-').collect();
    let Some(at) = parts.iter().position(|p| ARCHS.contains(p)) else {
        return (name.to_string(), String::new());
    };
    if at == 0 {
        return (name.to_string(), String::new());
    }
    let host = parts[at..].join("-");
    let head = &parts[..at];
    let channel = match head[0] {
        "stable" | "beta" | "nightly" => head[0].to_string(),
        _ => head.join("-"),
    };
    (channel, host)
}

/// `rustup toolchain list -v` (or without `-v`).
pub fn parse_toolchain_list(text: &str) -> Vec<Toolchain> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("no installed toolchains"))
        .map(|line| {
            let (name, rest) = line.split_once(' ').unwrap_or((line, ""));
            let mut flags = "";
            let mut path = rest.trim();
            if let Some(r) = path.strip_prefix('(') {
                if let Some((f, p)) = r.split_once(')') {
                    flags = f;
                    path = p.trim();
                }
            }
            let has = |f: &str| flags.split(',').any(|x| x.trim() == f);
            let (channel, host) = split_name(name);
            Toolchain {
                name: name.to_string(),
                channel,
                host,
                path: path.to_string(),
                default: has("default"),
                active: has("active") || has("override"),
                source: "rustup".into(),
            }
        })
        .collect()
}

/// Why a toolchain is active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ActiveReason {
    /// rustup's default toolchain.
    Default,
    /// A `rust-toolchain.toml` / `rust-toolchain` file (absolute path as rustup prints it).
    File {
        path: String,
    },
    /// `rustup override set` for a folder.
    Directory {
        path: String,
    },
    /// `RUSTUP_TOOLCHAIN` or `+toolchain`.
    Environment,
    Other {
        text: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveToolchain {
    pub name: String,
    pub reason: ActiveReason,
}

/// `rustup show active-toolchain`: `stable-x86_64-unknown-linux-gnu (overridden by
/// '/w/shop/rust-toolchain.toml')`, `… (default)`, `… (directory override for '/w')`,
/// `… (environment override by RUSTUP_TOOLCHAIN)`; rustup 1.28 also says `active because:`.
pub fn parse_active_toolchain(text: &str) -> Option<ActiveToolchain> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    let (name, rest) = line.split_once(' ').unwrap_or((line, ""));
    let why = rest
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();
    let quoted = |s: &str| {
        s.split_once('\'')
            .and_then(|(_, r)| r.rsplit_once('\''))
            .map(|(p, _)| p.to_string())
    };
    let reason = if why.is_empty() || why == "default" || why.contains("default toolchain") {
        ActiveReason::Default
    } else if why.starts_with("overridden by") {
        match quoted(why) {
            Some(path) => ActiveReason::File { path },
            None => ActiveReason::Other { text: why.into() },
        }
    } else if why.starts_with("directory override") {
        ActiveReason::Directory {
            path: quoted(why).unwrap_or_default(),
        }
    } else if why.contains("environment") || why.contains("RUSTUP_TOOLCHAIN") {
        ActiveReason::Environment
    } else {
        ActiveReason::Other { text: why.into() }
    };
    Some(ActiveToolchain {
        name: name.to_string(),
        reason,
    })
}

/// A `rust-toolchain.toml` (or legacy `rust-toolchain`) override.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainFile {
    /// `stable`, `nightly-2026-10-20`, `1.85.0`; empty with only `path`.
    pub channel: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub profile: String,
    /// A custom toolchain folder (`path = "…"`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
}

#[derive(Deserialize, Default)]
struct RawFile {
    #[serde(default)]
    toolchain: RawSection,
}

#[derive(Deserialize, Default)]
struct RawSection {
    #[serde(default)]
    channel: String,
    #[serde(default)]
    components: Vec<String>,
    #[serde(default)]
    targets: Vec<String>,
    #[serde(default)]
    profile: String,
    #[serde(default)]
    path: String,
}

/// Parse a toolchain file: TOML with a `[toolchain]` table, or a legacy file whose only
/// line is the channel.
pub fn parse_toolchain_file(text: &str) -> Result<ToolchainFile, String> {
    let trimmed = text.trim();
    if !trimmed.contains('[') && !trimmed.contains('=') {
        let channel = trimmed.lines().next().unwrap_or("").trim().to_string();
        if channel.is_empty() {
            return Err("the toolchain file is empty".into());
        }
        return Ok(ToolchainFile {
            channel,
            ..ToolchainFile::default()
        });
    }
    let raw: RawFile = toml::from_str(text).map_err(|e| e.message().to_string())?;
    let t = raw.toolchain;
    if t.channel.is_empty() && t.path.is_empty() {
        return Err("[toolchain] needs a channel or a path".into());
    }
    Ok(ToolchainFile {
        channel: t.channel,
        components: t.components,
        targets: t.targets,
        profile: t.profile,
        path: t.path,
    })
}

/// `rustc -vV`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RustcVersion {
    /// `1.97.0`, `1.99.0-nightly`.
    pub release: String,
    /// `stable`, `beta` or `nightly`.
    pub channel: String,
    pub host: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commit_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub commit_date: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub llvm: String,
}

pub fn parse_rustc_version(text: &str) -> Option<RustcVersion> {
    let field = |key: &str| {
        text.lines()
            .find_map(|l| l.trim().strip_prefix(key).map(|v| v.trim().to_string()))
            .unwrap_or_default()
    };
    let release = field("release:");
    if release.is_empty() {
        return None;
    }
    let channel = if release.contains("nightly") || release.contains("-dev") {
        "nightly"
    } else if release.contains("beta") {
        "beta"
    } else {
        "stable"
    };
    Some(RustcVersion {
        channel: channel.into(),
        release,
        host: field("host:"),
        commit_hash: field("commit-hash:"),
        commit_date: field("commit-date:"),
        llvm: field("LLVM version:"),
    })
}

/// Installed components of `rustup component list --installed` (host suffixes removed):
/// `clippy-x86_64-unknown-linux-gnu` → `clippy`; `rust-std` entries keep their target.
pub fn parse_components(text: &str, host: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            let l = l.split_whitespace().next().unwrap_or(l);
            match l.strip_suffix(&format!("-{host}")) {
                Some(name) if !host.is_empty() => name.to_string(),
                _ => l.to_string(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolchain_lists() {
        let list =
            parse_toolchain_list(include_str!("../tests/fixtures/rustup-toolchain-list.txt"));
        let rows: Vec<(&str, &str, &str, bool, bool)> = list
            .iter()
            .map(|t| {
                (
                    t.name.as_str(),
                    t.channel.as_str(),
                    t.host.as_str(),
                    t.default,
                    t.active,
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "stable-x86_64-unknown-linux-gnu",
                    "stable",
                    "x86_64-unknown-linux-gnu",
                    true,
                    false
                ),
                (
                    "beta-x86_64-unknown-linux-gnu",
                    "beta",
                    "x86_64-unknown-linux-gnu",
                    false,
                    false
                ),
                (
                    "nightly-2026-10-20-x86_64-unknown-linux-gnu",
                    "nightly",
                    "x86_64-unknown-linux-gnu",
                    false,
                    true
                ),
                (
                    "1.85.0-x86_64-unknown-linux-gnu",
                    "1.85.0",
                    "x86_64-unknown-linux-gnu",
                    false,
                    false
                ),
                ("stage1", "stage1", "", false, false),
            ]
        );
        assert_eq!(list[4].path, "/home/anna/src/rust/build/host/stage1");
        assert!(list[2].is_nightly());
        // rustup 1.28 on this machine.
        let here = parse_toolchain_list(
            "stable-x86_64-unknown-linux-gnu (active, default) /root/.rustup/toolchains/stable-x86_64-unknown-linux-gnu\n",
        );
        assert!(here[0].active && here[0].default);
        assert_eq!(parse_toolchain_list("no installed toolchains\n"), []);
    }

    #[test]
    fn active_toolchains_and_reasons() {
        let a = parse_active_toolchain(
            "stable-x86_64-unknown-linux-gnu (overridden by '/home/anna/Projects/shop/rust-toolchain.toml')\n",
        )
        .unwrap();
        assert_eq!(
            a.reason,
            ActiveReason::File {
                path: "/home/anna/Projects/shop/rust-toolchain.toml".into()
            }
        );
        let d = parse_active_toolchain("nightly-x86_64-apple-darwin (default)").unwrap();
        assert_eq!(
            (d.name.as_str(), d.reason),
            ("nightly-x86_64-apple-darwin", ActiveReason::Default)
        );
        assert_eq!(
            parse_active_toolchain(
                "beta-x86_64-pc-windows-msvc (directory override for 'C:\\w\\shop')"
            )
            .unwrap()
            .reason,
            ActiveReason::Directory {
                path: "C:\\w\\shop".into()
            }
        );
        assert_eq!(
            parse_active_toolchain(
                "1.85.0-x86_64-unknown-linux-gnu (environment override by RUSTUP_TOOLCHAIN)"
            )
            .unwrap()
            .reason,
            ActiveReason::Environment
        );
        assert_eq!(parse_active_toolchain(""), None);
    }

    #[test]
    fn toolchain_files() {
        let t = parse_toolchain_file(
            "[toolchain]\nchannel = \"nightly-2026-10-20\"\ncomponents = [\"clippy\", \"rust-src\"]\ntargets = [\"wasm32-unknown-unknown\"]\nprofile = \"minimal\"\n",
        )
        .unwrap();
        assert_eq!(t.channel, "nightly-2026-10-20");
        assert_eq!(t.components, ["clippy", "rust-src"]);
        assert_eq!(t.targets, ["wasm32-unknown-unknown"]);
        assert_eq!(t.profile, "minimal");
        assert_eq!(parse_toolchain_file("1.85.0\n").unwrap().channel, "1.85.0");
        assert_eq!(
            parse_toolchain_file("[toolchain]\npath = \"/opt/rust\"\n")
                .unwrap()
                .path,
            "/opt/rust"
        );
        assert!(parse_toolchain_file("[toolchain]\n").is_err());
        assert!(parse_toolchain_file("").is_err());
        assert!(parse_toolchain_file("[toolchain\nchannel=").is_err());
    }

    #[test]
    fn rustc_versions_and_components() {
        let v = parse_rustc_version(include_str!("../tests/fixtures/rustc-vV.txt")).unwrap();
        assert_eq!(v.release, "1.99.1");
        assert_eq!(v.channel, "stable");
        assert_eq!(v.host, "x86_64-unknown-linux-gnu");
        assert_eq!(v.commit_hash, "58cf8f9c35b355199727627d07ae34a4e2846d67");
        let n = parse_rustc_version("rustc 1.99.0-nightly (abc 2026-10-20)\nrelease: 1.99.0-nightly\nhost: aarch64-apple-darwin\n").unwrap();
        assert_eq!(n.channel, "nightly");
        assert_eq!(parse_rustc_version("garbage"), None);
        let c = parse_components(
            include_str!("../tests/fixtures/rustup-components.txt"),
            "x86_64-unknown-linux-gnu",
        );
        assert!(c.contains(&"clippy".to_string()) && c.contains(&"rust-src".to_string()));
        assert!(c.contains(&"rust-std-wasm32-unknown-unknown".to_string()));
        assert!(!c.contains(&"rust-analyzer".to_string()));
    }

    #[test]
    fn names() {
        assert_eq!(
            split_name("nightly-2026-10-20-aarch64-apple-darwin"),
            ("nightly".into(), "aarch64-apple-darwin".into())
        );
        assert_eq!(split_name("my-rust"), ("my-rust".into(), "".into()));
    }
}
