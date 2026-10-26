//! Downloading a JDK from Eclipse Adoptium (Temurin), like IntelliJ's "Download JDK…":
//! the Adoptium API names the latest build of a feature release for this platform and its
//! SHA-256; the archive is verified and unpacked into Forge's JDK folder
//! (`<data dir>/forge/jdks/<release>`) through a staging folder that is renamed into place.

use std::path::{Path, PathBuf};

use forge_langservers::managed::node::sha256_hex;
use forge_langservers::managed::{archive, Event, Http};
use serde::{Deserialize, Serialize};

use crate::detect::{inspect, Jdk};

/// The public Adoptium API (setting `java.download.adoptiumApi`).
pub const DEFAULT_API: &str = "https://api.adoptium.net";

/// The feature releases Adoptium builds.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AvailableReleases {
    pub available_lts_releases: Vec<u32>,
    pub available_releases: Vec<u32>,
    pub most_recent_lts: u32,
    pub most_recent_feature_release: u32,
}

/// Adoptium's names for a platform and how its archives are packed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Platform {
    /// `linux`, `mac`, `windows`.
    pub os: &'static str,
    /// `x64`, `aarch64`.
    pub arch: &'static str,
    pub zip: bool,
}

impl Platform {
    /// The platform for Rust's `os` / `arch` names.
    pub fn for_platform(os: &str, arch: &str) -> Option<Self> {
        let arch = match arch {
            "x86_64" => "x64",
            "aarch64" => "aarch64",
            _ => return None,
        };
        let (os, zip) = match os {
            "linux" => ("linux", false),
            "macos" => ("mac", false),
            "windows" => ("windows", true),
            _ => return None,
        };
        Some(Platform { os, arch, zip })
    }

    pub fn current() -> Option<Self> {
        Self::for_platform(std::env::consts::OS, std::env::consts::ARCH)
    }
}

pub fn available_releases_url(api: &str) -> String {
    format!("{}/v3/info/available_releases", api.trim_end_matches('/'))
}

/// The latest JDK build of `feature` for a platform.
pub fn assets_url(api: &str, feature: u32, p: &Platform) -> String {
    format!(
        "{}/v3/assets/latest/{feature}/hotspot?architecture={}&image_type=jdk&os={}&vendor=eclipse",
        api.trim_end_matches('/'),
        p.arch,
        p.os
    )
}

/// A downloadable JDK build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    /// `jdk-21.0.4+7`.
    pub release_name: String,
    pub link: String,
    pub name: String,
    /// SHA-256 (lowercase hex) from the API.
    pub checksum: String,
    pub size: u64,
}

#[derive(Deserialize)]
struct ApiPackage {
    link: String,
    name: String,
    #[serde(default)]
    checksum: Option<String>,
    #[serde(default)]
    size: u64,
}

#[derive(Deserialize)]
struct ApiBinary {
    package: ApiPackage,
    #[serde(default)]
    image_type: String,
}

#[derive(Deserialize)]
struct ApiAsset {
    binary: ApiBinary,
    release_name: String,
}

/// The JDK builds of an `assets/latest` answer.
pub fn parse_assets(json: &str) -> Result<Vec<Asset>, String> {
    let list: Vec<ApiAsset> =
        serde_json::from_str(json).map_err(|e| format!("unexpected Adoptium answer: {e}"))?;
    Ok(list
        .into_iter()
        .filter(|a| a.binary.image_type.is_empty() || a.binary.image_type == "jdk")
        .map(|a| Asset {
            release_name: a.release_name,
            link: a.binary.package.link,
            name: a.binary.package.name,
            checksum: a
                .binary
                .package
                .checksum
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase(),
            size: a.binary.package.size,
        })
        .collect())
}

pub async fn available_releases(http: &Http, api: &str) -> Result<AvailableReleases, String> {
    http.json(&available_releases_url(api)).await
}

/// A file or folder name made of safe characters (`jdk-21.0.4+7`).
fn safe_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-+._".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Download, verify and unpack the latest Temurin JDK of `feature` into `jdks_dir`. An
/// already unpacked build of the same release is reused.
pub async fn download(
    http: &Http,
    api: &str,
    feature: u32,
    platform: &Platform,
    jdks_dir: &Path,
    on: &mut (dyn FnMut(Event) + Send),
) -> Result<Jdk, String> {
    on(Event::step(format!(
        "Looking up Eclipse Temurin {feature} for {} {}…",
        platform.os, platform.arch
    )));
    let text = http.text(&assets_url(api, feature, platform)).await?;
    let asset = parse_assets(&text)?
        .into_iter()
        .next()
        .ok_or_else(|| format!("Eclipse Temurin {feature} has no JDK build for this platform"))?;
    if asset.checksum.len() != 64 {
        return Err(format!(
            "{}: Adoptium published no SHA-256 checksum; the download was not started",
            asset.name
        ));
    }
    let dest = jdks_dir.join(safe_name(&asset.release_name));
    if let Some(jdk) = inspect(&dest, "Forge") {
        on(Event::step(format!(
            "{} is already installed.",
            asset.release_name
        )));
        return Ok(jdk);
    }
    on(Event::step(format!("Downloading {}…", asset.link)));
    let label = format!("Downloading {}", asset.release_name);
    let data = http
        .bytes(&asset.link, |done, total| {
            on(Event::progress(&label, done, total))
        })
        .await?;
    let actual = sha256_hex(&data);
    if actual != asset.checksum {
        return Err(format!(
            "{}: SHA-256 mismatch (expected {}, got {actual}); the download was discarded",
            asset.name, asset.checksum
        ));
    }
    on(Event::step(format!("Verified SHA-256 of {}.", asset.name)));
    std::fs::create_dir_all(jdks_dir)
        .map_err(|e| format!("cannot create {}: {e}", jdks_dir.display()))?;
    let staging = jdks_dir.join(format!(".{}.partial", safe_name(&asset.release_name)));
    let _ = std::fs::remove_dir_all(&staging);
    if platform.zip {
        archive::extract_zip(&data, &staging, 1)?;
    } else {
        archive::extract_tar_gz(&data, &staging, 1)?;
    }
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&staging, &dest)
        .map_err(|e| format!("cannot move the JDK into {}: {e}", dest.display()))?;
    on(Event::step(format!(
        "Installed {} in {}.",
        asset.release_name,
        dest.display()
    )));
    inspect(&dest, "Forge").ok_or_else(|| format!("{} holds no JDK", dest.display()))
}

/// Where Forge keeps the JDKs it downloaded.
pub fn jdk_dir_of(base: &Path, release: &str) -> PathBuf {
    base.join(safe_name(release))
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_langservers::managed::testserver;
    use std::io::Write;

    fn tar_gz(files: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, data, mode) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(*mode);
            header.set_cksum();
            builder.append_data(&mut header, path, *data).unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar).unwrap();
        gz.finish().unwrap()
    }

    fn serve(server: &testserver::TestServer, archive: &[u8], checksum: &str) {
        let p = Platform::for_platform("linux", "x86_64").unwrap();
        let path = assets_url("", 21, &p);
        server.routes.set(
            &path,
            serde_json::json!([{
                "binary": {
                    "image_type": "jdk",
                    "package": {
                        "link": format!("{}/jdk.tar.gz", server.url),
                        "name": "OpenJDK21U-jdk_x64_linux_hotspot_21.0.4_7.tar.gz",
                        "checksum": checksum,
                        "size": archive.len()
                    }
                },
                "release_name": "jdk-21.0.4+7"
            }])
            .to_string(),
        );
        server.routes.set("/jdk.tar.gz", archive.to_vec());
    }

    fn archive() -> Vec<u8> {
        tar_gz(&[
            ("jdk-21.0.4+7/bin/java", b"#!/bin/sh\n", 0o755),
            ("jdk-21.0.4+7/bin/javac", b"#!/bin/sh\n", 0o755),
            (
                "jdk-21.0.4+7/release",
                b"IMPLEMENTOR=\"Eclipse Adoptium\"\nJAVA_VERSION=\"21.0.4\"\n",
                0o644,
            ),
        ])
    }

    #[tokio::test]
    async fn downloads_verifies_and_reuses_a_jdk() {
        let server = testserver::start().await;
        let data = archive();
        serve(&server, &data, &sha256_hex(&data));
        let tmp = tempfile::tempdir().unwrap();
        let http = Http::direct().unwrap();
        let p = Platform::for_platform("linux", "x86_64").unwrap();
        let mut steps = Vec::new();
        let jdk = download(&http, &server.url, 21, &p, tmp.path(), &mut |e| {
            if let Event::Step { message } = e {
                steps.push(message)
            }
        })
        .await
        .unwrap();
        assert_eq!(
            (jdk.major, jdk.vendor.as_str(), jdk.source.as_str()),
            (21, "Eclipse Temurin", "Forge")
        );
        assert!(jdk.home.ends_with("jdk-21.0.4+7"));
        assert!(
            steps.iter().any(|s| s.starts_with("Verified SHA-256")),
            "{steps:?}"
        );
        assert!(!tmp.path().join(".jdk-21.0.4+7.partial").exists());
        download(&http, &server.url, 21, &p, tmp.path(), &mut |_| {})
            .await
            .unwrap();
        assert_eq!(
            server.routes.hits("/jdk.tar.gz"),
            1,
            "the second call reuses the JDK"
        );
    }

    #[tokio::test]
    async fn refuses_bad_or_missing_checksums() {
        let server = testserver::start().await;
        let data = archive();
        serve(&server, &data, &"0".repeat(64));
        let tmp = tempfile::tempdir().unwrap();
        let http = Http::direct().unwrap();
        let p = Platform::for_platform("linux", "x86_64").unwrap();
        let err = download(&http, &server.url, 21, &p, tmp.path(), &mut |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("SHA-256 mismatch"), "{err}");
        assert!(
            std::fs::read_dir(tmp.path())
                .map(|d| d.count())
                .unwrap_or(0)
                == 0
        );
        serve(&server, &data, "");
        let err = download(&http, &server.url, 21, &p, tmp.path(), &mut |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("no SHA-256"), "{err}");
    }

    #[test]
    fn urls_and_platforms() {
        let p = Platform::for_platform("macos", "aarch64").unwrap();
        assert_eq!(
            assets_url("https://api.adoptium.net/", 21, &p),
            "https://api.adoptium.net/v3/assets/latest/21/hotspot?architecture=aarch64&image_type=jdk&os=mac&vendor=eclipse"
        );
        assert!(Platform::for_platform("windows", "x86_64").unwrap().zip);
        assert!(Platform::for_platform("freebsd", "x86_64").is_none());
        assert_eq!(
            available_releases_url(DEFAULT_API),
            "https://api.adoptium.net/v3/info/available_releases"
        );
        let r: AvailableReleases = serde_json::from_str(r#"{"available_lts_releases":[17,21],"available_releases":[17,21,25],"most_recent_lts":21,"most_recent_feature_release":25}"#).unwrap();
        assert_eq!(r.most_recent_lts, 21);
    }
}
