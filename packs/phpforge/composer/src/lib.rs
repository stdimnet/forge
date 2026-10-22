//! Composer support of PHPForge, like PHPStorm's: version constraints, the requirements
//! of composer.json and the installed packages (composer.lock, installed.json), Packagist
//! metadata and search, outdated packages, `composer audit` advisories, and completion and
//! documentation in composer.json.
//!
//! Everything but [`net`] is pure and also builds to WebAssembly for browser mode. The
//! `net` feature adds the Packagist client (cached in the user cache dir) and the
//! composer.phar download.

pub mod audit;
pub mod complete;
pub mod constraint;
pub mod lock;
pub mod manifest;
#[cfg(feature = "net")]
pub mod net;
pub mod outdated;
pub mod packagist;
pub mod version;

#[cfg(all(test, feature = "net"))]
pub(crate) mod testserver;

pub use constraint::Constraint;
pub use version::{Stability, Version};

/// Setting keys (see `app/src-tauri/plugins/phpforge.toml`).
pub const EXECUTABLE_KEY: &str = "php.composer.executable";
pub const JSON_KEY: &str = "php.composer.json";
pub const PACKAGIST_KEY: &str = "php.composer.packagist";

/// The composer.lock next to a composer.json path (`sub/composer.json` → `sub/composer.lock`).
pub fn lock_path(composer_json: &str) -> String {
    sibling(composer_json, "composer.lock")
}

/// The vendor/composer/installed.json next to a composer.json path.
pub fn installed_json_path(composer_json: &str) -> String {
    sibling(composer_json, "vendor/composer/installed.json")
}

fn sibling(composer_json: &str, name: &str) -> String {
    match composer_json.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{name}"),
        None => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_sibling_files() {
        assert_eq!(super::lock_path("composer.json"), "composer.lock");
        assert_eq!(super::lock_path("api/composer.json"), "api/composer.lock");
        assert_eq!(
            super::installed_json_path("api/composer.json"),
            "api/vendor/composer/installed.json"
        );
    }
}
