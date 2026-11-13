//! How KotlinForge starts its language server: JetBrains' kotlin-lsp (`bin/intellij-server
//! --stdio` of the standalone build, with its bundled runtime) or, as an option, fwcd's
//! kotlin-language-server. Both import the Gradle / Maven build with the project JDK
//! (`JAVA_HOME`); kotlin-lsp also takes JVM options through `IJ_JAVA_OPTIONS`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The servers KotlinForge can start (`kotlin.lsp.server`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Server {
    /// JetBrains' kotlin-lsp (`kotlin-lsp`).
    KotlinLsp,
    /// fwcd's kotlin-language-server (`kotlin-language-server`).
    KotlinLanguageServer,
}

impl Server {
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "kotlin-language-server" | "fwcd" => Server::KotlinLanguageServer,
            _ => Server::KotlinLsp,
        }
    }

    /// The id the LSP manager, the trust check and the settings use.
    pub fn id(self) -> &'static str {
        match self {
            Server::KotlinLsp => "kotlin-lsp",
            Server::KotlinLanguageServer => "kotlin-language-server",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Server::KotlinLsp => "Kotlin LSP",
            Server::KotlinLanguageServer => "Kotlin Language Server",
        }
    }
}

/// Everything needed to start a Kotlin server for one project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KotlinServerSetup {
    /// The launcher (`…/bin/intellij-server`, `kotlin-language-server`).
    pub command: String,
    /// The project JDK (`JAVA_HOME` of the server and its build import).
    pub java_home: Option<String>,
    /// JVM options of the server (`-Xmx4g`).
    pub jvm_options: Vec<String>,
    /// Extra arguments (`kotlin.lsp.arguments`).
    pub arguments: Vec<String>,
}

impl KotlinServerSetup {
    pub fn args(&self, server: Server) -> Vec<String> {
        let mut a = Vec::new();
        if server == Server::KotlinLsp {
            a.push("--stdio".to_string());
        }
        a.extend(self.arguments.iter().cloned());
        a
    }

    pub fn env(&self, server: Server) -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        if let Some(home) = self.java_home.as_ref().filter(|h| !h.is_empty()) {
            env.insert("JAVA_HOME".to_string(), home.clone());
        }
        if !self.jvm_options.is_empty() {
            let key = match server {
                Server::KotlinLsp => "IJ_JAVA_OPTIONS",
                // The fwcd start script passes JAVA_OPTS to the JVM.
                Server::KotlinLanguageServer => "JAVA_OPTS",
            };
            env.insert(key.to_string(), self.jvm_options.join(" "));
        }
        env
    }

    /// `initializationOptions`: kotlin-lsp's code lens for `main` stays off (Forge draws its
    /// own Run gutter); fwcd's server gets its storage path in the cache directory.
    pub fn initialization_options(&self, server: Server, storage: Option<&str>) -> Value {
        match server {
            Server::KotlinLsp => json!({ "runMainCodeLens": false }),
            Server::KotlinLanguageServer => match storage {
                Some(s) => json!({ "storagePath": s }),
                None => Value::Null,
            },
        }
    }
}

/// The launcher inside an unpacked kotlin-lsp folder.
pub fn launcher_path(windows: bool) -> &'static str {
    if windows {
        "bin/intellij-server.exe"
    } else {
        "bin/intellij-server"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kotlin_lsp_and_fwcd_launch() {
        let s = KotlinServerSetup {
            command: "/data/forge/langservers/kotlin-lsp/263.6379.0/bin/intellij-server".into(),
            java_home: Some("/jdks/temurin-21".into()),
            jvm_options: vec!["-Xmx3g".into()],
            arguments: vec![],
        };
        assert_eq!(s.args(Server::KotlinLsp), ["--stdio"]);
        let env = s.env(Server::KotlinLsp);
        assert_eq!(env["JAVA_HOME"], "/jdks/temurin-21");
        assert_eq!(env["IJ_JAVA_OPTIONS"], "-Xmx3g");
        assert!(s.args(Server::KotlinLanguageServer).is_empty());
        assert_eq!(s.env(Server::KotlinLanguageServer)["JAVA_OPTS"], "-Xmx3g");
        assert_eq!(
            s.initialization_options(Server::KotlinLsp, None)["runMainCodeLens"],
            false
        );
        assert_eq!(
            s.initialization_options(Server::KotlinLanguageServer, Some("/cache/kls"))
                ["storagePath"],
            "/cache/kls"
        );
        assert_eq!(
            Server::parse("kotlin-language-server").id(),
            "kotlin-language-server"
        );
        assert_eq!(Server::parse("").id(), "kotlin-lsp");
        assert_eq!(launcher_path(true), "bin/intellij-server.exe");
    }
}
