//! How Forge starts Eclipse JDT LS: the `java` command line (like the official `jdtls`
//! launcher script, with the installation read-only and the OSGi configuration and
//! workspace data in Forge's cache directory), its initialization options (java-debug as
//! a bundle, extended client capabilities) and the `java.*` settings (JDKs as runtimes,
//! Maven / Gradle import, formatting, hot code replace).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// A JDK as JDT LS sees it (`java.configuration.runtimes`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Runtime {
    /// Java feature release (`8`, `17`, `21`).
    pub major: u32,
    /// The JDK's home.
    pub path: String,
    /// The project JDK.
    #[serde(default)]
    pub default: bool,
}

/// The execution environment name of a release: `JavaSE-1.8`, `JavaSE-21`.
pub fn execution_environment(major: u32) -> String {
    if major <= 8 {
        format!("JavaSE-1.{major}")
    } else {
        format!("JavaSE-{major}")
    }
}

/// Everything JDT LS needs for one project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JdtlsSetup {
    /// `java` that runs the server (a JDK 21 or newer).
    pub java: String,
    /// `plugins/org.eclipse.equinox.launcher_*.jar` of the installation.
    pub launcher_jar: String,
    /// The installation's `config_<os>` folder (read-only, shared).
    pub shared_config: String,
    /// Writable OSGi configuration in the user cache directory.
    pub config_dir: String,
    /// Workspace data in the user cache directory (never inside the project).
    pub data_dir: String,
    /// Extra VM options (`java.jdtls.vmOptions`, default `-Xmx1G`).
    pub vm_options: Vec<String>,
    /// Plugin jars loaded into the server (java-debug).
    pub bundles: Vec<String>,
    pub runtimes: Vec<Runtime>,
    /// Use the Gradle / Maven wrapper of the project.
    pub use_wrapper: bool,
    /// The JDK Gradle runs with during the import (`java.import.gradle.java.home`).
    pub gradle_java_home: Option<String>,
    /// `java.debug.settings.hotCodeReplace`: `auto`, `manual` or `never`.
    pub hot_code_replace: String,
    /// Download sources of Maven dependencies.
    pub download_sources: bool,
    /// Formatter profile (Eclipse XML), optional.
    pub formatter_url: Option<String>,
}

impl JdtlsSetup {
    /// The server's command line after `java`.
    pub fn args(&self) -> Vec<String> {
        let mut args: Vec<String> = [
            "-Declipse.application=org.eclipse.jdt.ls.core.id1",
            "-Dosgi.bundles.defaultStartLevel=4",
            "-Declipse.product=org.eclipse.jdt.ls.core.product",
            "-Dosgi.checkConfiguration=true",
            "-Dosgi.configuration.cascaded=true",
            "-Dosgi.sharedConfiguration.area.readOnly=true",
            "-Dlog.protocol=false",
            "-Dlog.level=ERROR",
        ]
        .map(String::from)
        .into();
        args.push(format!(
            "-Dosgi.sharedConfiguration.area={}",
            self.shared_config
        ));
        if self.vm_options.is_empty() {
            args.push("-Xmx1G".into());
        } else {
            args.extend(self.vm_options.iter().cloned());
        }
        args.extend(
            [
                "--add-modules=ALL-SYSTEM",
                "--add-opens",
                "java.base/java.util=ALL-UNNAMED",
                "--add-opens",
                "java.base/java.lang=ALL-UNNAMED",
                "-jar",
            ]
            .map(String::from),
        );
        args.push(self.launcher_jar.clone());
        args.push("-configuration".into());
        args.push(self.config_dir.clone());
        args.push("-data".into());
        args.push(self.data_dir.clone());
        args
    }

    /// The `java` settings tree (`workspace/configuration`, `didChangeConfiguration`).
    pub fn settings(&self) -> Value {
        let runtimes: Vec<Value> = self
            .runtimes
            .iter()
            .map(|r| {
                let mut v = json!({ "name": execution_environment(r.major), "path": r.path });
                if r.default {
                    v["default"] = json!(true);
                }
                v
            })
            .collect();
        let project_jdk = self
            .runtimes
            .iter()
            .find(|r| r.default)
            .map(|r| r.path.clone());
        let hcr = match self.hot_code_replace.as_str() {
            "" => "auto",
            other => other,
        };
        let mut java = json!({
            "configuration": {
                "runtimes": runtimes,
                "updateBuildConfiguration": "automatic",
            },
            "import": {
                "gradle": {
                    "enabled": true,
                    "wrapper": { "enabled": self.use_wrapper },
                },
                "maven": { "enabled": true },
                "exclusions": ["**/node_modules/**", "**/.metadata/**", "**/archetype-resources/**", "**/META-INF/maven/**"],
            },
            "maven": { "downloadSources": self.download_sources },
            "autobuild": { "enabled": true },
            "format": { "enabled": true },
            "signatureHelp": { "enabled": true },
            "contentProvider": { "preferred": "fernflower" },
            "completion": { "enabled": true, "guessMethodArguments": true },
            "sources": { "organizeImports": { "starThreshold": 99, "staticStarThreshold": 99 } },
            "debug": { "settings": { "hotCodeReplace": hcr } },
        });
        if let Some(home) = self.gradle_java_home.clone().or(project_jdk) {
            java["import"]["gradle"]["java"] = json!({ "home": home });
        }
        if let Some(url) = &self.formatter_url {
            java["format"]["settings"] = json!({ "url": url });
        }
        json!({ "java": java })
    }

    /// `initializationOptions` of the `initialize` request.
    pub fn initialization_options(&self) -> Value {
        json!({
            "bundles": self.bundles,
            "settings": self.settings(),
            "extendedClientCapabilities": {
                "progressReportProvider": false,
                "classFileContentsSupport": true,
                "overrideMethodsPromptSupport": false,
                "hashCodeEqualsPromptSupport": false,
                "advancedOrganizeImportsSupport": false,
                "generateToStringPromptSupport": false,
                "advancedGenerateAccessorsSupport": false,
                "generateConstructorsPromptSupport": false,
                "generateDelegateMethodsPromptSupport": false,
                "advancedExtractRefactoringSupport": false,
                "moveRefactoringSupport": false,
                "clientHoverProvider": false,
                "clientDocumentSymbolProvider": false,
                "gradleChecksumWrapperPromptSupport": false,
                "resolveAdditionalTextEditsSupport": true,
                "actionableRuntimeNotificationSupport": false,
                "shouldLanguageServerExitOnShutdown": true,
                "onCompletionItemSelectedCommand": "",
            },
        })
    }
}

/// The OSGi configuration folder of a platform in a JDT LS installation (`config_linux`,
/// `config_mac_arm`, `config_win`).
pub fn config_folder(os: &str, arch: &str) -> &'static str {
    let arm = matches!(arch, "aarch64" | "arm64");
    match (os, arm) {
        ("macos", true) => "config_mac_arm",
        ("macos", false) => "config_mac",
        ("windows", _) => "config_win",
        (_, true) => "config_linux_arm",
        _ => "config_linux",
    }
}

/// JDT LS commands Forge sends (`workspace/executeCommand`) and notifications.
pub mod commands {
    /// java-debug: start a DAP server inside JDT LS; returns its port.
    pub const START_DEBUG_SESSION: &str = "vscode.java.startDebugSession";
    /// java-debug: `[mainClass, projectName]` → `[modulepaths, classpaths]`.
    pub const RESOLVE_CLASSPATH: &str = "vscode.java.resolveClasspath";
    /// java-debug: the main classes of the workspace.
    pub const RESOLVE_MAIN_CLASS: &str = "vscode.java.resolveMainClass";
    /// java-debug: update settings such as `hotCodeReplace` (a JSON string).
    pub const UPDATE_DEBUG_SETTINGS: &str = "vscode.java.updateDebugSettings";
    /// Re-import a build file (Reload Project).
    pub const PROJECT_CONFIGURATION_UPDATE: &str = "java/projectConfigurationUpdate";
    /// Build the workspace (`true` = full rebuild).
    pub const BUILD_WORKSPACE: &str = "java/buildWorkspace";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> JdtlsSetup {
        JdtlsSetup {
            java: "/jdk21/bin/java".into(),
            launcher_jar: "/data/jdtls/1.40.0/plugins/org.eclipse.equinox.launcher_1.6.900.jar"
                .into(),
            shared_config: "/data/jdtls/1.40.0/config_linux".into(),
            config_dir: "/cache/forge/jdtls/config".into(),
            data_dir: "/cache/forge/jdtls/shop-1a2b3c4d".into(),
            bundles: vec!["/data/java-debug/com.microsoft.java.debug.plugin-0.53.1.jar".into()],
            runtimes: vec![
                Runtime {
                    major: 8,
                    path: "/jdk8".into(),
                    default: false,
                },
                Runtime {
                    major: 21,
                    path: "/jdk21".into(),
                    default: true,
                },
            ],
            use_wrapper: true,
            ..Default::default()
        }
    }

    #[test]
    fn command_line_keeps_data_outside_the_project() {
        let args = setup().args();
        assert_eq!(args[0], "-Declipse.application=org.eclipse.jdt.ls.core.id1");
        assert!(args.contains(
            &"-Dosgi.sharedConfiguration.area=/data/jdtls/1.40.0/config_linux".to_string()
        ));
        assert!(args.contains(&"-Xmx1G".to_string()));
        let tail = &args[args.len() - 5..];
        assert_eq!(
            tail,
            [
                "/data/jdtls/1.40.0/plugins/org.eclipse.equinox.launcher_1.6.900.jar",
                "-configuration",
                "/cache/forge/jdtls/config",
                "-data",
                "/cache/forge/jdtls/shop-1a2b3c4d"
            ]
        );
        let custom = JdtlsSetup {
            vm_options: vec!["-Xmx4G".into()],
            ..setup()
        };
        assert!(!custom.args().contains(&"-Xmx1G".to_string()));
    }

    #[test]
    fn settings_and_initialization_options() {
        let s = setup().settings();
        let runtimes = &s["java"]["configuration"]["runtimes"];
        assert_eq!(runtimes[0]["name"], "JavaSE-1.8");
        assert_eq!(runtimes[1]["name"], "JavaSE-21");
        assert_eq!(runtimes[1]["default"], true);
        assert_eq!(s["java"]["import"]["gradle"]["java"]["home"], "/jdk21");
        assert_eq!(s["java"]["import"]["gradle"]["wrapper"]["enabled"], true);
        assert_eq!(s["java"]["debug"]["settings"]["hotCodeReplace"], "auto");
        let init = setup().initialization_options();
        assert_eq!(
            init["bundles"][0],
            "/data/java-debug/com.microsoft.java.debug.plugin-0.53.1.jar"
        );
        assert_eq!(
            init["extendedClientCapabilities"]["shouldLanguageServerExitOnShutdown"],
            true
        );
        assert_eq!(config_folder("linux", "x86_64"), "config_linux");
        assert_eq!(config_folder("macos", "aarch64"), "config_mac_arm");
        assert_eq!(config_folder("windows", "x86_64"), "config_win");
    }
}
