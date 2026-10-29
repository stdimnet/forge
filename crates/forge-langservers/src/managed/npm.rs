//! npm packages without npm: registry metadata, tarball download with `dist.integrity`
//! (sha512) verification, unpacking into `<managed dir>/node_modules/<name>` and the
//! `node_modules/.bin` entries detection looks for. Packages are also recorded in the
//! managed directory's `package.json`, so a later `npm install --prefix` keeps them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha512};

use super::archive;
use super::http::Http;
use super::Event;

pub const DEFAULT_REGISTRY: &str = "https://registry.npmjs.org";

/// `name` or `name@version` (also for `@scope/name@version`).
pub fn parse_spec(spec: &str) -> (String, Option<String>) {
    let (scope, rest) = match spec.strip_prefix('@') {
        Some(rest) => ("@", rest),
        None => ("", spec),
    };
    match rest.split_once('@') {
        Some((name, version)) if !version.is_empty() => {
            (format!("{scope}{name}"), Some(version.to_string()))
        }
        _ => (format!("{scope}{}", rest.trim_end_matches('@')), None),
    }
}

/// `<registry>/<name>/<version or latest>`; scoped names are escaped (`@vue%2Fserver`).
pub fn metadata_url(registry: &str, name: &str, version: Option<&str>) -> String {
    format!(
        "{}/{}/{}",
        registry.trim_end_matches('/'),
        name.replace('/', "%2F"),
        version.unwrap_or("latest")
    )
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dist {
    pub tarball: String,
    #[serde(default)]
    pub integrity: Option<String>,
}

/// One version of a package as the registry describes it.
#[derive(Debug, Clone, Deserialize)]
pub struct PackageMeta {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
    pub dist: Dist,
}

impl PackageMeta {
    /// Packages without runtime dependencies are unpacked directly; others need npm.
    pub fn standalone(&self) -> bool {
        self.dependencies.is_empty()
    }
}

/// Check `data` against an SRI string (`sha512-<base64>`, several may be listed).
pub fn verify_integrity(data: &[u8], integrity: &str) -> Result<(), String> {
    let wanted: Vec<&str> = integrity
        .split_whitespace()
        .filter_map(|i| i.strip_prefix("sha512-"))
        .collect();
    if wanted.is_empty() {
        return Err(format!(
            "unsupported integrity {integrity:?} (sha512 needed)"
        ));
    }
    let actual = base64::engine::general_purpose::STANDARD.encode(Sha512::digest(data));
    if wanted.iter().any(|w| *w == actual) {
        Ok(())
    } else {
        Err(format!(
            "integrity check failed: expected sha512-{}, got sha512-{actual}. The download was discarded.",
            wanted[0]
        ))
    }
}

/// `bin` entries of a `package.json`: `(command, script path relative to the package)`.
pub fn bins(package_json: &Value) -> Vec<(String, String)> {
    let name = package_json
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match package_json.get("bin") {
        Some(Value::String(path)) => {
            let command = name.rsplit('/').next().unwrap_or(name);
            vec![(
                command.to_string(),
                path.trim_start_matches("./").to_string(),
            )]
        }
        Some(Value::Object(map)) => map
            .iter()
            .filter_map(|(k, v)| {
                Some((k.clone(), v.as_str()?.trim_start_matches("./").to_string()))
            })
            .collect(),
        _ => vec![],
    }
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Version of `name` installed in the managed directory.
pub fn installed_version(managed: &Path, name: &str) -> Option<String> {
    read_json(&managed.join("node_modules").join(name).join("package.json"))?
        .get("version")?
        .as_str()
        .map(str::to_owned)
}

/// Packages recorded in the managed directory's `package.json`.
pub fn recorded_packages(managed: &Path) -> Vec<String> {
    read_json(&managed.join("package.json"))
        .and_then(|v| v.get("dependencies")?.as_object().cloned())
        .map(|deps| deps.keys().cloned().collect())
        .unwrap_or_default()
}

fn record(managed: &Path, name: &str, version: &str) -> Result<(), String> {
    let path = managed.join("package.json");
    let mut root = read_json(&path).unwrap_or_else(|| json!({}));
    if !root.is_object() {
        root = json!({});
    }
    let deps = root
        .as_object_mut()
        .expect("object")
        .entry("dependencies")
        .or_insert_with(|| json!({}));
    if !deps.is_object() {
        *deps = json!({});
    }
    deps[name] = json!(format!("^{version}"));
    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    std::fs::write(&path, text + "\n").map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// The packages in `node_modules` (`name` and `@scope/name`).
fn packages(node_modules: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(node_modules) else {
        return vec![];
    };
    let mut out = Vec::new();
    for e in entries.filter_map(|e| e.ok()) {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || !p.is_dir() {
            continue;
        }
        if name.starts_with('@') {
            if let Ok(scoped) = std::fs::read_dir(&p) {
                out.extend(scoped.filter_map(|s| s.ok()).map(|s| s.path()));
            }
        } else {
            out.push(p);
        }
    }
    out.sort();
    out
}

/// The script behind `command` among the managed packages
/// (`vls` → `<managed>/node_modules/vls/lib/server.js`).
pub fn find_bin(managed: &Path, command: &str) -> Option<PathBuf> {
    packages(&managed.join("node_modules"))
        .into_iter()
        .find_map(|dir| {
            let pkg = read_json(&dir.join("package.json"))?;
            let (_, script) = bins(&pkg).into_iter().find(|(c, _)| c == command)?;
            let path = dir.join(script);
            path.is_file().then_some(path)
        })
}

/// Create `node_modules/.bin/<command>` for each bin of the package in `dir`: a symlink on
/// Unix, a `.cmd` shim on Windows. Detection finds servers there; Forge itself starts the
/// script with its Node.js (see [`crate::Detector::launch`]).
fn link_bins(node_modules: &Path, dir: &Path, package_json: &Value) -> Result<(), String> {
    let bin_dir = node_modules.join(".bin");
    std::fs::create_dir_all(&bin_dir)
        .map_err(|e| format!("cannot create {}: {e}", bin_dir.display()))?;
    let rel_pkg = dir.strip_prefix(node_modules).unwrap_or(dir);
    for (command, script) in bins(package_json) {
        let script_path = dir.join(&script);
        if !script_path.is_file() || command.contains(['/', '\\']) {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755));
            let link = bin_dir.join(&command);
            let _ = std::fs::remove_file(&link);
            let target = Path::new("..").join(rel_pkg).join(&script);
            std::os::unix::fs::symlink(&target, &link)
                .map_err(|e| format!("cannot link {}: {e}", link.display()))?;
        }
        #[cfg(not(unix))]
        {
            let rel = rel_pkg
                .join(&script)
                .display()
                .to_string()
                .replace('/', "\\");
            let shim = format!("@node \"%~dp0\\..\\{rel}\" %*\r\n");
            let path = bin_dir.join(format!("{command}.cmd"));
            std::fs::write(&path, shim)
                .map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        }
    }
    Ok(())
}

/// Fetch the metadata of `spec` (`name` or `name@version`).
pub async fn metadata(http: &Http, registry: &str, spec: &str) -> Result<PackageMeta, String> {
    let (name, version) = parse_spec(spec);
    http.json(&metadata_url(registry, &name, version.as_deref()))
        .await
}

/// Download, verify and unpack a package without dependencies into `managed`.
pub async fn install_standalone(
    http: &Http,
    meta: &PackageMeta,
    managed: &Path,
    on: &mut (dyn FnMut(Event) + Send),
) -> Result<PathBuf, String> {
    let integrity = meta
        .dist
        .integrity
        .as_deref()
        .ok_or_else(|| format!("{}@{} has no integrity hash", meta.name, meta.version))?;
    on(Event::step(format!(
        "Downloading {}@{} from {}…",
        meta.name, meta.version, meta.dist.tarball
    )));
    let label = format!("Downloading {} {}", meta.name, meta.version);
    let data = http
        .bytes(&meta.dist.tarball, |done, total| {
            on(Event::progress(&label, done, total))
        })
        .await?;
    verify_integrity(&data, integrity)?;
    on(Event::step(format!(
        "Verified sha512 integrity of {}@{}.",
        meta.name, meta.version
    )));

    let node_modules = managed.join("node_modules");
    let dest = node_modules.join(&meta.name);
    let staging = node_modules.join(format!(".{}.partial", meta.name.replace('/', "+")));
    let _ = std::fs::remove_dir_all(&staging);
    archive::extract_tar_gz(&data, &staging, 1)?;
    let package_json = read_json(&staging.join("package.json"))
        .ok_or_else(|| format!("{} has no package.json", meta.dist.tarball))?;
    let _ = std::fs::remove_dir_all(&dest);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    std::fs::rename(&staging, &dest)
        .map_err(|e| format!("cannot move {} into place: {e}", meta.name))?;
    link_bins(&node_modules, &dest, &package_json)?;
    record(managed, &meta.name, &meta.version)?;
    on(Event::step(format!(
        "Installed {}@{} into {}.",
        meta.name,
        meta.version,
        dest.display()
    )));
    Ok(dest)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::managed::archive::tests::tar_gz;
    use crate::managed::testserver;

    pub fn sri(data: &[u8]) -> String {
        format!(
            "sha512-{}",
            base64::engine::general_purpose::STANDARD.encode(Sha512::digest(data))
        )
    }

    #[test]
    fn specs_and_urls() {
        assert_eq!(
            parse_spec("fake-language-server"),
            ("fake-language-server".into(), None)
        );
        assert_eq!(
            parse_spec("fake-language-server@1.12.6"),
            ("fake-language-server".into(), Some("1.12.6".into()))
        );
        assert_eq!(
            parse_spec("@vue/language-server@2.1.0"),
            ("@vue/language-server".into(), Some("2.1.0".into()))
        );
        assert_eq!(
            parse_spec("@vue/language-server"),
            ("@vue/language-server".into(), None)
        );
        assert_eq!(
            metadata_url("https://registry.npmjs.org/", "@vue/language-server", None),
            "https://registry.npmjs.org/@vue%2Flanguage-server/latest"
        );
        assert_eq!(
            metadata_url("http://r", "fake-language-server", Some("1.0.0")),
            "http://r/fake-language-server/1.0.0"
        );
    }

    #[test]
    fn integrity_good_and_tampered() {
        let data = b"tarball bytes";
        verify_integrity(data, &sri(data)).unwrap();
        verify_integrity(data, &format!("sha1-abc {}", sri(data))).unwrap();
        let err = verify_integrity(b"tarball bytez", &sri(data)).unwrap_err();
        assert!(err.contains("integrity check failed"), "{err}");
        assert!(verify_integrity(data, "sha1-abc")
            .unwrap_err()
            .contains("unsupported"));
    }

    #[test]
    fn bin_entries() {
        let one = json!({"name": "@scope/tool", "bin": "cli.js"});
        assert_eq!(bins(&one), vec![("tool".into(), "cli.js".into())]);
        let many = json!({"name": "x", "bin": {"a": "a.js", "b": "bin/b.js"}});
        assert_eq!(bins(&many).len(), 2);
        assert!(bins(&json!({"name": "lib"})).is_empty());
    }

    pub const PACKAGE_JSON: &str = r#"{"name":"fake-language-server","version":"1.12.6","bin":{"fake-language-server":"lib/server.js"}}"#;

    /// A fake standalone server tarball and its registry metadata served under `/fake-language-server/…`.
    pub fn fake_package(server: &testserver::TestServer, version: &str, tamper: bool) {
        let package_json = PACKAGE_JSON.replace("1.12.6", version);
        let tgz = tar_gz(&[
            ("package/package.json", package_json.as_bytes(), 0o644),
            ("package/LICENSE.txt", b"licence", 0o644),
            ("package/lib/server.js", b"#!/usr/bin/env node\n", 0o644),
        ]);
        let integrity = sri(&tgz);
        let mut served = tgz;
        if tamper {
            served.push(0);
        }
        let tarball = format!("/fake-language-server/-/fake-language-server-{version}.tgz");
        let meta = json!({
            "name": "fake-language-server",
            "version": version,
            "dependencies": {},
            "dist": { "tarball": format!("{}{tarball}", server.url), "integrity": integrity },
        });
        server.routes.set(&tarball, served);
        server.routes.set(
            "/fake-language-server/latest",
            meta.to_string().into_bytes(),
        );
        server.routes.set(
            &format!("/fake-language-server/{version}"),
            meta.to_string().into_bytes(),
        );
    }

    #[tokio::test]
    async fn installs_a_standalone_package_with_bins_and_records_it() {
        let server = testserver::start().await;
        fake_package(&server, "1.12.6", false);
        let tmp = tempfile::tempdir().unwrap();
        let managed = tmp.path().join("langservers");
        let http = Http::direct().unwrap();
        let meta = metadata(&http, &server.url, "fake-language-server")
            .await
            .unwrap();
        assert!(meta.standalone());
        install_standalone(&http, &meta, &managed, &mut |_| {})
            .await
            .unwrap();

        let script = managed.join("node_modules/fake-language-server/lib/server.js");
        assert!(script.is_file());
        assert_eq!(
            installed_version(&managed, "fake-language-server").as_deref(),
            Some("1.12.6")
        );
        assert_eq!(
            find_bin(&managed, "fake-language-server"),
            Some(script.clone())
        );
        assert_eq!(recorded_packages(&managed), vec!["fake-language-server"]);
        #[cfg(unix)]
        {
            let link = managed.join("node_modules/.bin/fake-language-server");
            assert_eq!(
                std::fs::canonicalize(&link).unwrap(),
                std::fs::canonicalize(&script).unwrap()
            );
        }
        // The pinned version is fetched by its own URL.
        metadata(&http, &server.url, "fake-language-server@1.12.6")
            .await
            .unwrap();
        assert_eq!(server.routes.hits("/fake-language-server/1.12.6"), 1);
    }

    #[tokio::test]
    async fn a_tampered_tarball_is_rejected_and_nothing_is_installed() {
        let server = testserver::start().await;
        fake_package(&server, "1.12.6", true);
        let tmp = tempfile::tempdir().unwrap();
        let managed = tmp.path().join("langservers");
        let http = Http::direct().unwrap();
        let meta = metadata(&http, &server.url, "fake-language-server")
            .await
            .unwrap();
        let err = install_standalone(&http, &meta, &managed, &mut |_| {})
            .await
            .unwrap_err();
        assert!(err.contains("integrity check failed"), "{err}");
        assert!(!managed.join("node_modules/fake-language-server").exists());
        assert!(find_bin(&managed, "fake-language-server").is_none());
    }
}
