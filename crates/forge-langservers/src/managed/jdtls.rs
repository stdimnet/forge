//! Eclipse JDT LS (EPL-2.0) for JavaForge, installed with the user's consent from the
//! official Eclipse download site: the newest milestone (or snapshot) build named by
//! `latest.txt`, checked against the `.sha256` file published next to it and unpacked into
//! `<data dir>/forge/langservers/jdtls/<version>`. JDT LS is a Java program: JavaForge
//! starts it with a JDK 21+ (see `javaforge-lang-java`'s `jdtls` module).

use std::path::{Path, PathBuf};

use super::node::sha256_hex;
use super::{archive, Event, Http};

/// The official download site (setting `java.jdtls.downloadSite`).
pub const DEFAULT_MIRROR: &str = "https://download.eclipse.org/jdtls";

/// Marks a completely unpacked installation.
const COMPLETE: &str = ".forge-complete";
/// Names the installation in use (`<base>/current`).
const CURRENT: &str = "current";

/// Milestones (releases) or nightly snapshots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    Milestones,
    Snapshots,
}

impl Channel {
    pub fn parse(s: &str) -> Self {
        if s.eq_ignore_ascii_case("snapshots") {
            Channel::Snapshots
        } else {
            Channel::Milestones
        }
    }
}

/// A JDT LS build on the download site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// `1.40.0-202409261450`.
    pub version: String,
    /// `jdt-language-server-1.40.0-202409261450.tar.gz`.
    pub file: String,
    pub url: String,
}

/// `<data dir>/forge/langservers/jdtls`.
pub fn default_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("langservers").join("jdtls"))
}

/// Milestone versions in the listing of `<site>/milestones/`, oldest first.
pub fn parse_milestones(html: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in html.split("milestones/").skip(1) {
        let v: String = part
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let v = v.trim_end_matches('.').to_string();
        if v.split('.').count() == 3 && v.split('.').all(|p| !p.is_empty()) && !out.contains(&v) {
            out.push(v);
        }
    }
    out.sort_by_key(|v| version_key(v));
    out
}

fn version_key(v: &str) -> Vec<u64> {
    v.split(['.', '-'])
        .map(|p| p.parse::<u64>().unwrap_or(0))
        .collect()
}

/// `jdt-language-server-1.40.0-202409261450.tar.gz` → `1.40.0-202409261450`.
pub fn version_of(file: &str) -> Option<String> {
    let v = file
        .trim()
        .strip_prefix("jdt-language-server-")?
        .strip_suffix(".tar.gz")?;
    (!v.is_empty()).then(|| v.to_string())
}

/// The newest build of a channel.
pub async fn latest(http: &Http, site: &str, channel: Channel) -> Result<Release, String> {
    let site = site.trim_end_matches('/');
    let dir = match channel {
        Channel::Snapshots => format!("{site}/snapshots"),
        Channel::Milestones => {
            let listing = http.text(&format!("{site}/milestones/")).await?;
            let newest = parse_milestones(&listing)
                .pop()
                .ok_or("the JDT LS download site lists no milestones")?;
            format!("{site}/milestones/{newest}")
        }
    };
    let file = http.text(&format!("{dir}/latest.txt")).await?;
    let file = file.trim().to_string();
    let version =
        version_of(&file).ok_or_else(|| format!("unexpected JDT LS build name '{file}'"))?;
    Ok(Release {
        version,
        url: format!("{dir}/{file}"),
        file,
    })
}

/// An unpacked JDT LS.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub dir: PathBuf,
    pub version: String,
    /// `plugins/org.eclipse.equinox.launcher_<version>.jar`.
    pub launcher_jar: PathBuf,
}

/// The equinox launcher jar of an installation.
pub fn launcher_jar(dir: &Path) -> Option<PathBuf> {
    let mut jars: Vec<PathBuf> = std::fs::read_dir(dir.join("plugins"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().map(|n| n.to_string_lossy()).is_some_and(|n| {
                n.starts_with("org.eclipse.equinox.launcher_") && n.ends_with(".jar")
            })
        })
        .collect();
    jars.sort();
    jars.pop()
}

/// An installation in a folder (Forge's, or the user's own `java.jdtls.path`).
pub fn inspect(dir: &Path) -> Option<Installation> {
    let launcher_jar = launcher_jar(dir)?;
    let version = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Some(Installation {
        dir: dir.to_path_buf(),
        version,
        launcher_jar,
    })
}

/// The installation Forge downloaded last, if it is complete.
pub fn installed(base: &Path) -> Option<Installation> {
    let version = std::fs::read_to_string(base.join(CURRENT)).ok()?;
    let dir = base.join(version.trim());
    if !dir.join(COMPLETE).is_file() {
        return None;
    }
    inspect(&dir)
}

/// Download, verify and unpack the newest build of `channel` unless it is installed.
pub async fn install(
    http: &Http,
    site: &str,
    channel: Channel,
    base: &Path,
    on: &mut (dyn FnMut(Event) + Send),
) -> Result<Installation, String> {
    on(Event::step(format!(
        "Looking up the newest Eclipse JDT LS on {site}…"
    )));
    let release = latest(http, site, channel).await?;
    let dest = base.join(&release.version);
    if dest.join(COMPLETE).is_file() {
        if let Some(found) = inspect(&dest) {
            std::fs::write(base.join(CURRENT), &release.version)
                .map_err(|e| format!("cannot write {}: {e}", base.display()))?;
            on(Event::step(format!(
                "JDT LS {} is already installed.",
                release.version
            )));
            return Ok(found);
        }
    }
    let sums = http.text(&format!("{}.sha256", release.url)).await?;
    let expected = sums
        .split_whitespace()
        .next()
        .map(str::to_ascii_lowercase)
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| format!("{}.sha256 holds no SHA-256 checksum", release.file))?;
    on(Event::step(format!("Downloading {}…", release.url)));
    let label = format!("Downloading JDT LS {}", release.version);
    let data = http
        .bytes(&release.url, |done, total| {
            on(Event::progress(&label, done, total))
        })
        .await?;
    let actual = sha256_hex(&data);
    if actual != expected {
        return Err(format!(
            "{}: SHA-256 mismatch (expected {expected}, got {actual}); the download was discarded",
            release.file
        ));
    }
    on(Event::step(format!(
        "Verified SHA-256 of {}.",
        release.file
    )));
    std::fs::create_dir_all(base).map_err(|e| format!("cannot create {}: {e}", base.display()))?;
    let staging = base.join(format!(".{}.partial", release.version));
    let _ = std::fs::remove_dir_all(&staging);
    archive::extract_tar_gz(&data, &staging, 0)?;
    if launcher_jar(&staging).is_none() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(format!("{} holds no JDT LS launcher", release.file));
    }
    std::fs::write(staging.join(COMPLETE), "")
        .map_err(|e| format!("cannot write {}: {e}", staging.display()))?;
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&staging, &dest)
        .map_err(|e| format!("cannot move JDT LS into {}: {e}", dest.display()))?;
    std::fs::write(base.join(CURRENT), &release.version)
        .map_err(|e| format!("cannot write {}: {e}", base.display()))?;
    on(Event::step(format!(
        "Installed JDT LS {} in {}.",
        release.version,
        dest.display()
    )));
    inspect(&dest).ok_or_else(|| "the installation is incomplete".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed::archive::tests::tar_gz;
    use crate::managed::testserver;

    const LISTING: &str = r#"<a href="/jdtls/milestones/1.9.0">1.9.0</a>
<a href='/jdtls/milestones/1.40.0'>1.40.0</a> <a href="/jdtls/milestones/1.39.0/">1.39.0</a>
<a href="/jdtls/milestones/..">..</a>"#;

    fn serve(server: &testserver::TestServer, sum: Option<&str>) -> Vec<u8> {
        let tgz = tar_gz(&[
            (
                "plugins/org.eclipse.equinox.launcher_1.6.900.v20240613.jar",
                b"jar",
                0o644,
            ),
            ("config_linux/config.ini", b"osgi", 0o644),
            ("bin/jdtls", b"#!/usr/bin/env python3", 0o755),
        ]);
        let file = "jdt-language-server-1.40.0-202409261450.tar.gz";
        server.routes.set("/milestones/", LISTING);
        server
            .routes
            .set("/milestones/1.40.0/latest.txt", format!("{file}\n"));
        server
            .routes
            .set(&format!("/milestones/1.40.0/{file}"), tgz.clone());
        let sum = sum.map(str::to_string).unwrap_or_else(|| sha256_hex(&tgz));
        server.routes.set(
            &format!("/milestones/1.40.0/{file}.sha256"),
            format!("{sum}  {file}\n"),
        );
        tgz
    }

    #[test]
    fn parses_listings_and_names() {
        assert_eq!(parse_milestones(LISTING), ["1.9.0", "1.39.0", "1.40.0"]);
        assert_eq!(
            version_of("jdt-language-server-1.63.0-202610191322.tar.gz").as_deref(),
            Some("1.63.0-202610191322")
        );
        assert_eq!(version_of("index.html"), None);
        assert_eq!(Channel::parse("Snapshots"), Channel::Snapshots);
        assert_eq!(Channel::parse(""), Channel::Milestones);
    }

    #[tokio::test]
    async fn installs_the_newest_milestone_once() {
        let server = testserver::start().await;
        serve(&server, None);
        let tmp = tempfile::tempdir().unwrap();
        let http = Http::direct().unwrap();
        let inst = install(
            &http,
            &server.url,
            Channel::Milestones,
            tmp.path(),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert_eq!(inst.version, "1.40.0-202409261450");
        assert!(inst
            .launcher_jar
            .ends_with("plugins/org.eclipse.equinox.launcher_1.6.900.v20240613.jar"));
        assert_eq!(installed(tmp.path()), Some(inst.clone()));
        install(
            &http,
            &server.url,
            Channel::Milestones,
            tmp.path(),
            &mut |_| {},
        )
        .await
        .unwrap();
        assert_eq!(
            server
                .routes
                .hits("/milestones/1.40.0/jdt-language-server-1.40.0-202409261450.tar.gz"),
            1
        );
    }

    #[tokio::test]
    async fn refuses_a_tampered_download() {
        let server = testserver::start().await;
        serve(&server, Some(&"ab".repeat(32)));
        let tmp = tempfile::tempdir().unwrap();
        let http = Http::direct().unwrap();
        let err = install(
            &http,
            &server.url,
            Channel::Milestones,
            tmp.path(),
            &mut |_| {},
        )
        .await
        .unwrap_err();
        assert!(err.contains("SHA-256 mismatch"), "{err}");
        assert!(installed(tmp.path()).is_none());
    }
}
