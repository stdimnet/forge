//! Microsoft's java-debug (EPL-1.0): the `com.microsoft.java.debug.plugin` jar that JDT LS
//! loads as a bundle and that serves the Debug Adapter Protocol for JavaForge. Installed
//! with the user's consent from Maven Central, where Microsoft publishes it: the newest
//! release named by `maven-metadata.xml`, checked against the repository's checksum
//! (`.sha256` when published, else `.sha1`), into `<data dir>/forge/langservers/java-debug`.

use std::path::{Path, PathBuf};

use sha1::{Digest, Sha1};

use super::node::sha256_hex;
use super::{Event, Http};

/// Maven Central (setting `java.debug.repository`).
pub const DEFAULT_REPOSITORY: &str = "https://repo1.maven.org/maven2";
const ARTIFACT: &str = "com.microsoft.java.debug.plugin";
const GROUP_PATH: &str = "com/microsoft/java";
const CURRENT: &str = "current";

/// `<data dir>/forge/langservers/java-debug`.
pub fn default_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("forge").join("langservers").join("java-debug"))
}

/// The release a `maven-metadata.xml` names (`<release>`, else `<latest>`).
pub fn parse_metadata(xml: &str) -> Option<String> {
    ["release", "latest"].iter().find_map(|tag| {
        let open = format!("<{tag}>");
        let start = xml.find(&open)? + open.len();
        let end = xml[start..].find('<')? + start;
        let v = xml[start..end].trim();
        (!v.is_empty()).then(|| v.to_string())
    })
}

pub fn jar_name(version: &str) -> String {
    format!("{ARTIFACT}-{version}.jar")
}

/// The jar Forge installed last.
pub fn installed(base: &Path) -> Option<PathBuf> {
    let version = std::fs::read_to_string(base.join(CURRENT)).ok()?;
    let jar = base.join(jar_name(version.trim()));
    jar.is_file().then_some(jar)
}

fn sha1_hex(data: &[u8]) -> String {
    Sha1::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Download and verify the newest java-debug plugin unless it is installed.
pub async fn install(
    http: &Http,
    repository: &str,
    base: &Path,
    on: &mut (dyn FnMut(Event) + Send),
) -> Result<PathBuf, String> {
    let dir = format!(
        "{}/{GROUP_PATH}/{ARTIFACT}",
        repository.trim_end_matches('/')
    );
    on(Event::step(format!(
        "Looking up java-debug on {repository}…"
    )));
    let metadata = http.text(&format!("{dir}/maven-metadata.xml")).await?;
    let version =
        parse_metadata(&metadata).ok_or("maven-metadata.xml names no java-debug release")?;
    let target = base.join(jar_name(&version));
    if target.is_file() {
        std::fs::write(base.join(CURRENT), &version)
            .map_err(|e| format!("cannot write {}: {e}", base.display()))?;
        on(Event::step(format!(
            "java-debug {version} is already installed."
        )));
        return Ok(target);
    }
    let url = format!("{dir}/{version}/{}", jar_name(&version));
    // Prefer SHA-256; Maven Central always has SHA-1.
    let (kind, expected) = match http.text(&format!("{url}.sha256")).await {
        Ok(s) => ("SHA-256", s),
        Err(_) => ("SHA-1", http.text(&format!("{url}.sha1")).await?),
    };
    let expected = expected
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let length = if kind == "SHA-256" { 64 } else { 40 };
    if expected.len() != length || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!(
            "{url}.{} holds no checksum",
            kind.to_lowercase().replace('-', "")
        ));
    }
    on(Event::step(format!("Downloading {url}…")));
    let label = format!("Downloading java-debug {version}");
    let data = http
        .bytes(&url, |done, total| on(Event::progress(&label, done, total)))
        .await?;
    let actual = if kind == "SHA-256" {
        sha256_hex(&data)
    } else {
        sha1_hex(&data)
    };
    if actual != expected {
        return Err(format!(
            "{}: {kind} mismatch (expected {expected}, got {actual}); the download was discarded",
            jar_name(&version)
        ));
    }
    on(Event::step(format!(
        "Verified {kind} of {}.",
        jar_name(&version)
    )));
    std::fs::create_dir_all(base).map_err(|e| format!("cannot create {}: {e}", base.display()))?;
    let partial = base.join(format!(".{}.partial", jar_name(&version)));
    std::fs::write(&partial, &data)
        .map_err(|e| format!("cannot write {}: {e}", partial.display()))?;
    std::fs::rename(&partial, &target)
        .map_err(|e| format!("cannot write {}: {e}", target.display()))?;
    std::fs::write(base.join(CURRENT), &version)
        .map_err(|e| format!("cannot write {}: {e}", base.display()))?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed::testserver;

    const DIR: &str = "/com/microsoft/java/com.microsoft.java.debug.plugin";

    fn serve(server: &testserver::TestServer, sha1: &str) {
        server.routes.set(
            &format!("{DIR}/maven-metadata.xml"),
            "<metadata><versioning><latest>0.53.1</latest><release>0.53.1</release></versioning></metadata>",
        );
        server.routes.set(
            &format!("{DIR}/0.53.1/com.microsoft.java.debug.plugin-0.53.1.jar"),
            b"PK-jar".to_vec(),
        );
        server.routes.set(
            &format!("{DIR}/0.53.1/com.microsoft.java.debug.plugin-0.53.1.jar.sha1"),
            sha1,
        );
    }

    #[test]
    fn metadata() {
        assert_eq!(
            parse_metadata("<latest>0.5</latest><release>0.4</release>").as_deref(),
            Some("0.4")
        );
        assert_eq!(
            parse_metadata("<latest>0.5</latest>").as_deref(),
            Some("0.5")
        );
        assert_eq!(parse_metadata("<metadata/>"), None);
    }

    #[tokio::test]
    async fn installs_with_the_sha1_of_maven_central() {
        let server = testserver::start().await;
        serve(&server, &sha1_hex(b"PK-jar"));
        let tmp = tempfile::tempdir().unwrap();
        let http = Http::direct().unwrap();
        let jar = install(&http, &server.url, tmp.path(), &mut |_| {})
            .await
            .unwrap();
        assert!(jar.ends_with("com.microsoft.java.debug.plugin-0.53.1.jar"));
        assert_eq!(installed(tmp.path()), Some(jar));
        assert_eq!(
            std::fs::read(installed(tmp.path()).unwrap()).unwrap(),
            b"PK-jar"
        );
    }

    #[tokio::test]
    async fn refuses_a_wrong_checksum() {
        let server = testserver::start().await;
        serve(&server, &"0".repeat(40));
        let tmp = tempfile::tempdir().unwrap();
        let http = Http::direct().unwrap();
        let err = install(&http, &server.url, tmp.path(), &mut |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("SHA-1 mismatch"), "{err}");
        assert!(installed(tmp.path()).is_none());
    }
}
