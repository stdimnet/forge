//! PHP stubs for Go to Declaration on PHP's own classes, functions and constants
//! (`\Throwable`, `array_map`, `PHP_EOL`), like PHPStorm's "PHP Runtime" library.
//!
//! Forge uses JetBrains' phpstorm-stubs (Apache-2.0, redistributable): a pinned release is
//! downloaded on first use into `<cache dir>/forge/php-stubs/<version>`, checked against
//! its SHA-256 and unpacked with its `LICENSE` (and `NOTICE`, when the release has one).
//! Users can point `php.stubs.path` at their own checkout instead, and Intelephense's own
//! stubs (`lib/stub` in its package) are used when it is installed and nothing else is.

use std::path::{Path, PathBuf};

use super::node::sha256_hex;
use super::{archive, Event, Http};

/// A pinned phpstorm-stubs release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StubsRelease {
    pub version: String,
    /// A `.tar.gz` with one top-level folder (GitHub's source archive of the release
    /// commit).
    pub url: String,
    /// SHA-256 of the archive, lowercase hex. Downloads without one are refused.
    pub sha256: String,
}

/// The release Forge downloads: phpstorm-stubs v2025.3 (commit `d1ee5e5`).
///
/// The checksum has to be filled in from a verified download of `url` before the managed
/// download is enabled; until then Forge uses `php.stubs.path` or Intelephense's stubs and
/// explains how to get stubs instead of downloading unverified files.
pub fn pinned_release() -> StubsRelease {
    StubsRelease {
        version: "2025.3".into(),
        url: "https://codeload.github.com/JetBrains/phpstorm-stubs/tar.gz/d1ee5e570343bd4276a3d5959e6e1c2530b006d0".into(),
        sha256: PINNED_SHA256.into(),
    }
}

/// SHA-256 of the pinned archive (see [`pinned_release`]).
const PINNED_SHA256: &str = "";

/// `<cache dir>/forge/php-stubs`.
pub fn default_stubs_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("forge").join("php-stubs"))
}

/// Marks a completely unpacked release.
const COMPLETE: &str = ".forge-complete";

/// The unpacked `version` below `base`, if it is complete.
pub fn installed(base: &Path, version: &str) -> Option<PathBuf> {
    let dir = base.join(version);
    dir.join(COMPLETE).is_file().then_some(dir)
}

/// Intelephense's own stubs (`node_modules/intelephense/lib/stub`) in Forge's server
/// directory or next to an `intelephense` executable, when installed.
pub fn intelephense_stubs(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find_map(|c| {
        let mut dir = c.as_path();
        // From an executable (`…/node_modules/.bin/intelephense` or
        // `…/node_modules/intelephense/lib/intelephense.js`) up to the package.
        for _ in 0..4 {
            for pkg in [
                dir.join("node_modules/intelephense/lib/stub"),
                dir.join("lib/stub"),
            ] {
                if pkg.is_dir() && pkg.join("Core").is_dir() {
                    return Some(pkg);
                }
            }
            dir = dir.parent()?;
        }
        None
    })
}

/// Download, verify and unpack `release` into `base/<version>` unless it is there already.
pub async fn ensure(
    http: &Http,
    base: &Path,
    release: &StubsRelease,
    on: &mut (dyn FnMut(Event) + Send),
) -> Result<PathBuf, String> {
    if let Some(dir) = installed(base, &release.version) {
        return Ok(dir);
    }
    if release.sha256.trim().is_empty() {
        return Err(format!(
            "PHP stubs (phpstorm-stubs {}) cannot be downloaded: no verified checksum is \
             configured. Set Settings | PHP | Stubs path to a phpstorm-stubs checkout.",
            release.version
        ));
    }
    on(Event::step(format!(
        "Downloading PHP stubs (phpstorm-stubs {})…",
        release.version
    )));
    let label = format!("Downloading PHP stubs {}", release.version);
    let data = http
        .bytes(&release.url, |done, total| {
            on(Event::progress(&label, done, total))
        })
        .await?;
    let actual = sha256_hex(&data);
    if !actual.eq_ignore_ascii_case(release.sha256.trim()) {
        return Err(format!(
            "checksum mismatch for phpstorm-stubs {}: expected SHA-256 {}, got {actual}. The download was discarded.",
            release.version, release.sha256
        ));
    }
    on(Event::step("Verified SHA-256 of the PHP stubs."));
    std::fs::create_dir_all(base).map_err(|e| format!("cannot create {}: {e}", base.display()))?;
    let dest = base.join(&release.version);
    let staging = base.join(format!(".{}.partial", release.version));
    let _ = std::fs::remove_dir_all(&staging);
    archive::extract_tar_gz(&data, &staging, 1)?;
    if !staging.join("LICENSE").is_file() || !staging.join("Core").is_dir() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err("the PHP stubs archive has no LICENSE or Core folder".into());
    }
    std::fs::write(staging.join(COMPLETE), &release.version)
        .map_err(|e| format!("cannot write into {}: {e}", staging.display()))?;
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&staging, &dest)
        .map_err(|e| format!("cannot move the PHP stubs into {}: {e}", dest.display()))?;
    on(Event::step(format!(
        "PHP stubs are ready in {}.",
        dest.display()
    )));
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed::archive::tests::tar_gz;
    use crate::managed::testserver;

    fn archive() -> Vec<u8> {
        tar_gz(&[
            ("phpstorm-stubs-abc/LICENSE", b"Apache License 2.0", 0o644),
            (
                "phpstorm-stubs-abc/Core/Core_c.php",
                b"<?php\ninterface Throwable {}\n",
                0o644,
            ),
            ("phpstorm-stubs-abc/tests/Test.php", b"<?php\n", 0o644),
        ])
    }

    fn release(url: &str, sha256: &str) -> StubsRelease {
        StubsRelease {
            version: "2025.3".into(),
            url: format!("{url}/stubs.tar.gz"),
            sha256: sha256.into(),
        }
    }

    #[tokio::test]
    async fn downloads_verifies_and_unpacks_once() {
        let server = testserver::start().await;
        let data = archive();
        server.routes.set("/stubs.tar.gz", data.clone());
        let base = tempfile::tempdir().unwrap();
        let http = Http::direct().unwrap();
        let rel = release(&server.url, &sha256_hex(&data));
        let mut steps = Vec::new();
        let dir = ensure(&http, base.path(), &rel, &mut |e| steps.push(e))
            .await
            .unwrap();
        assert_eq!(dir, base.path().join("2025.3"));
        assert!(dir.join("Core/Core_c.php").is_file());
        assert!(dir.join("LICENSE").is_file());
        assert!(steps
            .iter()
            .any(|e| matches!(e, Event::Step { message } if message.contains("Verified"))));
        // Second call: no download.
        ensure(&http, base.path(), &rel, &mut |_| {}).await.unwrap();
        assert_eq!(server.routes.hits("/stubs.tar.gz"), 1);
        assert_eq!(installed(base.path(), "2025.3"), Some(dir));
    }

    #[tokio::test]
    async fn refuses_tampered_or_unpinned_downloads() {
        let server = testserver::start().await;
        server.routes.set("/stubs.tar.gz", archive());
        let base = tempfile::tempdir().unwrap();
        let http = Http::direct().unwrap();
        let err = ensure(
            &http,
            base.path(),
            &release(&server.url, &"0".repeat(64)),
            &mut |_| {},
        )
        .await
        .unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
        assert!(installed(base.path(), "2025.3").is_none());
        assert!(!base.path().join("2025.3").exists());

        let err = ensure(&http, base.path(), &release(&server.url, ""), &mut |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("Stubs path"), "{err}");
        assert_eq!(server.routes.hits("/stubs.tar.gz"), 1);
    }

    #[test]
    fn finds_intelephense_stubs_from_its_executable() {
        let dir = tempfile::tempdir().unwrap();
        let pkg = dir.path().join("node_modules/intelephense");
        std::fs::create_dir_all(pkg.join("lib/stub/Core")).unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules/.bin")).unwrap();
        let bin = dir.path().join("node_modules/.bin/intelephense");
        std::fs::write(&bin, "").unwrap();
        assert_eq!(
            intelephense_stubs(&[PathBuf::from("/nowhere"), bin]),
            Some(pkg.join("lib/stub"))
        );
        assert_eq!(
            intelephense_stubs(&[dir.path().to_path_buf()]),
            Some(pkg.join("lib/stub"))
        );
    }
}
