//! The js-debug release Forge downloads: Microsoft's standalone DAP server
//! (`js-debug-dap-v<version>.tar.gz` from the GitHub releases of microsoft/vscode-js-debug,
//! MIT licensed). Nothing is bundled: the user agrees to the download, the archive is
//! checked against a pinned SHA-256 and unpacked into `<data dir>/forge/debug-adapters/
//! js-debug/<version>` with its `LICENSE`.

use std::path::{Path, PathBuf};

use forge_langservers::managed::node::sha256_hex;
use forge_langservers::managed::{archive, Event, Http};
use serde::Serialize;

/// A pinned js-debug release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    pub url: String,
    /// SHA-256 of the archive, lowercase hex. Downloads that differ are discarded.
    pub sha256: String,
    /// Size of the archive in bytes (shown in the consent dialog).
    pub size: u64,
}

/// js-debug v1.140.0 (`js-debug-dap-v1.140.0.tar.gz`).
pub fn pinned_release() -> Release {
    Release {
        version: "1.140.0".into(),
        url: "https://github.com/microsoft/vscode-js-debug/releases/download/v1.140.0/js-debug-dap-v1.140.0.tar.gz".into(),
        sha256: "27dab92937ec1ab35821ae955aac867544fe06a1b6307229049f2d789af10968".into(),
        size: 1_249_709,
    }
}

/// The DAP server script below an unpacked js-debug folder.
pub const SERVER_SCRIPT: &str = "src/dapDebugServer.js";

/// Marks a completely unpacked release.
const COMPLETE: &str = ".forge-complete";

/// `<data dir>/forge/debug-adapters/js-debug`.
pub fn default_base_dir() -> Option<PathBuf> {
    forge_langservers::Detector::system()
        .runtimes_dir()
        .and_then(Path::parent)
        .map(|forge| forge.join("debug-adapters").join("js-debug"))
}

/// The server script of the installed `version` below `base`.
pub fn installed(base: &Path, version: &str) -> Option<PathBuf> {
    let dir = base.join(version);
    let script = dir.join(SERVER_SCRIPT);
    (dir.join(COMPLETE).is_file() && script.is_file()).then_some(script)
}

/// The server script of a folder the user chose (`web.jsDebug.path`): the unpacked
/// `js-debug` folder, its parent, or the script itself.
pub fn user_server(path: &Path) -> Option<PathBuf> {
    [
        path.to_path_buf(),
        path.join(SERVER_SCRIPT),
        path.join("js-debug").join(SERVER_SCRIPT),
    ]
    .into_iter()
    .find(|p| p.is_file() && p.file_name().is_some_and(|n| n == "dapDebugServer.js"))
}

/// Verify a downloaded archive and unpack it into `base/<version>`.
pub fn install_archive(data: &[u8], base: &Path, release: &Release) -> Result<PathBuf, String> {
    let actual = sha256_hex(data);
    if !actual.eq_ignore_ascii_case(release.sha256.trim()) {
        return Err(format!(
            "checksum mismatch for js-debug {}: expected SHA-256 {}, got {actual}. The download was discarded.",
            release.version, release.sha256
        ));
    }
    std::fs::create_dir_all(base).map_err(|e| format!("cannot create {}: {e}", base.display()))?;
    let dest = base.join(&release.version);
    let staging = base.join(format!(".{}.partial", release.version));
    let _ = std::fs::remove_dir_all(&staging);
    // The archive has one top-level folder, `js-debug/`.
    archive::extract_tar_gz(data, &staging, 1)?;
    if !staging.join(SERVER_SCRIPT).is_file() || !staging.join("LICENSE").is_file() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!(
            "the js-debug archive has no {SERVER_SCRIPT} or LICENSE"
        ));
    }
    std::fs::write(staging.join(COMPLETE), &release.version)
        .map_err(|e| format!("cannot write into {}: {e}", staging.display()))?;
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&staging, &dest)
        .map_err(|e| format!("cannot move js-debug into {}: {e}", dest.display()))?;
    Ok(dest.join(SERVER_SCRIPT))
}

/// Download, verify and unpack `release` into `base` unless it is there already; returns
/// the server script.
pub async fn ensure(
    http: &Http,
    base: &Path,
    release: &Release,
    on: &mut (dyn FnMut(Event) + Send),
) -> Result<PathBuf, String> {
    if let Some(script) = installed(base, &release.version) {
        return Ok(script);
    }
    on(Event::step(format!(
        "Downloading js-debug {} from {}…",
        release.version, release.url
    )));
    let label = format!("Downloading js-debug {}", release.version);
    let data = http
        .bytes(&release.url, |done, total| {
            on(Event::progress(&label, done, total))
        })
        .await?;
    let script = install_archive(&data, base, release)?;
    on(Event::step(format!(
        "Verified SHA-256 {} of js-debug {}.",
        release.sha256, release.version
    )));
    on(Event::step(format!(
        "js-debug is ready in {}.",
        script
            .parent()
            .and_then(Path::parent)
            .unwrap_or(base)
            .display()
    )));
    Ok(script)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    pub fn tar_gz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            builder.append_data(&mut header, path, *data).unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).unwrap();
        gz.finish().unwrap()
    }

    fn fake_release(data: &[u8]) -> Release {
        Release {
            version: "1.0.0".into(),
            url: "http://127.0.0.1:9/js-debug.tar.gz".into(),
            sha256: sha256_hex(data),
            size: data.len() as u64,
        }
    }

    #[test]
    fn verifies_and_unpacks_the_archive() {
        let data = tar_gz(&[
            ("js-debug/LICENSE", b"MIT License"),
            ("js-debug/src/dapDebugServer.js", b"console.log('dap')"),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let release = fake_release(&data);
        assert!(installed(dir.path(), "1.0.0").is_none());
        let script = install_archive(&data, dir.path(), &release).unwrap();
        assert_eq!(script, dir.path().join("1.0.0/src/dapDebugServer.js"));
        assert_eq!(installed(dir.path(), "1.0.0"), Some(script.clone()));
        assert_eq!(user_server(&dir.path().join("1.0.0")), Some(script.clone()));
        assert_eq!(user_server(&script), Some(script));
        assert!(user_server(dir.path()).is_none());
    }

    #[test]
    fn refuses_archives_with_another_checksum_or_content() {
        let data = tar_gz(&[("js-debug/LICENSE", b"MIT")]);
        let dir = tempfile::tempdir().unwrap();
        let mut release = fake_release(&data);
        let err = install_archive(&data, dir.path(), &release).unwrap_err();
        assert!(err.contains("dapDebugServer.js"), "{err}");
        release.sha256 = "00".repeat(32);
        let err = install_archive(&data, dir.path(), &release).unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
        assert!(installed(dir.path(), "1.0.0").is_none());
    }

    #[test]
    fn the_pinned_release_is_complete() {
        let r = pinned_release();
        assert_eq!(r.sha256.len(), 64);
        assert!(r
            .url
            .starts_with("https://github.com/microsoft/vscode-js-debug/releases/download/v"));
        assert!(r
            .url
            .ends_with(&format!("js-debug-dap-v{}.tar.gz", r.version)));
    }

    #[tokio::test]
    async fn installed_releases_are_not_downloaded_again() {
        let data = tar_gz(&[
            ("js-debug/LICENSE", b"MIT License"),
            ("js-debug/src/dapDebugServer.js", b"//"),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let release = fake_release(&data);
        install_archive(&data, dir.path(), &release).unwrap();
        // The URL is unreachable: a download would fail.
        let http = Http::direct().unwrap();
        let mut events = Vec::new();
        let script = ensure(&http, dir.path(), &release, &mut |e| events.push(e))
            .await
            .unwrap();
        assert!(script.ends_with(SERVER_SCRIPT));
        assert!(events.is_empty());
    }
}
