//! Composer (Settings | Languages & Frameworks | PHP | Composer): the PHP language level
//! from `require.php` of composer.json ("Synchronize IDE settings with composer.json") and
//! the outdated packages `composer outdated` reports ("Check for package updates on
//! project open").

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

pub use phpforge_composer::outdated::{parse_outdated, OutdatedPackage};

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
}
