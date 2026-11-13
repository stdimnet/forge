//! A Node.js runtime for npm language servers: a suitable one on the machine (>= 18), or the
//! official build from nodejs.org (or a mirror) downloaded into
//! `<data dir>/forge/runtimes/node-<version>`, verified against the release's
//! `SHASUMS256.txt`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::archive;
use super::http::Http;
use super::Event;

/// Oldest Node.js major version language servers are started with.
pub const MIN_NODE_MAJOR: u64 = 18;

pub const DEFAULT_MIRROR: &str = "https://nodejs.org/dist";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    TarGz,
    Zip,
}

/// A Node.js build for one platform.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistTarget {
    /// Platform part of the archive name: `linux-x64`, `darwin-arm64`, `win-x64`.
    pub platform: &'static str,
    /// Its name in the release index's `files` list: `linux-x64`, `osx-arm64-tar`, ...
    pub index_file: &'static str,
    pub archive: ArchiveKind,
    pub windows: bool,
}

impl DistTarget {
    /// The build for `os` / `arch` as Rust names them (`linux`, `macos`, `windows`;
    /// `x86_64`, `aarch64`).
    pub fn for_platform(os: &str, arch: &str) -> Option<Self> {
        let (platform, index_file, archive, windows) = match (os, arch) {
            ("linux", "x86_64") => ("linux-x64", "linux-x64", ArchiveKind::TarGz, false),
            ("linux", "aarch64") => ("linux-arm64", "linux-arm64", ArchiveKind::TarGz, false),
            ("macos", "x86_64") => ("darwin-x64", "osx-x64-tar", ArchiveKind::TarGz, false),
            ("macos", "aarch64") => ("darwin-arm64", "osx-arm64-tar", ArchiveKind::TarGz, false),
            ("windows", "x86_64") => ("win-x64", "win-x64-zip", ArchiveKind::Zip, true),
            ("windows", "aarch64") => ("win-arm64", "win-arm64-zip", ArchiveKind::Zip, true),
            _ => return None,
        };
        Some(DistTarget {
            platform,
            index_file,
            archive,
            windows,
        })
    }

    pub fn current() -> Option<Self> {
        Self::for_platform(std::env::consts::OS, std::env::consts::ARCH)
    }

    /// `node-v22.11.0-linux-x64.tar.gz`
    pub fn archive_name(&self, version: &str) -> String {
        let ext = match self.archive {
            ArchiveKind::TarGz => "tar.gz",
            ArchiveKind::Zip => "zip",
        };
        format!("node-{version}-{}.{ext}", self.platform)
    }
}

/// `v22.11.0` / `22.11.0` → `(22, 11, 0)`.
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches('v');
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.').map(|p| p.parse::<u64>());
    let major = parts.next()?.ok()?;
    let minor = parts.next().unwrap_or(Ok(0)).ok()?;
    let patch = parts.next().unwrap_or(Ok(0)).ok()?;
    Some((major, minor, patch))
}

/// One release in `<mirror>/index.json` (newest first).
#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub version: String,
    /// `false`, or the LTS code name.
    #[serde(default)]
    pub lts: serde_json::Value,
    #[serde(default)]
    pub files: Vec<String>,
}

/// The newest LTS release that has a build for `target`.
pub fn pick_lts<'a>(releases: &'a [Release], target: &DistTarget) -> Option<&'a Release> {
    releases.iter().find(|r| {
        r.lts.is_string()
            && r.files.iter().any(|f| f == target.index_file)
            && parse_version(&r.version).is_some_and(|v| v.0 >= MIN_NODE_MAJOR)
    })
}

/// The SHA-256 of `file` in a `SHASUMS256.txt` (`<hex>  <file name>` per line).
pub fn shasum_for(shasums: &str, file: &str) -> Option<String> {
    shasums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?;
        (name == file && hash.len() == 64).then(|| hash.to_ascii_lowercase())
    })
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The `node` executable inside an unpacked build.
pub fn node_executable(runtime: &Path, windows: bool) -> PathBuf {
    if windows {
        runtime.join("node.exe")
    } else {
        runtime.join("bin").join("node")
    }
}

/// npm's entry script inside an unpacked build (run as `node <script> …`).
pub fn npm_cli(runtime: &Path, windows: bool) -> PathBuf {
    let modules = if windows {
        runtime.join("node_modules")
    } else {
        runtime.join("lib").join("node_modules")
    };
    modules.join("npm").join("bin").join("npm-cli.js")
}

/// Unpacked builds in `runtimes` (`node-v*` directories with a `node` executable), newest
/// first.
pub fn installed_runtimes(runtimes: &Path, windows: bool) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(runtimes) else {
        return vec![];
    };
    let mut found: Vec<((u64, u64, u64), PathBuf)> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?;
            let version = parse_version(name.strip_prefix("node-")?)?;
            node_executable(&p, windows)
                .is_file()
                .then_some((version, p))
        })
        .collect();
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    found.into_iter().map(|(_, p)| p).collect()
}

/// The runtime directory a `node` executable belongs to, when it is one of Forge's.
pub fn runtime_of(node: &Path, runtimes: &Path) -> Option<PathBuf> {
    let rel = node.strip_prefix(runtimes).ok()?;
    rel.components()
        .next()
        .map(|c| runtimes.join(c.as_os_str()))
}

/// A `major.minor.patch` version.
pub type Version = (u64, u64, u64);

static VERSIONS: Mutex<Option<HashMap<PathBuf, Option<Version>>>> = Mutex::new(None);

/// `node --version` of an executable (cached per path for the life of the process).
pub fn node_version(node: &Path) -> Option<(u64, u64, u64)> {
    let mut cache = VERSIONS.lock().expect("node version cache");
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(v) = cache.get(node) {
        return *v;
    }
    let mut cmd = std::process::Command::new(node);
    cmd.arg("--version");
    let v = forge_hostspawn::host_command(cmd)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| parse_version(&String::from_utf8_lossy(&o.stdout)));
    cache.insert(node.to_path_buf(), v);
    v
}

/// Whether `node` is recent enough for the language servers.
pub fn is_suitable(node: &Path) -> bool {
    node_version(node).is_some_and(|v| v.0 >= MIN_NODE_MAJOR)
}

/// Download the newest LTS build for `target` from `mirror`, verify it and unpack it into
/// `runtimes/node-<version>`. Returns the `node` executable.
pub async fn download(
    http: &Http,
    mirror: &str,
    target: &DistTarget,
    runtimes: &Path,
    on: &mut (dyn FnMut(Event) + Send),
) -> Result<PathBuf, String> {
    let mirror = mirror.trim_end_matches('/');
    on(Event::step("Looking up the current Node.js LTS release…"));
    let releases: Vec<Release> = http.json(&format!("{mirror}/index.json")).await?;
    let release = pick_lts(&releases, target)
        .ok_or_else(|| format!("no Node.js LTS build for {}", target.platform))?;
    let version = release.version.clone();
    let file = target.archive_name(&version);
    let shasums = http
        .text(&format!("{mirror}/{version}/SHASUMS256.txt"))
        .await?;
    let expected = shasum_for(&shasums, &file)
        .ok_or_else(|| format!("{file} is not listed in SHASUMS256.txt"))?;

    on(Event::step(format!(
        "Downloading Node.js {version} ({})…",
        target.platform
    )));
    let label = format!("Downloading Node.js {version}");
    let data = http
        .bytes(&format!("{mirror}/{version}/{file}"), |done, total| {
            on(Event::progress(&label, done, total))
        })
        .await?;
    let actual = sha256_hex(&data);
    if actual != expected {
        return Err(format!(
            "checksum mismatch for {file}: expected SHA-256 {expected}, got {actual}. The download was discarded."
        ));
    }
    on(Event::step(format!("Verified SHA-256 of {file}.")));

    on(Event::step("Unpacking Node.js…"));
    let dest = runtimes.join(format!("node-{version}"));
    let staging = runtimes.join(format!(".node-{version}.partial"));
    let _ = std::fs::remove_dir_all(&staging);
    match target.archive {
        ArchiveKind::TarGz => archive::extract_tar_gz(&data, &staging, 1)?,
        ArchiveKind::Zip => archive::extract_zip(&data, &staging, 1)?,
    }
    let node = node_executable(&staging, target.windows);
    if !node.is_file() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!("{file} does not contain a node executable"));
    }
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&staging, &dest)
        .map_err(|e| format!("cannot move Node.js into {}: {e}", dest.display()))?;
    on(Event::step(format!(
        "Node.js {version} is ready in {}.",
        dest.display()
    )));
    Ok(node_executable(&dest, target.windows))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed::archive::tests::tar_gz;
    use crate::managed::testserver;

    fn linux() -> DistTarget {
        DistTarget::for_platform("linux", "x86_64").unwrap()
    }

    #[test]
    fn platforms_and_archive_names() {
        assert_eq!(
            linux().archive_name("v22.11.0"),
            "node-v22.11.0-linux-x64.tar.gz"
        );
        let mac = DistTarget::for_platform("macos", "aarch64").unwrap();
        assert_eq!(
            mac.archive_name("v22.11.0"),
            "node-v22.11.0-darwin-arm64.tar.gz"
        );
        assert_eq!(mac.index_file, "osx-arm64-tar");
        let win = DistTarget::for_platform("windows", "x86_64").unwrap();
        assert_eq!(win.archive_name("v22.11.0"), "node-v22.11.0-win-x64.zip");
        assert!(DistTarget::for_platform("freebsd", "x86_64").is_none());
    }

    #[test]
    fn picks_the_newest_lts_with_a_build() {
        let releases: Vec<Release> = serde_json::from_str(
            r#"[
              {"version":"v23.1.0","lts":false,"files":["linux-x64"]},
              {"version":"v22.11.0","lts":"Jod","files":["osx-arm64-tar"]},
              {"version":"v20.18.0","lts":"Iron","files":["linux-x64","win-x64-zip"]},
              {"version":"v16.20.2","lts":"Gallium","files":["linux-x64"]}
            ]"#,
        )
        .unwrap();
        assert_eq!(pick_lts(&releases, &linux()).unwrap().version, "v20.18.0");
        let mac = DistTarget::for_platform("macos", "aarch64").unwrap();
        assert_eq!(pick_lts(&releases, &mac).unwrap().version, "v22.11.0");
        assert!(pick_lts(&releases[3..], &linux()).is_none(), "too old");
    }

    #[test]
    fn versions_and_shasums() {
        assert_eq!(parse_version("v22.11.0\n"), Some((22, 11, 0)));
        assert_eq!(parse_version("1.12.6-beta.1"), Some((1, 12, 6)));
        assert_eq!(parse_version("x"), None);
        let sums = format!(
            "{}  node-v1-linux-x64.tar.gz\n{}  node-v1-win-x64.zip\n",
            "a".repeat(64),
            "B".repeat(64)
        );
        assert_eq!(
            shasum_for(&sums, "node-v1-win-x64.zip"),
            Some("b".repeat(64))
        );
        assert_eq!(shasum_for(&sums, "node-v1-darwin-x64.tar.gz"), None);
    }

    /// Serve a fake Node.js release; `tamper` corrupts the archive after hashing.
    async fn serve_node(tamper: bool) -> testserver::TestServer {
        let server = testserver::start().await;
        let archive = tar_gz(&[(
            "node-v20.18.0-linux-x64/bin/node",
            b"#!/bin/sh\necho v20.18.0\n",
            0o755,
        )]);
        let sums = format!("{}  node-v20.18.0-linux-x64.tar.gz\n", sha256_hex(&archive));
        let mut served = archive.clone();
        if tamper {
            let last = served.len() - 1;
            served[last] ^= 0xff;
        }
        server.routes.set(
            "/index.json",
            r#"[{"version":"v20.18.0","lts":"Iron","files":["linux-x64"]}]"#,
        );
        server
            .routes
            .set("/v20.18.0/SHASUMS256.txt", sums.into_bytes());
        server
            .routes
            .set("/v20.18.0/node-v20.18.0-linux-x64.tar.gz", served);
        server
    }

    #[tokio::test]
    async fn downloads_verifies_and_unpacks_node() {
        let server = serve_node(false).await;
        let tmp = tempfile::tempdir().unwrap();
        let runtimes = tmp.path().join("runtimes");
        let mut events = Vec::new();
        let node = download(
            &Http::direct().unwrap(),
            &format!("{}/", server.url),
            &linux(),
            &runtimes,
            &mut |e| events.push(e),
        )
        .await
        .unwrap();
        assert_eq!(node, runtimes.join("node-v20.18.0/bin/node"));
        assert!(node.is_file());
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::Progress { done, .. } if *done > 0)));
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::Step { message } if message.contains("Verified SHA-256"))));
        assert_eq!(
            installed_runtimes(&runtimes, false),
            vec![runtimes.join("node-v20.18.0")]
        );
        assert_eq!(
            runtime_of(&node, &runtimes),
            Some(runtimes.join("node-v20.18.0"))
        );
        #[cfg(unix)]
        assert_eq!(node_version(&node), Some((20, 18, 0)));
    }

    #[tokio::test]
    async fn a_tampered_download_is_rejected_and_nothing_is_installed() {
        let server = serve_node(true).await;
        let tmp = tempfile::tempdir().unwrap();
        let runtimes = tmp.path().join("runtimes");
        let err = download(
            &Http::direct().unwrap(),
            &server.url,
            &linux(),
            &runtimes,
            &mut |_| {},
        )
        .await
        .unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
        assert!(installed_runtimes(&runtimes, false).is_empty());
        assert!(!runtimes.join("node-v20.18.0").exists());
    }
}
