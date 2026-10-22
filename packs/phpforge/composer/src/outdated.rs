//! Outdated packages in composer.json: the installed version (composer.lock or
//! installed.json) against the latest release (Packagist, or what `composer outdated`
//! reported), with Composer's statuses: `semver-safe-update` when the constraint allows the
//! update, `update-possible` when it needs a new constraint.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::constraint::{recommended, Constraint};
use crate::lock::InstalledPackage;
use crate::manifest::{self, Offsets, Position};
use crate::version::Version;

/// A package `composer outdated` reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutdatedPackage {
    pub name: String,
    pub version: String,
    pub latest: String,
    /// `semver-safe-update` (within the constraint) or `update-possible`.
    pub status: String,
}

/// Parse `composer outdated --direct --format=json`.
pub fn parse_outdated(json: &str) -> Vec<OutdatedPackage> {
    let Ok(v) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    v.get("installed")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let s = |k: &str| p.get(k).and_then(Value::as_str).map(str::to_string);
            let status = s("latest-status")?;
            let name = s("name")?;
            (status != "up-to-date").then(|| OutdatedPackage {
                name,
                version: s("version").unwrap_or_default(),
                latest: s("latest").unwrap_or_default(),
                status,
            })
        })
        .collect()
}

/// An outdated requirement of the composer.json in the editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutdatedMarker {
    pub name: String,
    pub constraint: String,
    pub installed: String,
    pub latest: String,
    pub status: String,
    pub dev: bool,
    /// The constraint that allows `latest` (`^13.0`), for "Update to …".
    pub suggested: String,
    /// UTF-16 offsets of the constraint string (quotes included) and its line (0-based).
    pub from: usize,
    pub to: usize,
    pub line: usize,
    pub start: Position,
    pub end: Position,
    /// The inspection's message.
    pub message: String,
}

/// Outdated requirements of `composer_json`: `latest` maps package names to their newest
/// release.
pub fn markers(
    composer_json: &str,
    installed: &[InstalledPackage],
    latest: &HashMap<String, String>,
) -> Vec<OutdatedMarker> {
    let offsets = Offsets::new(composer_json);
    let mut out = Vec::new();
    for r in manifest::requirements(composer_json) {
        if r.is_platform() {
            continue;
        }
        let Some(pkg) = installed.iter().find(|p| p.name == r.name) else {
            continue;
        };
        let Some(newest) = latest.get(&r.name) else {
            continue;
        };
        let (Some(have), Some(new)) = (Version::parse(&pkg.version), Version::parse(newest)) else {
            continue;
        };
        if have.is_branch() || new <= have {
            continue;
        }
        let safe = Constraint::parse(&r.constraint).is_some_and(|c| c.matches(&new));
        let message = format!(
            "Package '{}' is outdated: {} is installed, {} is available",
            r.name, pkg.version, newest
        );
        out.push(OutdatedMarker {
            message,
            name: r.name.clone(),
            constraint: r.constraint.clone(),
            installed: pkg.version.clone(),
            latest: newest.clone(),
            status: if safe {
                "semver-safe-update"
            } else {
                "update-possible"
            }
            .into(),
            dev: r.dev,
            suggested: if safe {
                r.constraint.clone()
            } else {
                recommended(&new)
            },
            from: offsets.utf16(r.value.0),
            to: offsets.utf16(r.value.1),
            line: offsets.line(r.value.0),
            start: offsets.position(r.value.0),
            end: offsets.position(r.value.1),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &str, version: &str) -> InstalledPackage {
        InstalledPackage {
            name: name.into(),
            version: version.into(),
            description: String::new(),
            homepage: None,
            source: None,
            dev: false,
            abandoned: None,
        }
    }

    #[test]
    fn marks_outdated_requirements() {
        let json = "{\n  \"require\": {\n    \"php\": \"^8.3\",\n    \"laravel/framework\": \"^12.0\",\n    \"doctrine/dbal\": \"^3.0\",\n    \"guzzlehttp/guzzle\": \"^7.9\"\n  },\n  \"require-dev\": {\"phpunit/phpunit\": \"^11.0\"}\n}";
        let installed = [
            pkg("laravel/framework", "v12.1.0"),
            pkg("doctrine/dbal", "3.9.4"),
            pkg("guzzlehttp/guzzle", "7.9.2"),
            pkg("phpunit/phpunit", "11.4.0"),
        ];
        let latest: HashMap<String, String> = [
            ("laravel/framework", "v12.3.0"),
            ("doctrine/dbal", "4.2.1"),
            ("guzzlehttp/guzzle", "7.9.2"),
            ("php", "9.0.0"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        let m = markers(json, &installed, &latest);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].name, "laravel/framework");
        assert_eq!(m[0].status, "semver-safe-update");
        assert_eq!(m[0].suggested, "^12.0");
        assert_eq!(m[0].line, 3);
        assert_eq!(&json[m[0].from..m[0].to], "\"^12.0\"");
        assert_eq!(m[1].status, "update-possible");
        assert_eq!(m[1].suggested, "^4.2");
        assert_eq!(m[1].start.line, 4);
        assert_eq!(
            m[0].message,
            "Package 'laravel/framework' is outdated: v12.1.0 is installed, v12.3.0 is available"
        );
    }

    #[test]
    fn parses_composer_outdated() {
        let json = r#"{"installed": [
            {"name": "laravel/framework", "version": "v12.1.0", "latest": "v12.3.0", "latest-status": "semver-safe-update"},
            {"name": "guzzlehttp/guzzle", "version": "7.9.2", "latest": "7.9.2", "latest-status": "up-to-date"},
            {"name": "phpunit/phpunit", "version": "10.5.0", "latest": "11.4.0", "latest-status": "update-possible"}
        ]}"#;
        let out = parse_outdated(json);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].name, "laravel/framework");
        assert_eq!(out[1].status, "update-possible");
        assert!(parse_outdated("garbage").is_empty());
    }
}
