//! A JDK's `release` file (`JAVA_VERSION="21.0.4"`, `IMPLEMENTOR="Eclipse Adoptium"`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    /// `JAVA_VERSION`: `21.0.4`, `1.8.0_412`.
    pub java_version: String,
    /// `IMPLEMENTOR`: `Eclipse Adoptium`, `Oracle Corporation`, `Azul Systems, Inc.`.
    pub implementor: String,
    /// `IMPLEMENTOR_VERSION`: `Temurin-21.0.4+7`, `Zulu21.36+17-CA`.
    pub implementor_version: String,
    /// `JAVA_RUNTIME_VERSION`: `21.0.4+7-LTS`.
    pub runtime_version: String,
    /// `OS_ARCH`: `x86_64`, `aarch64`.
    pub arch: String,
}

/// Parse `KEY="value"` lines.
pub fn parse_release(text: &str) -> ReleaseInfo {
    let mut info = ReleaseInfo::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').to_string();
        match key.trim() {
            "JAVA_VERSION" => info.java_version = value,
            "IMPLEMENTOR" => info.implementor = value,
            "IMPLEMENTOR_VERSION" => info.implementor_version = value,
            "JAVA_RUNTIME_VERSION" => info.runtime_version = value,
            "OS_ARCH" => info.arch = value,
            _ => {}
        }
    }
    info
}

/// The feature release of a version: `1.8.0_412` → 8, `21.0.4` → 21, `25` → 25,
/// `26-ea` → 26.
pub fn major_of(version: &str) -> u32 {
    let mut parts = version
        .trim()
        .split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty());
    match parts.next().and_then(|p| p.parse::<u32>().ok()) {
        Some(1) => parts.next().and_then(|p| p.parse().ok()).unwrap_or(1),
        Some(n) => n,
        None => 0,
    }
}

/// A short vendor name, as IntelliJ shows it: `Eclipse Temurin`, `Azul Zulu`, `Oracle
/// OpenJDK`, `Amazon Corretto`, `GraalVM`, …
pub fn vendor_name(info: &ReleaseInfo) -> String {
    let v = info.implementor_version.to_ascii_lowercase();
    let i = info.implementor.to_ascii_lowercase();
    let name = if v.contains("temurin") || i.contains("adoptium") {
        "Eclipse Temurin"
    } else if v.contains("zulu") || i.contains("azul") {
        "Azul Zulu"
    } else if v.contains("corretto") || i.contains("amazon") {
        "Amazon Corretto"
    } else if v.contains("graalvm") || i.contains("graalvm") {
        "GraalVM"
    } else if i.contains("microsoft") {
        "Microsoft OpenJDK"
    } else if v.contains("liberica") || i.contains("bellsoft") {
        "BellSoft Liberica"
    } else if v.contains("semeru") || i.contains("international business machines") {
        "IBM Semeru"
    } else if v.contains("sapmachine") || i.contains("sap se") {
        "SapMachine"
    } else if i.contains("jetbrains") {
        "JetBrains Runtime"
    } else if i.contains("oracle") {
        "Oracle OpenJDK"
    } else if i.contains("ubuntu")
        || i.contains("debian")
        || i.contains("red hat")
        || i.contains("n/a")
        || info.implementor.trim().is_empty()
    {
        "OpenJDK"
    } else {
        return info.implementor.trim().to_string();
    };
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_release_files() {
        let info = parse_release(
            "IMPLEMENTOR=\"Eclipse Adoptium\"\nIMPLEMENTOR_VERSION=\"Temurin-21.0.4+7\"\nJAVA_RUNTIME_VERSION=\"21.0.4+7-LTS\"\nJAVA_VERSION=\"21.0.4\"\nOS_ARCH=\"x86_64\"\nMODULES=\"java.base\"\n",
        );
        assert_eq!(info.java_version, "21.0.4");
        assert_eq!(info.arch, "x86_64");
        assert_eq!(vendor_name(&info), "Eclipse Temurin");
        let zulu = parse_release("IMPLEMENTOR=\"Azul Systems, Inc.\"\nJAVA_VERSION=\"1.8.0_412\"");
        assert_eq!(vendor_name(&zulu), "Azul Zulu");
        assert_eq!(vendor_name(&ReleaseInfo::default()), "OpenJDK");
        assert_eq!(vendor_name(&parse_release("IMPLEMENTOR=\"Acme\"")), "Acme");
    }

    #[test]
    fn majors() {
        assert_eq!(major_of("1.8.0_412"), 8);
        assert_eq!(major_of("21.0.4"), 21);
        assert_eq!(major_of("25"), 25);
        assert_eq!(major_of("26-ea"), 26);
        assert_eq!(major_of(""), 0);
    }
}
