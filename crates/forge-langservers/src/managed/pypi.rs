//! Language servers that PyPI ships as binary wheels (ruff, ty): the wheel for this
//! platform is downloaded from the index (`langservers.download.pypiIndex`, default
//! `https://pypi.org`), checked against the SHA-256 digest of the index's JSON API, and
//! its executable (`<name>-<version>.data/scripts/<name>`) is copied to
//! `<managed dir>/bin`, which the detector searches. Installed versions are recorded in
//! `<managed dir>/pypi-tools.json`. No Python is needed.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::http::Http;
use super::node::sha256_hex;
use super::Event;

pub const DEFAULT_INDEX: &str = "https://pypi.org";

/// One file of a release.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReleaseFile {
    pub filename: String,
    pub url: String,
    #[serde(default)]
    pub digests: Digests,
    #[serde(default)]
    pub packagetype: String,
    #[serde(default)]
    pub yanked: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct Digests {
    #[serde(default)]
    pub sha256: String,
}

#[derive(Debug, Deserialize)]
struct Info {
    version: String,
}

#[derive(Debug, Deserialize)]
struct Project {
    info: Info,
    #[serde(default)]
    urls: Vec<ReleaseFile>,
}

/// The latest release of a project: its version and files.
pub fn parse_release(json: &str) -> Result<(String, Vec<ReleaseFile>), String> {
    let p: Project =
        serde_json::from_str(json).map_err(|e| format!("unexpected PyPI response: {e}"))?;
    Ok((p.info.version, p.urls))
}

/// The wheel platform tags of `file` (`ruff-0.7.0-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl`).
fn platform_tags(file: &str) -> Vec<&str> {
    let Some(stem) = file.strip_suffix(".whl") else {
        return Vec::new();
    };
    stem.rsplit('-')
        .next()
        .map(|t| t.split('.').collect())
        .unwrap_or_default()
}

/// How well a platform tag fits `os` / `arch` (higher is better; 0 = not at all).
fn fit(tag: &str, os: &str, arch: &str) -> u8 {
    let arch_ok = |names: &[&str]| names.iter().any(|a| tag.ends_with(a));
    match (os, arch) {
        ("linux", "x86_64") if arch_ok(&["_x86_64"]) => linux_fit(tag),
        ("linux", "aarch64") if arch_ok(&["_aarch64"]) => linux_fit(tag),
        ("macos", "aarch64") if tag.starts_with("macosx_") && arch_ok(&["_arm64"]) => 3,
        ("macos", "x86_64") if tag.starts_with("macosx_") && arch_ok(&["_x86_64"]) => 3,
        ("macos", _) if tag.starts_with("macosx_") && arch_ok(&["_universal2"]) => 2,
        ("windows", "x86_64") if tag == "win_amd64" => 3,
        ("windows", "aarch64") if tag == "win_arm64" => 3,
        _ => 0,
    }
}

/// glibc wheels before musl ones.
fn linux_fit(tag: &str) -> u8 {
    if tag.starts_with("manylinux") {
        3
    } else if tag.starts_with("musllinux") {
        1
    } else {
        0
    }
}

/// The best wheel for `os` (`linux`, `macos`, `windows`) and `arch` (`x86_64`, `aarch64`).
pub fn choose_wheel<'a>(files: &'a [ReleaseFile], os: &str, arch: &str) -> Option<&'a ReleaseFile> {
    files
        .iter()
        .filter(|f| f.packagetype == "bdist_wheel" && !f.yanked && !f.digests.sha256.is_empty())
        .map(|f| {
            let score = platform_tags(&f.filename)
                .iter()
                .map(|t| fit(t, os, arch))
                .max()
                .unwrap_or(0);
            (score, f)
        })
        .filter(|(s, _)| *s > 0)
        .max_by_key(|(s, _)| *s)
        .map(|(_, f)| f)
}

/// The executable `name` (`name.exe`) in a wheel's `*.data/scripts/`.
pub fn extract_script(wheel: &[u8], name: &str) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(wheel))
        .map_err(|e| format!("not a wheel: {e}"))?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let path = entry.name().to_string();
        let mut parts = path.rsplit('/');
        let file = parts.next().unwrap_or("");
        let scripts = parts.next() == Some("scripts");
        let data = parts.next().is_some_and(|d| d.ends_with(".data"));
        if scripts && data && (file == name || file == format!("{name}.exe")) {
            let mut out = Vec::new();
            entry.read_to_end(&mut out).map_err(|e| e.to_string())?;
            return Ok(out);
        }
    }
    Err(format!("the wheel has no {name} executable"))
}

/// Where the executables go.
pub fn bin_dir(managed: &Path) -> PathBuf {
    managed.join("bin")
}

fn record_file(managed: &Path) -> PathBuf {
    managed.join("pypi-tools.json")
}

/// Versions of the tools installed from PyPI.
pub fn recorded(managed: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(record_file(managed))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Download and install `name`'s executable for this platform.
pub async fn install(
    http: &Http,
    index: &str,
    name: &str,
    managed: &Path,
    (os, arch): (&str, &str),
    on: &mut (dyn FnMut(Event) + Send),
) -> Result<String, String> {
    let index = index.trim_end_matches('/');
    let url = format!("{index}/pypi/{name}/json");
    on(Event::step(format!("Fetching {name} from {index}…")));
    let (version, files) = parse_release(&http.text(&url).await?)?;
    let wheel = choose_wheel(&files, os, arch)
        .ok_or_else(|| format!("{name} {version} has no build for {os} {arch}; install it with pipx or your package manager"))?;
    let label = format!("{name} {version}");
    let data = http
        .bytes(&wheel.url, |done, total| {
            on(Event::progress(&label, done, total))
        })
        .await?;
    let digest = sha256_hex(&data);
    if !digest.eq_ignore_ascii_case(wheel.digests.sha256.trim()) {
        return Err(format!(
            "SHA-256 of {} does not match the index; nothing was installed",
            wheel.filename
        ));
    }
    on(Event::step(format!(
        "Verified SHA-256 of {}.",
        wheel.filename
    )));
    let windows = os == "windows";
    let exe = extract_script(&data, name)?;
    let dir = bin_dir(managed);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let target = dir.join(if windows {
        format!("{name}.exe")
    } else {
        name.to_string()
    });
    let staging = dir.join(format!(".{name}.download"));
    std::fs::write(&staging, &exe)
        .map_err(|e| format!("cannot write {}: {e}", staging.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    std::fs::rename(&staging, &target)
        .map_err(|e| format!("cannot install {}: {e}", target.display()))?;
    let mut versions = recorded(managed);
    versions.insert(name.to_string(), version.clone());
    if let Ok(json) = serde_json::to_string_pretty(&versions) {
        let _ = std::fs::write(record_file(managed), json);
    }
    on(Event::step(format!(
        "Installed {label} to {}.",
        target.display()
    )));
    Ok(version)
}

/// The OS and architecture names of this machine for [`choose_wheel`].
pub fn current_platform() -> (&'static str, &'static str) {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    (os, arch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed::archive::tests::zip;
    use crate::managed::testserver;

    fn release(server: &str, digest: &str) -> String {
        format!(
            r#"{{"info": {{"version": "0.7.0"}}, "urls": [
            {{"filename": "ruff-0.7.0.tar.gz", "url": "{server}/sdist", "packagetype": "sdist", "digests": {{"sha256": "00"}}}},
            {{"filename": "ruff-0.7.0-py3-none-musllinux_1_2_x86_64.whl", "url": "{server}/musl.whl", "packagetype": "bdist_wheel", "digests": {{"sha256": "{digest}"}}}},
            {{"filename": "ruff-0.7.0-py3-none-manylinux_2_17_x86_64.manylinux2014_x86_64.whl", "url": "{server}/ruff.whl", "packagetype": "bdist_wheel", "digests": {{"sha256": "{digest}"}}}},
            {{"filename": "ruff-0.7.0-py3-none-macosx_11_0_arm64.whl", "url": "{server}/mac.whl", "packagetype": "bdist_wheel", "digests": {{"sha256": "{digest}"}}}},
            {{"filename": "ruff-0.7.0-py3-none-win_amd64.whl", "url": "{server}/win.whl", "packagetype": "bdist_wheel", "digests": {{"sha256": "{digest}"}}}}
            ]}}"#
        )
    }

    #[test]
    fn chooses_the_wheel_for_the_platform() {
        let (_, files) = parse_release(&release("http://x", "ab")).unwrap();
        let pick = |os, arch| choose_wheel(&files, os, arch).map(|f| f.url.as_str());
        assert_eq!(pick("linux", "x86_64"), Some("http://x/ruff.whl"));
        assert_eq!(pick("macos", "aarch64"), Some("http://x/mac.whl"));
        assert_eq!(pick("windows", "x86_64"), Some("http://x/win.whl"));
        assert_eq!(pick("linux", "aarch64"), None);
    }

    #[tokio::test]
    async fn installs_a_verified_wheel() {
        let server = testserver::start().await;
        let wheel = zip(&[
            ("ruff-0.7.0.dist-info/METADATA", b"Name: ruff"),
            (
                "ruff-0.7.0.data/scripts/ruff",
                b"#!/bin/sh\necho ruff 0.7.0\n",
            ),
        ]);
        server
            .routes
            .set("/pypi/ruff/json", release(&server.url, &sha256_hex(&wheel)));
        server.routes.set("/ruff.whl", wheel.clone());
        let tmp = tempfile::tempdir().unwrap();
        let http = Http::direct().unwrap();
        let mut events = Vec::new();
        let version = install(
            &http,
            &server.url,
            "ruff",
            tmp.path(),
            ("linux", "x86_64"),
            &mut |e| events.push(e),
        )
        .await
        .unwrap();
        assert_eq!(version, "0.7.0");
        let exe = tmp.path().join("bin/ruff");
        assert_eq!(
            std::fs::read(&exe).unwrap(),
            b"#!/bin/sh\necho ruff 0.7.0\n"
        );
        assert_eq!(
            recorded(tmp.path()).get("ruff").map(String::as_str),
            Some("0.7.0")
        );
        assert!(events.iter().any(
            |e| matches!(e, Event::Step { message } if message.starts_with("Verified SHA-256"))
        ));

        // A tampered download is refused and nothing is written.
        let other = tempfile::tempdir().unwrap();
        server
            .routes
            .set("/pypi/ruff/json", release(&server.url, &"0".repeat(64)));
        let err = install(
            &http,
            &server.url,
            "ruff",
            other.path(),
            ("linux", "x86_64"),
            &mut |_| {},
        )
        .await
        .unwrap_err();
        assert!(err.contains("SHA-256"), "{err}");
        assert!(!other.path().join("bin/ruff").exists());
    }
}
