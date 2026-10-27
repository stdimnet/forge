//! CForge's clangd download, with the user's consent, from the official clangd releases on
//! GitHub (`github.com/clangd/clangd`, Apache-2.0 WITH LLVM-exception), at a pinned version,
//! unpacked into `<data dir>/forge/langservers/clangd/<version>`.
//!
//! Integrity: every download is checked against a SHA-256 before it is unpacked. The
//! release does not publish checksum files, so the hash comes from [`CLANGD_BUILDS`] when it
//! is recorded there, otherwise from the `digest` (`sha256:…`) that the GitHub REST API
//! reports for the pinned release's asset. A download without either is refused.
//!
//! The pinned version's SHA-256 values could not be recorded while this module was written
//! (github.com was not reachable from the build machine); record them in [`CLANGD_BUILDS`]
//! when bumping [`CLANGD_VERSION`].

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use super::node::sha256_hex;
use super::{archive, Event, Http};

/// The pinned clangd release.
pub const CLANGD_VERSION: &str = "19.1.2";
const RELEASES: &str = "https://github.com/clangd/clangd/releases/download";
const API: &str = "https://api.github.com/repos/clangd/clangd/releases/tags";

/// `(os, arch, asset platform, SHA-256 when recorded)` of the pinned release. The macOS
/// build is universal (x86_64 and arm64); Linux and Windows builds are x86_64.
pub const CLANGD_BUILDS: &[(&str, &str, &str, Option<&str>)] = &[
    ("linux", "x86_64", "linux", None),
    ("macos", "x86_64", "mac", None),
    ("macos", "aarch64", "mac", None),
    ("windows", "x86_64", "windows", None),
];

/// A pinned download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClangdDownload {
    pub version: String,
    /// `clangd-linux-19.1.2.zip`.
    pub asset: String,
    pub url: String,
    /// The recorded SHA-256, if any (else the GitHub API's digest is used).
    pub sha256: Option<String>,
    /// Where the release metadata with the asset digests is.
    pub api_url: String,
}

/// The pinned build for a platform (`std::env::consts` names).
pub fn clangd_release(os: &str, arch: &str) -> Option<ClangdDownload> {
    let (_, _, platform, sha) = CLANGD_BUILDS
        .iter()
        .find(|(o, a, ..)| *o == os && *a == arch)?;
    let asset = format!("clangd-{platform}-{CLANGD_VERSION}.zip");
    Some(ClangdDownload {
        version: CLANGD_VERSION.into(),
        url: format!("{RELEASES}/{CLANGD_VERSION}/{asset}"),
        asset,
        sha256: sha.map(str::to_string),
        api_url: format!("{API}/{CLANGD_VERSION}"),
    })
}

/// The pinned build for this machine.
pub fn current_clangd_release() -> Option<ClangdDownload> {
    clangd_release(std::env::consts::OS, std::env::consts::ARCH)
}

/// The SHA-256 the GitHub REST API reports for `asset` of a release (`"digest":
/// "sha256:<hex>"`).
pub fn asset_digest(release_json: &str, asset: &str) -> Option<String> {
    let v: Value = serde_json::from_str(release_json).ok()?;
    v.get("assets")?
        .as_array()?
        .iter()
        .find(|a| a.get("name").and_then(Value::as_str) == Some(asset))?
        .get("digest")?
        .as_str()?
        .strip_prefix("sha256:")
        .filter(|h| h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_ascii_lowercase)
}

/// `<data dir>/forge/langservers/clangd`.
pub fn clangd_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("langservers").join("clangd"))
}

const COMPLETE: &str = ".forge-complete";

/// An unpacked clangd.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClangdInstallation {
    pub dir: PathBuf,
    pub binary: PathBuf,
    pub version: String,
}

fn binary_in(dir: &Path) -> Option<PathBuf> {
    ["bin/clangd", "bin/clangd.exe"]
        .iter()
        .map(|b| dir.join(b))
        .find(|p| p.is_file())
}

/// The clangd Forge installed (the pinned version, else the newest complete one).
pub fn installed_clangd(base: &Path) -> Option<ClangdInstallation> {
    let pinned = base.join(CLANGD_VERSION);
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
    Some(ClangdInstallation {
        binary: binary_in(&dir)?,
        version: dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        dir,
    })
}

/// Verify a downloaded release archive against `sha256` and unpack it into
/// `base/<version>` (the archive's `clangd_<version>/` folder is stripped).
pub fn install_clangd_archive(
    data: &[u8],
    base: &Path,
    d: &ClangdDownload,
    sha256: &str,
) -> Result<ClangdInstallation, String> {
    let actual = sha256_hex(data);
    if !actual.eq_ignore_ascii_case(sha256.trim()) {
        return Err(format!(
            "checksum mismatch for clangd {}: expected SHA-256 {sha256}, got {actual}. The download was discarded.",
            d.version
        ));
    }
    std::fs::create_dir_all(base).map_err(|e| format!("cannot create {}: {e}", base.display()))?;
    let dest = base.join(&d.version);
    let staging = base.join(format!(".{}.partial", d.version));
    let _ = std::fs::remove_dir_all(&staging);
    archive::extract_zip(data, &staging, 1)?;
    let Some(binary) = binary_in(&staging) else {
        let _ = std::fs::remove_dir_all(&staging);
        return Err("the clangd archive has no bin/clangd".into());
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
        .map_err(|e| format!("cannot move clangd into {}: {e}", dest.display()))?;
    installed_clangd(base)
        .filter(|i| i.version == d.version)
        .ok_or_else(|| "clangd was unpacked without its binary".into())
}

/// Download, verify and unpack the pinned clangd unless it is installed. `d` is
/// [`current_clangd_release`] (tests point it at a local server).
pub async fn install_clangd(
    http: &Http,
    base: &Path,
    d: &ClangdDownload,
    on: &mut (dyn FnMut(Event) + Send),
) -> Result<ClangdInstallation, String> {
    if let Some(i) = installed_clangd(base).filter(|i| i.version == d.version) {
        on(Event::step(format!("clangd {} is already installed.", d.version)));
        return Ok(i);
    }
    let sha256 = match &d.sha256 {
        Some(s) => s.clone(),
        None => {
            on(Event::step(format!("Reading the SHA-256 of {} from {}…", d.asset, d.api_url)));
            let json = http.text(&d.api_url).await?;
            asset_digest(&json, &d.asset).ok_or_else(|| {
                format!(
                    "GitHub reports no SHA-256 for {}: Forge does not install unverified downloads. Install clangd from your package manager or LLVM instead.",
                    d.asset
                )
            })?
        }
    };
    on(Event::step(format!("Downloading {}…", d.url)));
    let label = format!("Downloading clangd {}", d.version);
    let data = http
        .bytes(&d.url, |done, total| on(Event::progress(&label, done, total)))
        .await?;
    on(Event::step("Unpacking clangd…"));
    let inst = install_clangd_archive(&data, base, d, &sha256)?;
    on(Event::step(format!(
        "Verified SHA-256 {sha256} of clangd {}.",
        d.version
    )));
    Ok(inst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed::archive::tests::zip;
    use crate::managed::testserver;

    fn archive() -> Vec<u8> {
        zip(&[
            ("clangd_19.1.2/bin/clangd", b"#!/bin/sh\necho clangd version 19.1.2\n".as_slice()),
            ("clangd_19.1.2/lib/clang/19/include/stddef.h", b"/* header */".as_slice()),
        ])
    }

    #[test]
    fn pinned_builds() {
        let linux = clangd_release("linux", "x86_64").unwrap();
        assert_eq!(linux.asset, "clangd-linux-19.1.2.zip");
        assert_eq!(
            linux.url,
            "https://github.com/clangd/clangd/releases/download/19.1.2/clangd-linux-19.1.2.zip"
        );
        assert_eq!(linux.api_url, "https://api.github.com/repos/clangd/clangd/releases/tags/19.1.2");
        assert_eq!(clangd_release("macos", "aarch64").unwrap().asset, "clangd-mac-19.1.2.zip");
        assert!(clangd_release("linux", "aarch64").is_none());
        let json = r#"{"tag_name": "19.1.2", "assets": [
            {"name": "clangd-linux-19.1.2.zip", "digest": "sha256:ABCDEF0123456789abcdef0123456789abcdef0123456789abcdef0123456789"},
            {"name": "clangd-mac-19.1.2.zip", "digest": null}
        ]}"#;
        assert_eq!(
            asset_digest(json, "clangd-linux-19.1.2.zip").as_deref(),
            Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
        );
        assert_eq!(asset_digest(json, "clangd-mac-19.1.2.zip"), None);
        assert_eq!(asset_digest("nope", "x"), None);
    }

    #[test]
    fn verifies_and_unpacks() {
        let dir = tempfile::tempdir().unwrap();
        let data = archive();
        let d = clangd_release("linux", "x86_64").unwrap();
        let err = install_clangd_archive(&data, dir.path(), &d, &"00".repeat(32)).unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
        assert!(installed_clangd(dir.path()).is_none());
        let inst = install_clangd_archive(&data, dir.path(), &d, &sha256_hex(&data)).unwrap();
        assert_eq!(inst.version, "19.1.2");
        assert_eq!(inst.binary, dir.path().join("19.1.2/bin/clangd"));
        assert!(dir.path().join("19.1.2/lib/clang/19/include/stddef.h").is_file());
        assert_eq!(installed_clangd(dir.path()), Some(inst));
    }

    #[tokio::test]
    async fn downloads_with_the_api_digest() {
        let server = testserver::start().await;
        let data = archive();
        let mut d = clangd_release("linux", "x86_64").unwrap();
        d.url = format!("{}/download/{}", server.url, d.asset);
        d.api_url = format!("{}/release", server.url);
        server.routes.set(&format!("/download/{}", d.asset), data.clone());
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
        let mut steps = Vec::new();
        let inst = install_clangd(&http, dir.path(), &d, &mut |e| {
            if let Event::Step { message } = e {
                steps.push(message)
            }
        })
        .await
        .unwrap();
        assert!(inst.binary.is_file());
        assert!(steps.last().unwrap().starts_with("Verified SHA-256"));
        // Installed: no second download.
        install_clangd(&http, dir.path(), &d, &mut |_| {}).await.unwrap();
        assert_eq!(server.routes.hits(&format!("/download/{}", d.asset)), 1);
        // No digest: refused before downloading.
        let other = tempfile::tempdir().unwrap();
        server.routes.set("/release", r#"{"assets": []}"#);
        let err = install_clangd(&http, other.path(), &d, &mut |_| {}).await.unwrap_err();
        assert!(err.contains("no SHA-256"), "{err}");
        assert_eq!(server.routes.hits(&format!("/download/{}", d.asset)), 1);
    }
}
