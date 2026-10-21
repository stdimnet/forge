//! Composer (Settings | Languages & Frameworks | PHP | Composer): the PHP language level
//! from `require.php` of composer.json ("Synchronize IDE settings with composer.json") and
//! the outdated packages `composer outdated` reports ("Check for package updates on
//! project open").

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Setting keys (see `app/src-tauri/plugins/phpforge.toml`).
pub const EXECUTABLE_KEY: &str = "php.composer.executable";
pub const JSON_KEY: &str = "php.composer.json";
pub const SYNC_KEY: &str = "php.composer.syncWithComposerJson";
pub const LIBRARIES_KEY: &str = "php.composer.addPackagesAsLibraries";
pub const CHECK_UPDATES_KEY: &str = "php.composer.checkUpdates";

/// The language levels Forge offers (`php.languageLevel`).
pub const LANGUAGE_LEVELS: &[&str] = &["7.4", "8.0", "8.1", "8.2", "8.3", "8.4"];

/// The language level a composer.json asks for: the lowest PHP version its `require.php`
/// constraint allows, as one of [`LANGUAGE_LEVELS`] (older versions count as 7.4, newer
/// ones as the newest level). `None` without a constraint.
pub fn language_level(composer_json: &str) -> Option<String> {
    let manifest: Value = serde_json::from_str(composer_json).ok()?;
    let constraint = manifest.get("require")?.get("php")?.as_str()?;
    let lowest = versions_in(constraint).into_iter().min()?;
    let level = LANGUAGE_LEVELS
        .iter()
        .rev()
        .find(|l| parse_level(l).is_some_and(|v| v <= lowest))
        .unwrap_or(&LANGUAGE_LEVELS[0]);
    Some(level.to_string())
}

fn parse_level(s: &str) -> Option<(u32, u32)> {
    let (major, minor) = s.split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

/// Every `major.minor` (or bare `major`) in a constraint like `^8.2 || ~7.4.0`.
fn versions_in(constraint: &str) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let bytes = constraint.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if !bytes[i].is_ascii_digit()
            || (i > 0 && (bytes[i - 1].is_ascii_digit() || bytes[i - 1] == b'.'))
        {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.' || bytes[i] == b'*')
        {
            i += 1;
        }
        let mut parts = constraint[start..i].split('.');
        let major = parts.next().and_then(|p| p.parse().ok());
        let minor = parts.next().map_or(Some(0), |p| p.parse().ok().or(Some(0)));
        if let (Some(major), Some(minor)) = (major, minor) {
            out.push((major, minor));
        }
    }
    out
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_language_level_from_require_php() {
        let level = |c: &str| language_level(&format!(r#"{{"require": {{"php": "{c}"}}}}"#));
        assert_eq!(level("^8.2").as_deref(), Some("8.2"));
        assert_eq!(level(">=8.1 <9.0").as_deref(), Some("8.1"));
        assert_eq!(level("^7.3 || ^8.0").as_deref(), Some("7.4"));
        assert_eq!(level("~8.3.4").as_deref(), Some("8.3"));
        assert_eq!(level("8.*").as_deref(), Some("8.0"));
        assert_eq!(level("^8.5").as_deref(), Some("8.4"));
        assert_eq!(
            language_level(r#"{"require": {"laravel/framework": "^12.0"}}"#),
            None
        );
        assert_eq!(language_level("{not json"), None);
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
