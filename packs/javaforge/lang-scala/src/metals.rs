//! Metals, the Scala language server (Apache-2.0, scalameta.org), like IntelliJ IDEA's Scala
//! plugin would use a BSP server: Forge starts `metals` from its settings, from its own
//! install or from the PATH. It never downloads Metals by itself; with the user's consent it
//! installs the pinned version with Coursier (`cs bootstrap`, which checks the artifacts'
//! checksums from Maven Central) into Forge's data directory.

use serde_json::{json, Value};

/// The Metals version Forge installs.
pub const METALS_VERSION: &str = "1.5.3";
/// Metals 1.x is published for Scala 2.13.
pub const METALS_ARTIFACT: &str = "org.scalameta:metals_2.13";
/// The language server id (also `forge_trust::SERVERS_RUNNING_PROJECT_CODE`'s: Metals
/// imports the build with sbt or Bloop).
pub const SERVER_ID: &str = "metals";

/// The Coursier coordinates of a Metals version.
pub fn coordinates(version: &str) -> String {
    format!("{METALS_ARTIFACT}:{version}")
}

/// `cs` arguments that write the launcher of `version` to `output` (a file in Forge's data
/// directory), with the JVM options Metals recommends.
pub fn bootstrap_args(version: &str, output: &str) -> Vec<String> {
    vec![
        "bootstrap".into(),
        "--java-opt".into(),
        "-Xss4m".into(),
        "--java-opt".into(),
        "-Xms100m".into(),
        coordinates(version),
        "-o".into(),
        output.into(),
        "-f".into(),
    ]
}

/// The `initializationOptions` Forge sends: no HTTP client, status and slow task messages
/// as notifications the shared LSP client shows.
pub fn initialization_options() -> Value {
    json!({
        "isHttpEnabled": false,
        "statusBarProvider": "show-message",
        "inputBoxProvider": false,
        "quickPickProvider": false,
        "executeClientCommandProvider": false,
        "treeViewProvider": false,
        "doctorProvider": "json",
        "compilerOptions": { "snippetAutoIndent": false }
    })
}

/// The `metals` workspace settings (`workspace/didChangeConfiguration`).
pub fn settings(java_home: Option<&str>, sbt_script: Option<&str>) -> Value {
    let mut metals = json!({ "showInferredType": true, "superMethodLensesEnabled": false });
    if let Some(h) = java_home.filter(|h| !h.is_empty()) {
        metals["javaHome"] = json!(h);
    }
    if let Some(s) = sbt_script.filter(|s| !s.is_empty()) {
        metals["sbtScript"] = json!(s);
    }
    json!({ "metals": metals })
}

/// The version of a Metals launcher's folder name (`…/metals/1.5.3/metals`).
pub fn version_of_install(path: &str) -> Option<String> {
    let p = path.replace('\\', "/");
    let mut parts = p.rsplit('/');
    parts.next()?;
    let v = parts.next()?;
    v.chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit())
        .then(|| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_and_options() {
        let a = bootstrap_args(
            METALS_VERSION,
            "/data/forge/langservers/metals/1.5.3/metals",
        );
        assert_eq!(a[0], "bootstrap");
        assert!(a.contains(&"org.scalameta:metals_2.13:1.5.3".to_string()));
        assert_eq!(
            a[a.len() - 2],
            "/data/forge/langservers/metals/1.5.3/metals"
        );
        assert_eq!(initialization_options()["isHttpEnabled"], false);
        let s = settings(Some("/jdk/21"), None);
        assert_eq!(s["metals"]["javaHome"], "/jdk/21");
        assert!(s["metals"].get("sbtScript").is_none());
        assert_eq!(
            version_of_install("/data/forge/langservers/metals/1.5.3/metals").as_deref(),
            Some("1.5.3")
        );
        assert_eq!(version_of_install("/usr/bin/metals"), None);
    }
}
