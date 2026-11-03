//! PHP stubs for Go to Declaration on PHP's own classes, functions and constants
//! (`\Throwable`, `array_map`, `PHP_EOL`), like PHPStorm's "PHP Runtime" library.
//!
//! Forge uses JetBrains' phpstorm-stubs (Apache-2.0, redistributable): a pinned release is
//! downloaded on first use into `<cache dir>/forge/php-stubs/<version>`, unpacked with its
//! `LICENSE` (and `NOTICE`, when the release has one) and checked against a pinned digest of
//! its files (see [`content_digest`]).
//! Users can point `php.stubs.path` at their own checkout instead.

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
    /// [`content_digest`] of the unpacked files, lowercase hex. Downloads without one are
    /// refused. The files are pinned rather than the archive bytes, because GitHub builds the
    /// archive on the fly; the digest can be computed from `git archive <commit>` as well.
    pub content_sha256: String,
}

/// The release Forge downloads: phpstorm-stubs v2025.3 (commit `d1ee5e5`).
pub fn pinned_release() -> StubsRelease {
    StubsRelease {
        version: "2025.3".into(),
        url: "https://codeload.github.com/JetBrains/phpstorm-stubs/tar.gz/d1ee5e570343bd4276a3d5959e6e1c2530b006d0".into(),
        content_sha256: PINNED_CONTENT_SHA256.into(),
    }
}

/// [`content_digest`] of the pinned release, computed from
/// `git archive d1ee5e570343bd4276a3d5959e6e1c2530b006d0` of JetBrains/phpstorm-stubs.
const PINNED_CONTENT_SHA256: &str =
    "551300552df8fa898b81ce72d258153bfc0dcc2a436f7ff091478cf168773f85";

/// SHA-256 over the regular files below `dir`, sorted by their `/`-separated relative path:
/// one line `"<path>\0<sha256 of the file>\n"` per file. Symbolic links count as
/// `"<path>\0-> <target>\n"`.
pub fn content_digest(dir: &Path) -> std::io::Result<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .expect("below root")
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                let target = std::fs::read_link(&path)?;
                out.push((rel, format!("-> {}", target.to_string_lossy())));
            } else if kind.is_dir() {
                walk(root, &path, out)?;
            } else if kind.is_file() {
                out.push((rel, sha256_hex(&std::fs::read(&path)?)));
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files)?;
    files.sort();
    let mut listing = Vec::new();
    for (path, hash) in files {
        listing.extend_from_slice(path.as_bytes());
        listing.push(0);
        listing.extend_from_slice(hash.as_bytes());
        listing.push(b'\n');
    }
    Ok(sha256_hex(&listing))
}

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
    if release.content_sha256.trim().is_empty() {
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
    std::fs::create_dir_all(base).map_err(|e| format!("cannot create {}: {e}", base.display()))?;
    let dest = base.join(&release.version);
    let staging = base.join(format!(".{}.partial", release.version));
    let _ = std::fs::remove_dir_all(&staging);
    archive::extract_tar_gz(&data, &staging, 1)?;
    let actual =
        content_digest(&staging).map_err(|e| format!("cannot read the unpacked PHP stubs: {e}"))?;
    if !actual.eq_ignore_ascii_case(release.content_sha256.trim()) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!(
            "checksum mismatch for phpstorm-stubs {}: expected content SHA-256 {}, got {actual}. The download was discarded.",
            release.version, release.content_sha256
        ));
    }
    on(Event::step("Verified the PHP stubs' content digest."));
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

    fn release(url: &str, content_sha256: &str) -> StubsRelease {
        StubsRelease {
            version: "2025.3".into(),
            url: format!("{url}/stubs.tar.gz"),
            content_sha256: content_sha256.into(),
        }
    }

    /// The digest of `archive()` once unpacked.
    fn archive_digest() -> String {
        let dir = tempfile::tempdir().unwrap();
        archive::extract_tar_gz(&archive(), dir.path(), 1).unwrap();
        content_digest(dir.path()).unwrap()
    }

    #[test]
    fn content_digest_hashes_sorted_paths_and_file_hashes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("b")).unwrap();
        std::fs::write(dir.path().join("b/x.php"), b"x").unwrap();
        std::fs::write(dir.path().join("a.txt"), b"a").unwrap();
        let expected = sha256_hex(
            format!(
                "a.txt\0{}\nb/x.php\0{}\n",
                sha256_hex(b"a"),
                sha256_hex(b"x")
            )
            .as_bytes(),
        );
        assert_eq!(content_digest(dir.path()).unwrap(), expected);
    }

    /// Prints the digest of a folder, to pin a new release:
    /// `git archive <commit> | tar -x -C /tmp/stubs` then
    /// `FORGE_STUBS_DIR=/tmp/stubs cargo test -p forge-langservers print_content_digest -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn print_content_digest() {
        let dir = std::env::var("FORGE_STUBS_DIR").expect("FORGE_STUBS_DIR");
        println!("{}", content_digest(Path::new(&dir)).unwrap());
    }

    #[tokio::test]
    async fn downloads_verifies_and_unpacks_once() {
        let server = testserver::start().await;
        let data = archive();
        server.routes.set("/stubs.tar.gz", data.clone());
        let base = tempfile::tempdir().unwrap();
        let http = Http::direct().unwrap();
        let rel = release(&server.url, &archive_digest());
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
}
