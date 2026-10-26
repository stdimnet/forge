//! Go SDK downloads (with the user's consent): the release list of `go.dev/dl/?mode=json`
//! names every archive with its SHA-256; Forge downloads the archive for this platform,
//! checks the checksum and unpacks it into `<data dir>/forge/go-sdks/go<version>`.

use std::path::{Path, PathBuf};

use forge_langservers::managed::archive;
use forge_langservers::managed::node::sha256_hex;
use forge_langservers::managed::{Event, Http};
use serde::{Deserialize, Serialize};

use crate::detect::go_executable;

pub const DEFAULT_SOURCE: &str = "https://go.dev/dl";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoFile {
    pub filename: String,
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub arch: String,
    pub version: String,
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    /// `archive`, `installer` or `source`.
    #[serde(default)]
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoRelease {
    /// `go1.25.3`.
    pub version: String,
    #[serde(default)]
    pub stable: bool,
    #[serde(default)]
    pub files: Vec<GoFile>,
}

/// Go's names of this platform (`linux`/`darwin`/`windows`, `amd64`/`arm64`/…).
pub fn platform() -> (String, String) {
    platform_of(std::env::consts::OS, std::env::consts::ARCH)
}

pub fn platform_of(os: &str, arch: &str) -> (String, String) {
    let os = match os {
        "macos" => "darwin",
        other => other,
    };
    let arch = match arch {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" => "386",
        "arm" => "armv6l",
        "powerpc64" => "ppc64le",
        other => other,
    };
    (os.to_string(), arch.to_string())
}

/// What the SDK download dialog offers: stable releases with an archive for the platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Downloadable {
    /// `1.25.3`.
    pub version: String,
    pub file: GoFile,
}

/// The archive of each stable release for `os`/`arch`, newest first.
pub fn downloadable(releases: &[GoRelease], os: &str, arch: &str) -> Vec<Downloadable> {
    releases
        .iter()
        .filter(|r| r.stable)
        .filter_map(|r| {
            let file = r
                .files
                .iter()
                .find(|f| f.kind == "archive" && f.os == os && f.arch == arch)?;
            Some(Downloadable {
                version: r.version.trim_start_matches("go").to_string(),
                file: file.clone(),
            })
        })
        .collect()
}

pub fn parse_releases(json: &str) -> Result<Vec<GoRelease>, String> {
    serde_json::from_str(json).map_err(|e| format!("unexpected go.dev/dl answer: {e}"))
}

/// The release list (`<source>/?mode=json`, stable releases).
pub async fn releases(http: &Http, source: &str) -> Result<Vec<GoRelease>, String> {
    let text = http
        .text(&format!("{}/?mode=json", source.trim_end_matches('/')))
        .await?;
    parse_releases(&text)
}

/// Download `file`, verify it and unpack it into `<sdks>/go<version>`. Returns the SDK root.
pub async fn install(
    http: &Http,
    source: &str,
    file: &GoFile,
    sdks: &Path,
    on: &mut (dyn FnMut(Event) + Send),
) -> Result<PathBuf, String> {
    if file.sha256.len() != 64 {
        return Err(format!("{} has no SHA-256 checksum", file.filename));
    }
    if file.filename.contains(['/', '\\']) || file.filename.starts_with('.') {
        return Err(format!("unexpected file name {}", file.filename));
    }
    let url = format!("{}/{}", source.trim_end_matches('/'), file.filename);
    on(Event::step(format!("Downloading {}…", file.filename)));
    let label = format!("Downloading Go {}", file.version.trim_start_matches("go"));
    let data = http
        .bytes(&url, |done, total| {
            on(Event::progress(&label, done, total.or(Some(file.size))))
        })
        .await?;
    let actual = sha256_hex(&data);
    if !actual.eq_ignore_ascii_case(&file.sha256) {
        return Err(format!(
            "checksum mismatch for {}: expected SHA-256 {}, got {actual}. The download was discarded.",
            file.filename, file.sha256
        ));
    }
    on(Event::step(format!(
        "Verified SHA-256 of {}.",
        file.filename
    )));
    std::fs::create_dir_all(sdks).map_err(|e| format!("cannot create {}: {e}", sdks.display()))?;
    let dest = sdks.join(&file.version);
    let staging = sdks.join(format!(".{}.partial", file.version));
    let _ = std::fs::remove_dir_all(&staging);
    on(Event::step("Unpacking the Go SDK…"));
    // The archives hold everything below `go/`.
    if file.filename.ends_with(".zip") {
        archive::extract_zip(&data, &staging, 1)?;
    } else if file.filename.ends_with(".tar.gz") {
        archive::extract_tar_gz(&data, &staging, 1)?;
    } else {
        return Err(format!(
            "{} is not an archive Forge can unpack",
            file.filename
        ));
    }
    let windows = file.os == "windows";
    if !go_executable(&staging, windows).is_file() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!("{} does not contain bin/go", file.filename));
    }
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&staging, &dest)
        .map_err(|e| format!("cannot move the SDK into {}: {e}", dest.display()))?;
    on(Event::step(format!("Go SDK ready in {}.", dest.display())));
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testserver;
    use std::io::Write;

    const LIST: &str = r#"[
      {"version":"go1.25.3","stable":true,"files":[
        {"filename":"go1.25.3.src.tar.gz","os":"","arch":"","version":"go1.25.3","sha256":"aa","size":1,"kind":"source"},
        {"filename":"go1.25.3.linux-amd64.tar.gz","os":"linux","arch":"amd64","version":"go1.25.3","sha256":"SHA","size":2,"kind":"archive"},
        {"filename":"go1.25.3.windows-amd64.msi","os":"windows","arch":"amd64","version":"go1.25.3","sha256":"bb","size":3,"kind":"installer"},
        {"filename":"go1.25.3.windows-amd64.zip","os":"windows","arch":"amd64","version":"go1.25.3","sha256":"cc","size":3,"kind":"archive"}]},
      {"version":"go1.26rc1","stable":false,"files":[
        {"filename":"go1.26rc1.linux-amd64.tar.gz","os":"linux","arch":"amd64","version":"go1.26rc1","sha256":"dd","size":2,"kind":"archive"}]},
      {"version":"go1.24.9","stable":true,"files":[
        {"filename":"go1.24.9.linux-amd64.tar.gz","os":"linux","arch":"amd64","version":"go1.24.9","sha256":"ee","size":2,"kind":"archive"}]}
    ]"#;

    #[test]
    fn picks_stable_archives_for_the_platform() {
        let r = parse_releases(LIST).unwrap();
        let d = downloadable(&r, "linux", "amd64");
        let v: Vec<_> = d
            .iter()
            .map(|d| (d.version.as_str(), d.file.filename.as_str()))
            .collect();
        assert_eq!(
            v,
            [
                ("1.25.3", "go1.25.3.linux-amd64.tar.gz"),
                ("1.24.9", "go1.24.9.linux-amd64.tar.gz")
            ]
        );
        assert_eq!(
            downloadable(&r, "windows", "amd64")[0].file.filename,
            "go1.25.3.windows-amd64.zip"
        );
        assert_eq!(
            platform_of("macos", "aarch64"),
            ("darwin".into(), "arm64".into())
        );
    }

    fn sdk_tarball() -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, data, mode) in [
            ("go/bin/go", &b"#!/bin/sh\n"[..], 0o755),
            ("go/VERSION", &b"go1.25.3\n"[..], 0o644),
            ("go/src/fmt/print.go", &b"package fmt\n"[..], 0o644),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(mode);
            header.set_cksum();
            builder.append_data(&mut header, path, data).unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).unwrap();
        gz.finish().unwrap()
    }

    #[tokio::test]
    async fn downloads_verifies_and_unpacks() {
        let server = testserver::start().await;
        let data = sdk_tarball();
        server
            .routes
            .set("/dl/go1.25.3.linux-amd64.tar.gz", data.clone());
        let mut file = GoFile {
            filename: "go1.25.3.linux-amd64.tar.gz".into(),
            os: "linux".into(),
            arch: "amd64".into(),
            version: "go1.25.3".into(),
            sha256: sha256_hex(&data),
            size: data.len() as u64,
            kind: "archive".into(),
        };
        let dir = tempfile::tempdir().unwrap();
        let http = Http::direct().unwrap();
        let source = format!("{}/dl", server.url);
        let mut events = Vec::new();
        let root = install(&http, &source, &file, dir.path(), &mut |e| events.push(e))
            .await
            .unwrap();
        assert_eq!(root, dir.path().join("go1.25.3"));
        assert_eq!(crate::detect::sdk_version(&root).as_deref(), Some("1.25.3"));
        assert!(events.iter().any(
            |e| matches!(e, Event::Step { message } if message.starts_with("Verified SHA-256"))
        ));

        file.sha256 = "0".repeat(64);
        let err = install(&http, &source, &file, dir.path(), &mut |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
        file.filename = "../evil.tar.gz".into();
        assert!(install(&http, &source, &file, dir.path(), &mut |_| {})
            .await
            .is_err());
    }
}
