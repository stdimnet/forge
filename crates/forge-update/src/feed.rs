//! The update feed (`latest.json`) and the offer shown to the user.
//!
//! `pnpm tauri build` with `createUpdaterArtifacts` writes signed bundles plus their `.sig`
//! files; `latest.json` lists them per platform:
//!
//! ```json
//! { "version": "0.2.0", "notes": "…", "pub_date": "2026-11-02T10:00:00Z",
//!   "platforms": { "linux-x86_64": { "signature": "…", "url": "https://…/Forge_0.2.0_amd64.AppImage" } } }
//! ```
//!
//! Keys are `{os}-{arch}` or `{os}-{arch}-{installer}` (`linux-x86_64-appimage`), as
//! `tauri-plugin-updater` reads them.

use std::collections::BTreeMap;

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::policy::{evaluate, Channel, UpdateState, Verdict};
use crate::{format_size, parse_version, Result, UpdateError};

/// One platform's download in the feed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeedPlatform {
    pub signature: String,
    pub url: String,
}

/// `latest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feed {
    pub version: String,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub pub_date: Option<String>,
    #[serde(default)]
    pub platforms: BTreeMap<String, FeedPlatform>,
}

impl Feed {
    pub fn parse(text: &str) -> Result<Self> {
        serde_json::from_str(text).map_err(|e| UpdateError::Feed(e.to_string()))
    }

    /// The download for the first of `targets` the feed has (most specific first).
    pub fn platform(&self, targets: &[String]) -> Option<(&str, &FeedPlatform)> {
        targets
            .iter()
            .find_map(|t| self.platforms.get_key_value(t.as_str()))
            .map(|(k, p)| (k.as_str(), p))
    }
}

/// An update the user can install, as the splash screen and the Check for Updates dialog
/// show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOffer {
    pub version: String,
    pub current_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    /// Download size in bytes, when the server reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// The one-line description under the title.
    pub summary: String,
    /// A simulated update: the download is fake and nothing is installed.
    #[serde(default)]
    pub simulated: bool,
}

impl UpdateOffer {
    pub fn new(
        version: &str,
        current: &str,
        notes: Option<String>,
        date: Option<String>,
        size: Option<u64>,
        simulated: bool,
    ) -> Self {
        let summary = summary(current, size, simulated);
        Self {
            version: version.to_string(),
            current_version: current.to_string(),
            notes: notes.filter(|n| !n.trim().is_empty()),
            date,
            size,
            summary,
            simulated,
        }
    }
}

/// "You have 0.1.0. The download is 94 MB and is checked against Forge's signing key before
/// it installs."
pub fn summary(current: &str, size: Option<u64>, simulated: bool) -> String {
    let size = size.map(format_size);
    match (simulated, size) {
        (true, Some(s)) => format!(
            "You have {current}. This update is simulated: the {s} download is fake and nothing is installed."
        ),
        (true, None) => format!(
            "You have {current}. This update is simulated: the download is fake and nothing is installed."
        ),
        (false, Some(s)) => format!(
            "You have {current}. The download is {s} and is checked against Forge's signing key before it installs."
        ),
        (false, None) => format!(
            "You have {current}. The download is checked against Forge's signing key before it installs."
        ),
    }
}

/// Read the feed. A `204 No Content` answer means "no update" and returns `None`.
pub async fn fetch_feed(client: &reqwest::Client, endpoint: &str) -> Result<Option<Feed>> {
    let response = client
        .get(endpoint)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    let status = response.status();
    if status == reqwest::StatusCode::NO_CONTENT {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(UpdateError::Status(status.as_u16()));
    }
    let text = response
        .text()
        .await
        .map_err(|e| UpdateError::Network(e.to_string()))?;
    Feed::parse(&text).map(Some)
}

/// The download's size from a `HEAD` request, if the server tells.
pub async fn download_size(client: &reqwest::Client, url: &str) -> Option<u64> {
    let response = client.head(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)?
        .to_str()
        .ok()?
        .parse()
        .ok()
        .filter(|n| *n > 0)
}

/// What a check found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum CheckOutcome {
    Available {
        offer: UpdateOffer,
    },
    /// Nothing newer (or the server answered `204`).
    UpToDate,
    /// A newer release exists but is not offered (skipped, postponed or another channel).
    NotOffered {
        version: String,
        verdict: Verdict,
    },
}

/// The inputs of one check.
#[derive(Debug, Clone)]
pub struct CheckRequest<'a> {
    pub endpoint: &'a str,
    pub current: &'a Version,
    pub channel: Channel,
    pub state: &'a UpdateState,
    pub manual: bool,
    /// Unix seconds.
    pub now: u64,
    /// Feed platform keys of this installation, most specific first.
    pub targets: &'a [String],
}

/// Read the feed and decide what to offer.
pub async fn check(client: &reqwest::Client, req: &CheckRequest<'_>) -> Result<CheckOutcome> {
    let Some(feed) = fetch_feed(client, req.endpoint).await? else {
        return Ok(CheckOutcome::UpToDate);
    };
    let remote = parse_version(&feed.version)?;
    match evaluate(
        req.current,
        &remote,
        req.channel,
        req.state,
        req.manual,
        req.now,
    ) {
        Verdict::Update => {}
        Verdict::UpToDate => return Ok(CheckOutcome::UpToDate),
        verdict => {
            return Ok(CheckOutcome::NotOffered {
                version: remote.to_string(),
                verdict,
            })
        }
    }
    let (_, platform) = feed
        .platform(req.targets)
        .ok_or_else(|| UpdateError::NoPlatform(req.targets.join(", ")))?;
    let size = download_size(client, &platform.url).await;
    Ok(CheckOutcome::Available {
        offer: UpdateOffer::new(
            &remote.to_string(),
            &req.current.to_string(),
            feed.notes.clone(),
            feed.pub_date.clone(),
            size,
            false,
        ),
    })
}

#[cfg(test)]
#[path = "feed_tests.rs"]
mod tests;
