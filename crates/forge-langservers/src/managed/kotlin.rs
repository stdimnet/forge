//! KotlinForge's downloads, each with the user's consent, from the official source, at a
//! pinned version, checked against a pinned SHA-256:
//!
//! - JetBrains' kotlin-lsp, the standalone server archive from download.jetbrains.com
//!   (the links and checksums of github.com/Kotlin/kotlin-lsp's `RELEASES.md`), unpacked
//!   into `<data dir>/forge/langservers/kotlin-lsp/<version>`. Its repository is Apache-2.0;
//!   the server build also contains closed JetBrains components, which is why it is never
//!   bundled with Forge.
//! - ktlint (MIT, `ktlint-cli-<v>-all.jar`) and detekt (Apache-2.0,
//!   `detekt-cli-<v>-all.jar`) from Maven Central into
//!   `<data dir>/forge/langservers/jars`, run with `java -jar`.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::node::sha256_hex;
use super::{archive, Event, Http};

/// Archive format of a kotlin-lsp download.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ArchiveKind {
    TarGz,
    /// `.win.zip`, and `.sit` (macOS builds are zip archives with that extension).
    Zip,
}

/// A pinned download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Download {
    pub name: String,
    pub version: String,
    pub url: String,
    /// Lowercase hex SHA-256; a download that differs is discarded.
    pub sha256: String,
    /// Bytes (shown in the consent balloon).
    pub size: u64,
}

/// kotlin-lsp's pinned version (`RELEASES.md` v263.6379.0).
pub const KOTLIN_LSP_VERSION: &str = "263.6379.0";
const KOTLIN_LSP_BASE: &str = "https://download.jetbrains.com/language-server/kotlin-server";

/// `(os, arch)` → file suffix, SHA-256, size, archive kind of the pinned kotlin-lsp.
const KOTLIN_LSP_BUILDS: &[(&str, &str, &str, &str, u64, ArchiveKind)] = &[
    (
        "linux",
        "x86_64",
        ".tar.gz",
        "ab8ca4455dc2fc5fe1a24db2bccc46c104254d2c465155c4251ee65df8f3f7cc",
        368_488_700,
        ArchiveKind::TarGz,
    ),
    (
        "linux",
        "aarch64",
        "-aarch64.tar.gz",
        "50999901ef8bcfa1e58561b6a8d782a72dea5620fcf92a64130807f8924a56fc",
        367_499_596,
        ArchiveKind::TarGz,
    ),
    (
        "macos",
        "x86_64",
        ".sit",
        "e69e0c9d27b915b2db9ee692ec08d097df1a394d9f21190457ef199f2905f77e",
        362_128_309,
        ArchiveKind::Zip,
    ),
    (
        "macos",
        "aarch64",
        "-aarch64.sit",
        "ebef2e13cd4adc4ec9e04084b848000a3ec7a9d2917c64f269574ce2efe9ecad",
        360_170_749,
        ArchiveKind::Zip,
    ),
    (
        "windows",
        "x86_64",
        ".win.zip",
        "72148fa0832c2a45a4b26c55f56197d8d8886bcdb732b910607efc5e2693b9df",
        363_404_886,
        ArchiveKind::Zip,
    ),
    (
        "windows",
        "aarch64",
        "-aarch64.win.zip",
        "290f48cb564295239bb678bb66b1ebbd06755b14fd4890f3cdf822f41af011ef",
        343_018_568,
        ArchiveKind::Zip,
    ),
];

/// The pinned kotlin-lsp build for a platform (`std::env::consts` names).
pub fn kotlin_lsp_release(os: &str, arch: &str) -> Option<(Download, ArchiveKind)> {
    let (_, _, suffix, sha, size, kind) = KOTLIN_LSP_BUILDS
        .iter()
        .find(|(o, a, ..)| *o == os && *a == arch)?;
    let file = format!("kotlin-server-{KOTLIN_LSP_VERSION}{suffix}");
    Some((
        Download {
            name: "kotlin-lsp".into(),
            version: KOTLIN_LSP_VERSION.into(),
            url: format!("{KOTLIN_LSP_BASE}/{KOTLIN_LSP_VERSION}/{file}"),
            sha256: sha.to_string(),
            size: *size,
        },
        *kind,
    ))
}

/// The pinned build for this machine.
pub fn current_kotlin_lsp_release() -> Option<(Download, ArchiveKind)> {
    kotlin_lsp_release(std::env::consts::OS, std::env::consts::ARCH)
}

/// Maven Central (setting `kotlin.download.repository`).
pub const DEFAULT_REPOSITORY: &str = "https://repo1.maven.org/maven2";

/// A command line tool distributed as one runnable jar on Maven Central.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinnedJar {
    pub name: &'static str,
    pub version: &'static str,
    /// The jar's path in the repository.
    pub path: &'static str,
    pub sha256: &'static str,
    pub size: u64,
    pub license: &'static str,
}

/// ktlint 1.8.0 (`com.pinterest.ktlint:ktlint-cli`, MIT).
pub const KTLINT: PinnedJar = PinnedJar {
    name: "ktlint",
    version: "1.8.0",
    path: "com/pinterest/ktlint/ktlint-cli/1.8.0/ktlint-cli-1.8.0-all.jar",
    sha256: "369ad2b789f95a011f807e1fcb690ccef80bd7cd014fd139e73ae82dcc0baeab",
    size: 71_470_431,
    license: "MIT",
};

/// detekt 1.23.8 (`io.gitlab.arturbosch.detekt:detekt-cli`, Apache-2.0).
pub const DETEKT: PinnedJar = PinnedJar {
    name: "detekt",
    version: "1.23.8",
    path: "io/gitlab/arturbosch/detekt/detekt-cli/1.23.8/detekt-cli-1.23.8-all.jar",
    sha256: "3afe89a11120303c73c9bdda3d8fe558dd9070a6937d27819ddc04b275381245",
    size: 69_922_269,
    license: "Apache-2.0",
};

impl PinnedJar {
    pub fn file_name(&self) -> String {
        format!("{}-{}.jar", self.name, self.version)
    }

    pub fn download(&self, repository: &str) -> Download {
        Download {
            name: self.name.into(),
            version: self.version.into(),
            url: format!("{}/{}", repository.trim_end_matches('/'), self.path),
            sha256: self.sha256.into(),
            size: self.size,
        }
    }
}

/// `<data dir>/forge/langservers/kotlin-lsp`.
pub fn kotlin_lsp_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("langservers").join("kotlin-lsp"))
}

/// `<data dir>/forge/langservers/jars`.
pub fn jars_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("langservers").join("jars"))
}

const COMPLETE: &str = ".forge-complete";

/// An unpacked kotlin-lsp.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KotlinLspInstallation {
    pub dir: PathBuf,
    pub launcher: PathBuf,
    pub version: String,
}

fn launcher_in(dir: &Path) -> Option<PathBuf> {
    [
        "bin/intellij-server",
        "bin/intellij-server.exe",
        "kotlin-lsp.sh",
        "kotlin-lsp.cmd",
    ]
    .iter()
    .map(|l| dir.join(l))
    .find(|p| p.is_file())
}

/// A kotlin-lsp folder the user chose (`kotlin.lsp.path`): the unpacked archive or its
/// launcher.
pub fn inspect_kotlin_lsp(path: &Path) -> Option<KotlinLspInstallation> {
    let (dir, launcher) = if path.is_file() {
        let dir = path.parent()?;
        let dir = if dir.ends_with("bin") {
            dir.parent()?
        } else {
            dir
        };
        (dir.to_path_buf(), path.to_path_buf())
    } else {
        (path.to_path_buf(), launcher_in(path)?)
    };
    let version = std::fs::read_to_string(dir.join("build.txt"))
        .ok()
        .map(|b| b.trim().trim_start_matches("ILS-").to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default();
    Some(KotlinLspInstallation {
        dir,
        launcher,
        version,
    })
}

/// The kotlin-lsp Forge installed (the pinned version, else the newest complete one).
pub fn installed_kotlin_lsp(base: &Path) -> Option<KotlinLspInstallation> {
    let pinned = base.join(KOTLIN_LSP_VERSION);
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(base)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join(COMPLETE).is_file())
        .collect();
    dirs.sort();
    let dir = if pinned.join(COMPLETE).is_file() {
        pinned
    } else {
        dirs.pop()?
    };
    let mut inst = inspect_kotlin_lsp(&dir)?;
    inst.version = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or(inst.version);
    Some(inst)
}

#[cfg(unix)]
fn make_executable(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    // Zip builds lose the modes of the launcher and the bundled runtime's programs.
    for sub in [
        "bin",
        "jbr/bin",
        "jbr/lib",
        "jbr/Contents/Home/bin",
        "jbr/Contents/Home/lib",
    ] {
        let Ok(entries) = std::fs::read_dir(dir.join(sub)) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let program = sub.ends_with("bin") || name == "jspawnhelper";
            if p.is_file() && program && !name.contains('.') || name == "intellij-server" {
                if let Ok(meta) = std::fs::metadata(&p) {
                    let mut perm = meta.permissions();
                    perm.set_mode(perm.mode() | 0o755);
                    let _ = std::fs::set_permissions(&p, perm);
                }
            }
        }
    }
}

#[cfg(not(unix))]
fn make_executable(_dir: &Path) {}

fn verify(data: &[u8], d: &Download) -> Result<(), String> {
    let actual = sha256_hex(data);
    if !actual.eq_ignore_ascii_case(d.sha256.trim()) {
        return Err(format!(
            "checksum mismatch for {} {}: expected SHA-256 {}, got {actual}. The download was discarded.",
            d.name, d.version, d.sha256
        ));
    }
    Ok(())
}

/// Verify a downloaded kotlin-lsp archive and unpack it into `base/<version>`.
pub fn install_kotlin_lsp_archive(
    data: &[u8],
    kind: ArchiveKind,
    base: &Path,
    d: &Download,
) -> Result<KotlinLspInstallation, String> {
    verify(data, d)?;
    std::fs::create_dir_all(base).map_err(|e| format!("cannot create {}: {e}", base.display()))?;
    let dest = base.join(&d.version);
    let staging = base.join(format!(".{}.partial", d.version));
    let _ = std::fs::remove_dir_all(&staging);
    // One top-level folder: `kotlin-server-<version>/`.
    match kind {
        ArchiveKind::TarGz => archive::extract_tar_gz(data, &staging, 1)?,
        ArchiveKind::Zip => archive::extract_zip(data, &staging, 1)?,
    }
    if launcher_in(&staging).is_none() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err("the kotlin-lsp archive has no bin/intellij-server".into());
    }
    make_executable(&staging);
    std::fs::write(staging.join(COMPLETE), &d.version)
        .map_err(|e| format!("cannot write into {}: {e}", staging.display()))?;
    let _ = std::fs::remove_dir_all(&dest);
    std::fs::rename(&staging, &dest)
        .map_err(|e| format!("cannot move kotlin-lsp into {}: {e}", dest.display()))?;
    inspect_kotlin_lsp(&dest)
        .map(|mut i| {
            i.version = d.version.clone();
            i
        })
        .ok_or_else(|| "kotlin-lsp was unpacked without its launcher".into())
}

/// Download, verify and unpack the pinned kotlin-lsp unless it is installed.
pub async fn install_kotlin_lsp(
    http: &Http,
    base: &Path,
    on: &mut (dyn FnMut(Event) + Send),
) -> Result<KotlinLspInstallation, String> {
    let (d, kind) = current_kotlin_lsp_release()
        .ok_or("JetBrains publishes no kotlin-lsp build for this platform")?;
    if let Some(i) = installed_kotlin_lsp(base).filter(|i| i.version == d.version) {
        on(Event::step(format!(
            "kotlin-lsp {} is already installed.",
            d.version
        )));
        return Ok(i);
    }
    on(Event::step(format!("Downloading {}…", d.url)));
    let label = format!("Downloading kotlin-lsp {}", d.version);
    let data = http
        .bytes(&d.url, |done, total| {
            on(Event::progress(&label, done, total))
        })
        .await?;
    on(Event::step("Unpacking kotlin-lsp…"));
    let inst = install_kotlin_lsp_archive(&data, kind, base, &d)?;
    on(Event::step(format!(
        "Verified SHA-256 {} of kotlin-lsp {}.",
        d.sha256, d.version
    )));
    Ok(inst)
}

/// The jar of `tool` below `base`, when Forge installed it.
pub fn installed_jar(base: &Path, tool: &PinnedJar) -> Option<PathBuf> {
    let p = base.join(tool.file_name());
    p.is_file().then_some(p)
}

/// Verify and store a downloaded jar.
pub fn install_jar_bytes(
    data: &[u8],
    base: &Path,
    tool: &PinnedJar,
    d: &Download,
) -> Result<PathBuf, String> {
    verify(data, d)?;
    std::fs::create_dir_all(base).map_err(|e| format!("cannot create {}: {e}", base.display()))?;
    let target = base.join(tool.file_name());
    let partial = base.join(format!(".{}.partial", tool.file_name()));
    std::fs::write(&partial, data)
        .map_err(|e| format!("cannot write {}: {e}", partial.display()))?;
    std::fs::rename(&partial, &target)
        .map_err(|e| format!("cannot write {}: {e}", target.display()))?;
    Ok(target)
}

/// Download and verify a pinned jar (ktlint, detekt) unless it is installed.
pub async fn install_jar(
    http: &Http,
    repository: &str,
    base: &Path,
    tool: &PinnedJar,
    on: &mut (dyn FnMut(Event) + Send),
) -> Result<PathBuf, String> {
    if let Some(p) = installed_jar(base, tool) {
        return Ok(p);
    }
    let d = tool.download(repository);
    on(Event::step(format!("Downloading {}…", d.url)));
    let label = format!("Downloading {} {}", tool.name, tool.version);
    let data = http
        .bytes(&d.url, |done, total| {
            on(Event::progress(&label, done, total))
        })
        .await?;
    let p = install_jar_bytes(&data, base, tool, &d)?;
    on(Event::step(format!(
        "Verified SHA-256 of {}.",
        tool.file_name()
    )));
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed::archive::tests::{tar_gz, zip};
    use crate::managed::testserver;

    fn fake(data: &[u8], url: String) -> Download {
        Download {
            name: "kotlin-lsp".into(),
            version: "1.0.0".into(),
            url,
            sha256: sha256_hex(data),
            size: data.len() as u64,
        }
    }

    #[test]
    fn pinned_builds_cover_the_desktop_platforms() {
        for (os, arch) in [
            ("linux", "x86_64"),
            ("linux", "aarch64"),
            ("macos", "aarch64"),
            ("windows", "x86_64"),
        ] {
            let (d, _) = kotlin_lsp_release(os, arch).unwrap();
            assert_eq!(d.sha256.len(), 64);
            assert!(d.url.starts_with("https://download.jetbrains.com/language-server/kotlin-server/263.6379.0/kotlin-server-263.6379.0"));
        }
        assert_eq!(
            kotlin_lsp_release("linux", "x86_64").unwrap().1,
            ArchiveKind::TarGz
        );
        assert!(kotlin_lsp_release("freebsd", "x86_64").is_none());
        for jar in [KTLINT, DETEKT] {
            assert_eq!(jar.sha256.len(), 64);
            assert!(jar
                .download(DEFAULT_REPOSITORY)
                .url
                .starts_with("https://repo1.maven.org/maven2/"));
            assert!(jar.path.ends_with("-all.jar"));
        }
    }

    #[test]
    fn unpacks_verified_archives_only() {
        let data = tar_gz(&[
            (
                "kotlin-server-1.0.0/bin/intellij-server",
                b"#!/bin/sh\n",
                0o755,
            ),
            ("kotlin-server-1.0.0/build.txt", b"ILS-1.0.0", 0o644),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let d = fake(&data, "http://127.0.0.1:9/x".into());
        let inst = install_kotlin_lsp_archive(&data, ArchiveKind::TarGz, dir.path(), &d).unwrap();
        assert_eq!(inst.launcher, dir.path().join("1.0.0/bin/intellij-server"));
        assert_eq!(installed_kotlin_lsp(dir.path()).unwrap().version, "1.0.0");
        assert_eq!(
            inspect_kotlin_lsp(&inst.launcher).unwrap().dir,
            dir.path().join("1.0.0")
        );

        let mut bad = d.clone();
        bad.sha256 = "00".repeat(32);
        let err =
            install_kotlin_lsp_archive(&data, ArchiveKind::TarGz, dir.path(), &bad).unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");

        let empty = zip(&[("kotlin-server-2/README", b"x")]);
        let d2 = fake(&empty, String::new());
        let err =
            install_kotlin_lsp_archive(&empty, ArchiveKind::Zip, dir.path(), &d2).unwrap_err();
        assert!(err.contains("intellij-server"), "{err}");
    }

    #[tokio::test]
    async fn installs_jars_from_the_repository() {
        let server = testserver::start().await;
        let jar = b"PK-ktlint".to_vec();
        let tool = PinnedJar {
            sha256: Box::leak(sha256_hex(&jar).into_boxed_str()),
            ..KTLINT
        };
        server.routes.set(&format!("/{}", tool.path), jar.clone());
        let dir = tempfile::tempdir().unwrap();
        let http = Http::direct().unwrap();
        let p = install_jar(&http, &server.url, dir.path(), &tool, &mut |_| {})
            .await
            .unwrap();
        assert_eq!(p, dir.path().join("ktlint-1.8.0.jar"));
        assert_eq!(installed_jar(dir.path(), &tool), Some(p));
        // Installed: no second download.
        install_jar(&http, &server.url, dir.path(), &tool, &mut |_| {})
            .await
            .unwrap();
        assert_eq!(server.routes.hits(&format!("/{}", tool.path)), 1);
        let wrong = PinnedJar {
            name: "detekt",
            ..tool
        };
        let wrong = PinnedJar {
            sha256: KTLINT.sha256,
            ..wrong
        };
        let err = install_jar(&http, &server.url, dir.path(), &wrong, &mut |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("checksum mismatch"));
    }
}
