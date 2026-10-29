//! rust-analyzer (MIT OR Apache-2.0) from its official GitHub releases, downloaded only when
//! the user agrees and no rust-analyzer comes with the toolchain (`rustup component add
//! rust-analyzer` is preferred). The release is pinned ([`PINNED_RELEASE`]); the asset of
//! the platform (`rust-analyzer-<target>.gz`, `.zip` on Windows) is checked against the
//! SHA-256 digest GitHub publishes for it in the release's API answer and unpacked into
//! `<data dir>/forge/langservers/rust-analyzer/<release>`.

use std::io::Read;
use std::path::{Path, PathBuf};

use forge_langservers::managed::node::sha256_hex;
use forge_langservers::managed::{archive, Event, Http};
use serde::Deserialize;

/// The rust-analyzer release Forge downloads (weekly releases are named by date).
pub const PINNED_RELEASE: &str = "2026-10-19";
/// GitHub's API, where the release lists its assets with their digests.
pub const DEFAULT_API: &str = "https://api.github.com/repos/rust-lang/rust-analyzer";

const CURRENT: &str = "current";

/// `<data dir>/forge/langservers/rust-analyzer`.
pub fn default_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("langservers").join("rust-analyzer"))
}

/// The release asset for a platform (Rust's `std::env::consts` names).
pub fn asset_name(os: &str, arch: &str) -> Option<String> {
    let target = match (os, arch) {
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        _ => return None,
    };
    Some(if os == "windows" {
        format!("rust-analyzer-{target}.zip")
    } else {
        format!("rust-analyzer-{target}.gz")
    })
}

fn executable(dir: &Path, windows: bool) -> PathBuf {
    dir.join(if windows {
        "rust-analyzer.exe"
    } else {
        "rust-analyzer"
    })
}

/// The installed download, if any.
pub fn installed(base: &Path) -> Option<PathBuf> {
    let release = std::fs::read_to_string(base.join(CURRENT)).ok()?;
    let exe = executable(&base.join(release.trim()), cfg!(windows));
    exe.is_file().then_some(exe)
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    digest: Option<String>,
    #[serde(default)]
    size: u64,
}

/// Download, verify and unpack `release` for `asset` (see [`asset_name`]); returns the
/// executable.
pub async fn install(
    http: &Http,
    api: &str,
    release: &str,
    asset: &str,
    base: &Path,
    on: &mut (dyn FnMut(Event) + Send),
) -> Result<PathBuf, String> {
    let valid = release
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'));
    if release.is_empty() || !valid || release.contains("..") {
        return Err(format!("'{release}' is no rust-analyzer release"));
    }
    on(Event::step(format!(
        "Looking up rust-analyzer {release} on GitHub…"
    )));
    let info: Release = http
        .json(&format!(
            "{}/releases/tags/{release}",
            api.trim_end_matches('/')
        ))
        .await?;
    let a = info
        .assets
        .iter()
        .find(|a| a.name == asset)
        .ok_or_else(|| format!("rust-analyzer {} has no {asset}", info.tag_name))?;
    let expected = a
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .ok_or_else(|| format!("GitHub publishes no SHA-256 digest for {asset}"))?
        .to_ascii_lowercase();
    let label = format!("rust-analyzer {release}");
    let data = http
        .bytes(&a.browser_download_url, |done, total| {
            on(Event::progress(&label, done, total.or(Some(a.size))))
        })
        .await?;
    let actual = sha256_hex(&data);
    if actual != expected {
        return Err(format!(
            "checksum mismatch for {asset}: expected {expected}, got {actual}"
        ));
    }
    on(Event::step(format!("Verified SHA-256 of {asset}.")));
    let dir = base.join(release);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let windows = asset.ends_with(".zip");
    let exe = executable(&dir, windows);
    if windows {
        archive::extract_zip(&data, &dir, 0)?;
        if !exe.is_file() {
            return Err(format!("{asset} holds no rust-analyzer.exe"));
        }
    } else {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&data[..])
            .read_to_end(&mut out)
            .map_err(|e| format!("cannot unpack {asset}: {e}"))?;
        std::fs::write(&exe, out).map_err(|e| format!("cannot write {}: {e}", exe.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
    }
    std::fs::write(base.join(CURRENT), release).map_err(|e| e.to_string())?;
    on(Event::step(format!(
        "rust-analyzer ready in {}.",
        dir.display()
    )));
    Ok(exe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_langservers::managed::testserver;
    use std::io::Write;

    #[test]
    fn assets_per_platform() {
        assert_eq!(
            asset_name("linux", "x86_64").as_deref(),
            Some("rust-analyzer-x86_64-unknown-linux-gnu.gz")
        );
        assert_eq!(
            asset_name("windows", "aarch64").as_deref(),
            Some("rust-analyzer-aarch64-pc-windows-msvc.zip")
        );
        assert_eq!(asset_name("freebsd", "x86_64"), None);
    }

    #[tokio::test]
    async fn downloads_verifies_and_unpacks() {
        let server = testserver::start().await;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(b"#!/bin/sh\necho rust-analyzer\n").unwrap();
        let data = gz.finish().unwrap();
        let asset = "rust-analyzer-x86_64-unknown-linux-gnu.gz";
        let release_json = |digest: &str| {
            format!(
                r#"{{"tag_name":"{PINNED_RELEASE}","assets":[{{"name":"{asset}","browser_download_url":"{}/dl/{asset}","digest":"sha256:{digest}","size":{}}}]}}"#,
                server.url,
                data.len()
            )
        };
        server.routes.set(
            &format!("/releases/tags/{PINNED_RELEASE}"),
            release_json(&sha256_hex(&data)),
        );
        server.routes.set(&format!("/dl/{asset}"), data.clone());
        let base = tempfile::tempdir().unwrap();
        let http = Http::direct().unwrap();
        let mut events = Vec::new();
        let exe = install(
            &http,
            &server.url,
            PINNED_RELEASE,
            asset,
            base.path(),
            &mut |e| events.push(e),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&exe).unwrap(),
            "#!/bin/sh\necho rust-analyzer\n"
        );
        assert!(events.iter().any(
            |e| matches!(e, Event::Step { message } if message.starts_with("Verified SHA-256"))
        ));
        if cfg!(unix) {
            assert_eq!(installed(base.path()), Some(exe));
        }
        // A wrong digest is refused.
        server.routes.set(
            &format!("/releases/tags/{PINNED_RELEASE}"),
            release_json(&"0".repeat(64)),
        );
        let err = install(
            &http,
            &server.url,
            PINNED_RELEASE,
            asset,
            base.path(),
            &mut |_| {},
        )
        .await
        .unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
        let err = install(
            &http,
            &server.url,
            PINNED_RELEASE,
            "rust-analyzer-x.gz",
            base.path(),
            &mut |_| {},
        )
        .await
        .unwrap_err();
        assert!(err.contains("has no"), "{err}");
    }
}
