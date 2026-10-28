//! DotNetForge's netcoredbg download, with the user's consent: Samsung's .NET debugger
//! (MIT, a Debug Adapter Protocol server for .NET Core) from its official GitHub releases
//! (`github.com/Samsung/netcoredbg`) at a pinned version, unpacked into
//! `<data dir>/forge/debuggers/netcoredbg/<version>`.
//!
//! Integrity: every download is checked against a SHA-256 before it is unpacked. The
//! releases publish no checksum files, so the hash comes from [`NETCOREDBG_BUILDS`] when it
//! is recorded there, otherwise from the `digest` (`sha256:…`) the GitHub REST API reports
//! for the pinned release's asset. A download without either is refused.
//!
//! The pinned version's SHA-256 values could not be recorded while this module was written
//! (github.com was not reachable from the build machine); record them in
//! [`NETCOREDBG_BUILDS`] when bumping [`NETCOREDBG_VERSION`].

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::clangd::asset_digest;
use super::node::sha256_hex;
use super::{archive, Event, Http};

/// The pinned netcoredbg release.
pub const NETCOREDBG_VERSION: &str = "3.1.2-1054";
const RELEASES: &str = "https://github.com/Samsung/netcoredbg/releases/download";
const API: &str = "https://api.github.com/repos/Samsung/netcoredbg/releases/tags";

/// `(os, arch, asset, SHA-256 when recorded)` of the pinned release (macOS builds are
/// x86_64; Apple silicon runs them through Rosetta 2).
pub const NETCOREDBG_BUILDS: &[(&str, &str, &str, Option<&str>)] = &[
    ("linux", "x86_64", "netcoredbg-linux-amd64.tar.gz", None),
    ("linux", "aarch64", "netcoredbg-linux-arm64.tar.gz", None),
    ("macos", "x86_64", "netcoredbg-osx-amd64.tar.gz", None),
    ("macos", "aarch64", "netcoredbg-osx-amd64.tar.gz", None),
    ("windows", "x86_64", "netcoredbg-win64.zip", None),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetcoredbgDownload {
    pub version: String,
    pub asset: String,
    pub url: String,
    pub sha256: Option<String>,
    pub api_url: String,
}

/// The pinned build for a platform (`std::env::consts` names).
pub fn netcoredbg_release(os: &str, arch: &str) -> Option<NetcoredbgDownload> {
    let (_, _, asset, sha) = NETCOREDBG_BUILDS
        .iter()
        .find(|(o, a, ..)| *o == os && *a == arch)?;
    Some(NetcoredbgDownload {
        version: NETCOREDBG_VERSION.into(),
        url: format!("{RELEASES}/{NETCOREDBG_VERSION}/{asset}"),
        asset: asset.to_string(),
        sha256: sha.map(str::to_string),
        api_url: format!("{API}/{NETCOREDBG_VERSION}"),
    })
}

pub fn current_netcoredbg_release() -> Option<NetcoredbgDownload> {
    netcoredbg_release(std::env::consts::OS, std::env::consts::ARCH)
}

/// `<data dir>/forge/debuggers/netcoredbg`.
pub fn netcoredbg_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("debuggers").join("netcoredbg"))
}

const COMPLETE: &str = ".forge-complete";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetcoredbgInstallation {
    pub dir: PathBuf,
    pub binary: PathBuf,
    pub version: String,
}

fn binary_in(dir: &Path) -> Option<PathBuf> {
    ["netcoredbg", "netcoredbg.exe"]
        .iter()
        .map(|b| dir.join(b))
        .find(|p| p.is_file())
}

/// The netcoredbg Forge installed (the pinned version, else the newest complete one).
pub fn installed_netcoredbg(base: &Path) -> Option<NetcoredbgInstallation> {
    let pinned = base.join(NETCOREDBG_VERSION);
    let dir = if pinned.join(COMPLETE).is_file() {
        pinned
    } else {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(base)
            .ok()?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.join(COMPLETE).is_file())
            .collect();
        dirs.sort();
        dirs.pop()?
    };
    Some(NetcoredbgInstallation {
        binary: binary_in(&dir)?,
        version: dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        dir,
    })
}

/// Verify a downloaded archive against `sha256` and unpack it into `base/<version>`
/// (the archive's `netcoredbg/` folder is stripped).
pub fn install_netcoredbg_archive(
    data: &[u8],
    base: &Path,
    d: &NetcoredbgDownload,
    sha256: &str,
) -> Result<NetcoredbgInstallation, String> {
    let actual = sha256_hex(data);
    if !actual.eq_ignore_ascii_case(sha256.trim()) {
        return Err(format!(
            "checksum mismatch for netcoredbg {}: expected SHA-256 {sha256}, got {actual}. The download was discarded.",
            d.version
        ));
    }
    std::fs::create_dir_all(base).map_err(|e| format!("cannot create {}: {e}", base.display()))?;
    let dest = base.join(&d.version);
    let staging = base.join(format!(".{}.partial", d.version));
    let _ = std::fs::remove_dir_all(&staging);
    if d.asset.ends_with(".zip") {
        archive::extract_zip(data, &staging, 1)?;
    } else {
        archive::extract_tar_gz(data, &staging, 1)?;
    }
    let Some(binary) = binary_in(&staging) else {
        let _ = std::fs::remove_dir_all(&staging);
        return Err("the netcoredbg archive has no netcoredbg executable".into());
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&binary) {
            let mut perm = meta.permissions();
            perm.set_mode(perm.mode() | 0o755);
            let _ = std::fs::set_permissions(&binary, perm);
        }
    }
    #[cfg(not(unix))]
    let _ = &binary;
    std::fs::write(staging.join(COMPLETE), &d.version)
        .map_err(|e| format!("cannot write into {}: {e}", staging.display()))?;
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&staging, &dest)
        .map_err(|e| format!("cannot move netcoredbg into {}: {e}", dest.display()))?;
    installed_netcoredbg(base)
        .filter(|i| i.version == d.version)
        .ok_or_else(|| "netcoredbg was unpacked without its executable".into())
}

/// Download, verify and unpack the pinned netcoredbg unless it is installed.
pub async fn install_netcoredbg(
    http: &Http,
    base: &Path,
    d: &NetcoredbgDownload,
    on: &mut (dyn FnMut(Event) + Send),
) -> Result<NetcoredbgInstallation, String> {
    if let Some(i) = installed_netcoredbg(base).filter(|i| i.version == d.version) {
        on(Event::step(format!(
            "netcoredbg {} is already installed.",
            d.version
        )));
        return Ok(i);
    }
    let sha256 = match &d.sha256 {
        Some(s) => s.clone(),
        None => {
            on(Event::step(format!(
                "Reading the SHA-256 of {} from {}…",
                d.asset, d.api_url
            )));
            let json = http.text(&d.api_url).await?;
            asset_digest(&json, &d.asset).ok_or_else(|| {
                format!(
                    "GitHub reports no SHA-256 for {}: Forge does not install unverified downloads. Install netcoredbg yourself and set its path in Settings | Languages & Frameworks | .NET.",
                    d.asset
                )
            })?
        }
    };
    on(Event::step(format!("Downloading {}…", d.url)));
    let label = format!("Downloading netcoredbg {}", d.version);
    let data = http
        .bytes(&d.url, |done, total| {
            on(Event::progress(&label, done, total))
        })
        .await?;
    on(Event::step("Unpacking netcoredbg…"));
    let inst = install_netcoredbg_archive(&data, base, d, &sha256)?;
    on(Event::step(format!(
        "Verified SHA-256 {sha256} of netcoredbg {}.",
        d.version
    )));
    Ok(inst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed::archive::tests::tar_gz;
    use crate::managed::testserver;

    fn archive() -> Vec<u8> {
        tar_gz(&[
            (
                "netcoredbg/netcoredbg",
                b"#!/bin/sh\necho NET Core debugger 3.1.2\n".as_slice(),
                0o755,
            ),
            ("netcoredbg/libdbgshim.so", b"elf".as_slice(), 0o644),
        ])
    }

    #[test]
    fn pinned_builds() {
        let d = netcoredbg_release("linux", "x86_64").unwrap();
        assert_eq!(
            d.url,
            "https://github.com/Samsung/netcoredbg/releases/download/3.1.2-1054/netcoredbg-linux-amd64.tar.gz"
        );
        assert_eq!(
            d.api_url,
            "https://api.github.com/repos/Samsung/netcoredbg/releases/tags/3.1.2-1054"
        );
        assert_eq!(
            netcoredbg_release("windows", "x86_64").unwrap().asset,
            "netcoredbg-win64.zip"
        );
        assert!(netcoredbg_release("windows", "aarch64").is_none());
    }

    #[test]
    fn verifies_and_unpacks() {
        let dir = tempfile::tempdir().unwrap();
        let data = archive();
        let d = netcoredbg_release("linux", "x86_64").unwrap();
        assert!(
            install_netcoredbg_archive(&data, dir.path(), &d, &"00".repeat(32))
                .unwrap_err()
                .contains("checksum mismatch")
        );
        let inst = install_netcoredbg_archive(&data, dir.path(), &d, &sha256_hex(&data)).unwrap();
        assert_eq!(inst.binary, dir.path().join("3.1.2-1054/netcoredbg"));
        assert!(dir.path().join("3.1.2-1054/libdbgshim.so").is_file());
        assert_eq!(installed_netcoredbg(dir.path()), Some(inst));
    }

    #[tokio::test]
    async fn downloads_with_the_api_digest_or_refuses() {
        let server = testserver::start().await;
        let data = archive();
        let mut d = netcoredbg_release("linux", "x86_64").unwrap();
        d.url = format!("{}/download/{}", server.url, d.asset);
        d.api_url = format!("{}/release", server.url);
        server
            .routes
            .set(&format!("/download/{}", d.asset), data.clone());
        server.routes.set(
            "/release",
            format!(
                r#"{{"assets": [{{"name": "{}", "digest": "sha256:{}"}}]}}"#,
                d.asset,
                sha256_hex(&data)
            ),
        );
        let dir = tempfile::tempdir().unwrap();
        let http = Http::direct().unwrap();
        let inst = install_netcoredbg(&http, dir.path(), &d, &mut |_| {})
            .await
            .unwrap();
        assert!(inst.binary.is_file());
        let other = tempfile::tempdir().unwrap();
        server.routes.set("/release", r#"{"assets": []}"#);
        let err = install_netcoredbg(&http, other.path(), &d, &mut |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("no SHA-256"), "{err}");
        assert_eq!(server.routes.hits(&format!("/download/{}", d.asset)), 1);
    }
}
