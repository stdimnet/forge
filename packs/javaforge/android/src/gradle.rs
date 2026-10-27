//! Gradle Android modules: the Android Gradle Plugin (`com.android.application` /
//! `com.android.library`, also as version catalog aliases and `apply plugin:`), the
//! `namespace`, `applicationId`, SDK levels and the AGP version.

use regex_lite::Regex;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ModuleKind {
    Application,
    Library,
}

/// An Android module as its build file declares it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AndroidModule {
    pub kind: ModuleKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_sdk: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_sdk: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compile_sdk: Option<String>,
    /// Kotlin Android (`org.jetbrains.kotlin.android`, `kotlin("android")`).
    pub kotlin: bool,
}

impl AndroidModule {
    /// The package the app is installed as: `applicationId`, else the namespace.
    pub fn package(&self) -> Option<&str> {
        self.application_id.as_deref().or(self.namespace.as_deref())
    }
}

/// Which Android Gradle plugin a build file applies.
pub fn plugin_kind(text: &str) -> Option<ModuleKind> {
    let has = |ids: &[&str]| ids.iter().any(|id| text.contains(id));
    if has(&[
        "com.android.application",
        "android.application)",
        "android-application)",
        "androidApplication)",
    ]) {
        Some(ModuleKind::Application)
    } else if has(&[
        "com.android.library",
        "android.library)",
        "android-library)",
        "androidLibrary)",
    ]) {
        Some(ModuleKind::Library)
    } else {
        None
    }
}

fn value(text: &str, key: &str) -> Option<String> {
    // `namespace = "x"`, `namespace "x"`, `minSdk = 26`, `minSdkVersion 26`,
    // `compileSdk = 35`.
    let re = Regex::new(&format!(
        r#"(?m)^\s*{key}\s*(?:=\s*|\(\s*|\s+)["']?([\w.\-]+)["']?"#
    ))
    .ok()?;
    re.captures(text).map(|c| c[1].to_string())
}

/// The Android module a build file (`build.gradle(.kts)`) declares, if it applies AGP.
pub fn android_module(text: &str) -> Option<AndroidModule> {
    let kind = plugin_kind(text)?;
    let first = |keys: &[&str]| keys.iter().find_map(|k| value(text, k));
    Some(AndroidModule {
        kind,
        namespace: value(text, "namespace"),
        application_id: value(text, "applicationId"),
        min_sdk: first(&["minSdk", "minSdkVersion"]),
        target_sdk: first(&["targetSdk", "targetSdkVersion"]),
        compile_sdk: first(&["compileSdk", "compileSdkVersion"]),
        kotlin: text.contains("org.jetbrains.kotlin.android")
            || text.contains("kotlin(\"android\")")
            || text.contains("kotlin.android)")
            || text.contains("kotlin-android"),
    })
}

/// The AGP version: `agp = "8.7.3"` of `gradle/libs.versions.toml`, or the version of the
/// plugin in a build file (`id("com.android.application") version "8.7.3"`,
/// `com.android.tools.build:gradle:8.7.3`).
pub fn agp_version(catalog: Option<&str>, builds: &[&str]) -> Option<String> {
    if let Some(c) = catalog {
        let re = Regex::new(r#"(?m)^\s*(?:agp|androidGradlePlugin|android-gradle-plugin|androidGradle)\s*=\s*"([^"]+)""#).ok()?;
        if let Some(v) = re.captures(c) {
            return Some(v[1].to_string());
        }
    }
    let plugin =
        Regex::new(r#"com\.android\.(?:application|library)["']\)?\s+version\s+["']([^"']+)["']"#)
            .ok()?;
    let classpath = Regex::new(r#"com\.android\.tools\.build:gradle:([\w.\-]+)"#).ok()?;
    builds.iter().find_map(|b| {
        plugin
            .captures(b)
            .or_else(|| classpath.captures(b))
            .map(|c| c[1].to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kotlin_dsl_app_module() {
        let m = android_module(include_str!("../tests/fixtures/app.build.gradle.kts")).unwrap();
        assert_eq!(m.kind, ModuleKind::Application);
        assert_eq!(m.namespace.as_deref(), Some("com.acme.notes"));
        assert_eq!(m.application_id.as_deref(), Some("com.acme.notes"));
        assert_eq!(
            (
                m.min_sdk.as_deref(),
                m.target_sdk.as_deref(),
                m.compile_sdk.as_deref()
            ),
            (Some("26"), Some("35"), Some("35"))
        );
        assert!(m.kotlin);
        assert_eq!(m.package(), Some("com.acme.notes"));
    }

    #[test]
    fn groovy_dsl_library_and_versions() {
        let groovy = "apply plugin: 'com.android.library'\nandroid {\n    namespace 'com.acme.core'\n    compileSdkVersion 34\n    defaultConfig {\n        minSdkVersion 24\n    }\n}\n";
        let m = android_module(groovy).unwrap();
        assert_eq!(m.kind, ModuleKind::Library);
        assert_eq!(m.namespace.as_deref(), Some("com.acme.core"));
        assert_eq!(m.compile_sdk.as_deref(), Some("34"));
        assert_eq!(m.min_sdk.as_deref(), Some("24"));
        assert_eq!(m.package(), Some("com.acme.core"));
        assert!(android_module("plugins { java }").is_none());
        assert_eq!(
            plugin_kind("plugins {\n    alias(libs.plugins.android.application)\n}"),
            Some(ModuleKind::Application)
        );
        assert_eq!(
            agp_version(
                Some("[versions]\nagp = \"8.7.3\"\nkotlin = \"2.1.0\"\n"),
                &[]
            )
            .as_deref(),
            Some("8.7.3")
        );
        assert_eq!(
            agp_version(None, &["plugins {\n    id(\"com.android.application\") version \"8.6.1\" apply false\n}"]).as_deref(),
            Some("8.6.1")
        );
        assert_eq!(
            agp_version(None, &["classpath 'com.android.tools.build:gradle:7.4.2'"]).as_deref(),
            Some("7.4.2")
        );
    }
}
