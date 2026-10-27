//! DotNetForge's Roslyn language server download, with the user's consent:
//! `Microsoft.CodeAnalysis.LanguageServer.<rid>` (MIT, built from dotnet/roslyn), the
//! server VS Code's C# extension uses, from Microsoft's public `vs-impl` NuGet feed on
//! Azure DevOps at a pinned version, unpacked into
//! `<data dir>/forge/langservers/roslyn/<version>`. It runs on the user's .NET runtime
//! (`dotnet Microsoft.CodeAnalysis.LanguageServer.dll`), so a .NET 9 runtime or SDK is
//! needed.
//!
//! Integrity: the feed publishes no checksums, so every platform's package is pinned with
//! the SHA-256 recorded from the official feed when the version was chosen
//! ([`ROSLYN_BUILDS`]); a download that differs is discarded. A platform without a
//! recorded hash is refused.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::node::sha256_hex;
use super::{archive, Event, Http};

/// The pinned server version (published 2026-10-26).
pub const ROSLYN_VERSION: &str = "5.0.0-2.26526.1";
/// The public feed's flat container.
pub const ROSLYN_FEED: &str =
    "https://pkgs.dev.azure.com/azure-public/vside/_packaging/vs-impl/nuget/v3/flat2";

/// `(os, arch, runtime id, SHA-256, size)` of the pinned packages.
pub const ROSLYN_BUILDS: &[(&str, &str, &str, &str, u64)] = &[
    (
        "linux",
        "x86_64",
        "linux-x64",
        "255512b2b6e7b735e93c7576fb861b9c7d5fbb1ed74aa737772140e1489bfe1d",
        59_909_005,
    ),
    (
        "linux",
        "aarch64",
        "linux-arm64",
        "638f08e9eaa165face3de40b99f617939562cdfbe16e612d40ed287c88bd23eb",
        58_077_170,
    ),
    (
        "macos",
        "x86_64",
        "osx-x64",
        "da91487d1ca5397353e2b79c2e66c06165c125c48435846871be403737de25ed",
        60_133_490,
    ),
    (
        "macos",
        "aarch64",
        "osx-arm64",
        "72e0ff3966b8571fc6537b2ec85860d0d9979d2d52bbd7a3f5ca98c4da78e723",
        58_201_145,
    ),
    (
        "windows",
        "x86_64",
        "win-x64",
        "91482049401a3d98a00972f1c74d5d6e42f7371cfae66bc8f1c85ee3c4a6d92e",
        60_208_724,
    ),
    (
        "windows",
        "aarch64",
        "win-arm64",
        "64ddd0f474f9a27700fcc9da6263c7e9765155d3b4e34536bd0d1cefb15bc026",
        58_999_163,
    ),
];

/// The server assembly inside an installation.
pub const SERVER_DLL: &str = "Microsoft.CodeAnalysis.LanguageServer.dll";

/// A pinned download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoslynDownload {
    pub version: String,
    pub rid: String,
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

/// The pinned package for a platform (`std::env::consts` names).
pub fn roslyn_release(os: &str, arch: &str) -> Option<RoslynDownload> {
    let (_, _, rid, sha, size) = ROSLYN_BUILDS
        .iter()
        .find(|(o, a, ..)| *o == os && *a == arch)?;
    let id = format!("microsoft.codeanalysis.languageserver.{rid}");
    Some(RoslynDownload {
        version: ROSLYN_VERSION.into(),
        rid: rid.to_string(),
        url: format!("{ROSLYN_FEED}/{id}/{ROSLYN_VERSION}/{id}.{ROSLYN_VERSION}.nupkg"),
        sha256: sha.to_string(),
        size: *size,
    })
}

pub fn current_roslyn_release() -> Option<RoslynDownload> {
    roslyn_release(std::env::consts::OS, std::env::consts::ARCH)
}

/// `<data dir>/forge/langservers/roslyn`.
pub fn roslyn_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("langservers").join("roslyn"))
}

const COMPLETE: &str = ".forge-complete";

/// An unpacked server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoslynInstallation {
    pub dir: PathBuf,
    /// `Microsoft.CodeAnalysis.LanguageServer.dll`.
    pub dll: PathBuf,
    pub version: String,
}

/// A server folder the user points to (`dotnet.roslyn.path`): the folder or the dll.
pub fn inspect_roslyn(path: &Path) -> Option<RoslynInstallation> {
    let (dir, dll) = if path.is_dir() {
        (path.to_path_buf(), path.join(SERVER_DLL))
    } else {
        (path.parent()?.to_path_buf(), path.to_path_buf())
    };
    dll.is_file().then(|| RoslynInstallation {
        version: std::fs::read_to_string(dir.join(COMPLETE))
            .map(|v| v.trim().to_string())
            .unwrap_or_default(),
        dir,
        dll,
    })
}

/// The server Forge installed (the pinned version, else the newest complete one).
pub fn installed_roslyn(base: &Path) -> Option<RoslynInstallation> {
    let pinned = base.join(ROSLYN_VERSION);
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
    inspect_roslyn(&dir)
}

/// Verify a downloaded package against its pinned SHA-256 and unpack the server folder
/// (`content/LanguageServer/<rid>/`) into `base/<version>`.
pub fn install_roslyn_package(
    data: &[u8],
    base: &Path,
    d: &RoslynDownload,
) -> Result<RoslynInstallation, String> {
    let actual = sha256_hex(data);
    if !actual.eq_ignore_ascii_case(&d.sha256) {
        return Err(format!(
            "checksum mismatch for the Roslyn language server {}: expected SHA-256 {}, got {actual}. The download was discarded.",
            d.version, d.sha256
        ));
    }
    std::fs::create_dir_all(base).map_err(|e| format!("cannot create {}: {e}", base.display()))?;
    let dest = base.join(&d.version);
    let staging = base.join(format!(".{}.partial", d.version));
    let _ = std::fs::remove_dir_all(&staging);
    archive::extract_zip_prefix(data, &staging, &format!("content/LanguageServer/{}", d.rid))?;
    if !staging.join(SERVER_DLL).is_file() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!(
            "the package has no content/LanguageServer/{}/{SERVER_DLL}",
            d.rid
        ));
    }
    std::fs::write(staging.join(COMPLETE), &d.version)
        .map_err(|e| format!("cannot write into {}: {e}", staging.display()))?;
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&staging, &dest)
        .map_err(|e| format!("cannot move the server into {}: {e}", dest.display()))?;
    inspect_roslyn(&dest).ok_or_else(|| "the server was unpacked without its assembly".into())
}

/// Download, verify and unpack the pinned server unless it is installed. `d` is
/// [`current_roslyn_release`] (tests point it at a local server).
pub async fn install_roslyn(
    http: &Http,
    base: &Path,
    d: &RoslynDownload,
    on: &mut (dyn FnMut(Event) + Send),
) -> Result<RoslynInstallation, String> {
    if let Some(i) = installed_roslyn(base).filter(|i| i.version == d.version) {
        on(Event::step(format!(
            "The Roslyn language server {} is already installed.",
            d.version
        )));
        return Ok(i);
    }
    if d.sha256.len() != 64 {
        return Err(format!(
            "No SHA-256 is recorded for the Roslyn language server on {}: Forge does not install unverified downloads.",
            d.rid
        ));
    }
    on(Event::step(format!("Downloading {}…", d.url)));
    let label = format!("Downloading the Roslyn language server {}", d.version);
    let data = http
        .bytes(&d.url, |done, total| {
            on(Event::progress(&label, done, total))
        })
        .await?;
    on(Event::step("Unpacking the Roslyn language server…"));
    let inst = install_roslyn_package(&data, base, d)?;
    on(Event::step(format!(
        "Verified SHA-256 {} of the Roslyn language server {}.",
        d.sha256, d.version
    )));
    Ok(inst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed::archive::tests::zip;
    use crate::managed::testserver;

    fn package() -> Vec<u8> {
        zip(&[
            ("Microsoft.CodeAnalysis.LanguageServer.linux-x64.nuspec", b"<package/>".as_slice()),
            ("lib/net9.0/Microsoft.CodeAnalysis.LanguageServer.dll", b"lib copy".as_slice()),
            ("content/LanguageServer/linux-x64/Microsoft.CodeAnalysis.LanguageServer.dll", b"MZ server".as_slice()),
            ("content/LanguageServer/linux-x64/de/Microsoft.CodeAnalysis.LanguageServer.resources.dll", b"MZ de".as_slice()),
        ])
    }

    #[test]
    fn pinned_builds() {
        let d = roslyn_release("linux", "x86_64").unwrap();
        assert_eq!(d.rid, "linux-x64");
        assert_eq!(
            d.url,
            "https://pkgs.dev.azure.com/azure-public/vside/_packaging/vs-impl/nuget/v3/flat2/microsoft.codeanalysis.languageserver.linux-x64/5.0.0-2.26526.1/microsoft.codeanalysis.languageserver.linux-x64.5.0.0-2.26526.1.nupkg"
        );
        assert_eq!(d.sha256.len(), 64);
        assert!(ROSLYN_BUILDS.iter().all(|b| b.3.len() == 64 && b.4 > 0));
        assert_eq!(
            roslyn_release("windows", "aarch64").unwrap().rid,
            "win-arm64"
        );
        assert!(roslyn_release("freebsd", "x86_64").is_none());
    }

    #[test]
    fn verifies_and_unpacks_only_the_server() {
        let dir = tempfile::tempdir().unwrap();
        let data = package();
        let mut d = roslyn_release("linux", "x86_64").unwrap();
        let err = install_roslyn_package(&data, dir.path(), &d).unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
        assert!(installed_roslyn(dir.path()).is_none());
        d.sha256 = sha256_hex(&data);
        let inst = install_roslyn_package(&data, dir.path(), &d).unwrap();
        assert_eq!(inst.version, ROSLYN_VERSION);
        assert_eq!(inst.dll, dir.path().join(ROSLYN_VERSION).join(SERVER_DLL));
        assert_eq!(std::fs::read(&inst.dll).unwrap(), b"MZ server");
        assert!(dir.path().join(ROSLYN_VERSION).join("de").is_dir());
        assert!(!dir.path().join(ROSLYN_VERSION).join("lib").exists());
        assert_eq!(installed_roslyn(dir.path()), Some(inst.clone()));
        assert_eq!(inspect_roslyn(&inst.dll).unwrap().dir, inst.dir);
    }

    #[tokio::test]
    async fn downloads_once() {
        let server = testserver::start().await;
        let data = package();
        let mut d = roslyn_release("linux", "x86_64").unwrap();
        d.url = format!("{}/roslyn.nupkg", server.url);
        d.sha256 = sha256_hex(&data);
        server.routes.set("/roslyn.nupkg", data.clone());
        let dir = tempfile::tempdir().unwrap();
        let http = Http::direct().unwrap();
        let mut steps = Vec::new();
        let inst = install_roslyn(&http, dir.path(), &d, &mut |e| {
            if let Event::Step { message } = e {
                steps.push(message)
            }
        })
        .await
        .unwrap();
        assert!(inst.dll.is_file());
        assert!(steps.last().unwrap().starts_with("Verified SHA-256"));
        install_roslyn(&http, dir.path(), &d, &mut |_| {})
            .await
            .unwrap();
        assert_eq!(server.routes.hits("/roslyn.nupkg"), 1);
        // No recorded hash: refused before downloading.
        d.sha256.clear();
        let other = tempfile::tempdir().unwrap();
        let err = install_roslyn(&http, other.path(), &d, &mut |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("unverified"), "{err}");
        assert_eq!(server.routes.hits("/roslyn.nupkg"), 1);
    }
}
