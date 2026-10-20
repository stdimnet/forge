//! Update checks for Forge.
//!
//! The Tauri shell downloads, verifies (minisign signature) and installs updates with
//! `tauri-plugin-updater`. Everything that decides *whether* and *what* to offer lives here,
//! without a window or an app handle, so it is unit tested:
//!
//! - [`policy`]: the update channel, the remembered state (last check, skipped and postponed
//!   versions), when a check is due ([`plan`]) and which release counts as an update
//!   ([`evaluate`]).
//! - [`feed`]: the `latest.json` feed that `pnpm tauri build` produces (`createUpdaterArtifacts`),
//!   fetching it, the download size and the [`UpdateOffer`] the splash screen shows.
//! - [`simulate`]: a fake offer and download for `FORGE_SIMULATE_UPDATE` (desktop dev builds)
//!   and `?simulateUpdate` (browser mode). Nothing is installed.
//! - [`meter`]: the download speed shown while an update downloads.

pub mod feed;
pub mod meter;
pub mod policy;
pub mod simulate;

pub use feed::{check, fetch_feed, CheckOutcome, CheckRequest, Feed, FeedPlatform, UpdateOffer};
pub use meter::SpeedMeter;
pub use policy::{
    evaluate, plan, Channel, CheckPlan, PlanInput, Postponed, SkipReason, UpdateState, Verdict,
};

/// The feed of the stable channel: the newest GitHub release of `Reeperk/Forge`.
pub const DEFAULT_ENDPOINT: &str =
    "https://github.com/Reeperk/Forge/releases/latest/download/latest.json";
/// The feed of the beta channel: a release tagged `beta` whose `latest.json` is replaced
/// whenever a pre-release is published.
pub const BETA_ENDPOINT: &str =
    "https://github.com/Reeperk/Forge/releases/download/beta/latest.json";
/// Automatic checks run at most once per this many seconds (one day).
pub const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

/// Why an update check or download failed. [`UpdateError::short_message`] is what the splash
/// screen shows.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("cannot reach the update server: {0}")]
    Network(String),
    #[error("the update server answered {0}")]
    Status(u16),
    #[error("invalid update feed: {0}")]
    Feed(String),
    #[error("invalid version {0:?}")]
    Version(String),
    #[error("the update has no download for this system ({0})")]
    NoPlatform(String),
}

impl UpdateError {
    /// One short sentence for the splash screen and the Check for Updates dialog.
    pub fn short_message(&self) -> String {
        match self {
            Self::Network(_) => "Cannot reach the update server. Are you offline?".into(),
            Self::Status(404) => "No update information was found on the server.".into(),
            Self::Status(code) => format!("The update server answered with error {code}."),
            Self::Feed(_) | Self::Version(_) => "The update information is invalid.".into(),
            Self::NoPlatform(_) => "This update has no download for your system.".into(),
        }
    }
}

pub type Result<T> = std::result::Result<T, UpdateError>;

/// Parse a version as the feed writes it (`0.2.0`, `v0.2.0`, `0.3.0-beta.1`).
pub fn parse_version(text: &str) -> Result<semver::Version> {
    semver::Version::parse(text.trim().trim_start_matches('v'))
        .map_err(|_| UpdateError::Version(text.to_string()))
}

/// Download sizes as the splash screen shows them: `94 MB`, `3.4 MB`, `820 KB`.
pub fn format_size(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= 10.0 * MB {
        format!("{:.0} MB", b / MB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else {
        format!("{:.0} KB", (b / 1024.0).max(1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_accept_a_v_prefix() {
        assert_eq!(parse_version("v0.2.0").unwrap().to_string(), "0.2.0");
        assert_eq!(
            parse_version(" 0.3.0-beta.1 ").unwrap().to_string(),
            "0.3.0-beta.1"
        );
        assert!(parse_version("latest").is_err());
    }

    #[test]
    fn sizes_are_rounded_for_humans() {
        assert_eq!(format_size(94 * 1024 * 1024), "94 MB");
        assert_eq!(format_size(3_565_158), "3.4 MB");
        assert_eq!(format_size(820 * 1024), "820 KB");
        assert_eq!(format_size(10), "1 KB");
    }

    #[test]
    fn errors_have_short_messages() {
        assert!(UpdateError::Network("dns".into())
            .short_message()
            .contains("offline"));
        assert!(UpdateError::Status(500).short_message().contains("500"));
    }
}
