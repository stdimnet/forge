//! Third-party plugins for Forge.
//!
//! - [`icon_themes`]: icon themes plugins contribute (theme file and SVGs, checked).
//! - [`manifest`]: the `forge-plugin.json` format (id, version, engine range, permissions,
//!   contributions, activation events) and its validation.
//! - [`package`]: reading `.forgeplugin` packages (zip files) safely.
//! - [`store`]: installed plugins in the user data directory (never in a project), their
//!   enablement and granted permissions.
//! - [`permissions`]: checking a plugin's backend calls against what the user granted, and
//!   the permission prompt.
//! - [`marketplace`]: the versioned index format, search, update checks and verified
//!   downloads (SHA-256, optional Ed25519 signatures).
//!
//! Plugins run as sandboxed JavaScript in the UI (see the SDK docs); this crate holds
//! everything that decides what they may do. Native code is never loaded.

#[cfg(feature = "net")]
pub mod host;
pub mod icon_themes;
pub mod manifest;
pub mod marketplace;
#[cfg(feature = "disk")]
pub mod package;
pub mod permissions;
#[cfg(feature = "disk")]
pub mod store;

pub use manifest::{PluginManifest, Problem};
#[cfg(feature = "disk")]
pub use package::Package;
#[cfg(feature = "disk")]
pub use store::{InstalledPlugin, PluginRef, PluginStore, Source};

/// SHA-256 of `bytes`, lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(bytes))
}

/// The running Forge version, which engine ranges are checked against.
pub const FORGE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A downloaded package must be the plugin and version the marketplace listed, asking for
/// the permissions the listing showed in the install prompt.
#[cfg(feature = "disk")]
pub fn check_listing(
    package: &Package,
    id: &str,
    listed: &marketplace::IndexVersion,
) -> Result<(), String> {
    let m = &package.manifest;
    if m.id != id || m.version != listed.version {
        return Err(format!(
            "the download is {} {}, the marketplace listed {id} {}",
            m.id, m.version, listed.version
        ));
    }
    if m.permissions != listed.permissions {
        return Err("the package asks for other permissions than its marketplace listing".into());
    }
    Ok(())
}

#[cfg(all(test, feature = "disk"))]
mod tests {
    use super::*;

    #[test]
    fn listing_must_match_the_package() {
        let bytes = package::tests::word_count_package("1.0.0");
        let pkg = Package::read(&bytes, "0.1.0").unwrap();
        let mut listed: marketplace::IndexVersion = serde_json::from_value(serde_json::json!({
            "version": "1.0.0", "engines": { "forge": ">=0.1.0" }, "url": "x",
            "sha256": pkg.sha256, "permissions": { "editor": ["read"] }
        }))
        .unwrap();
        assert!(check_listing(&pkg, "example.word-count", &listed).is_ok());
        assert!(check_listing(&pkg, "example.other", &listed).is_err());
        listed.permissions.network = vec!["example.com".into()];
        assert!(check_listing(&pkg, "example.word-count", &listed)
            .unwrap_err()
            .contains("permissions"));
    }
}
