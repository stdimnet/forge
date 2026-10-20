//! "A newer version is available": at most one registry check a day, remembered in
//! `<managed dir>/.forge-update-check.json`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::node::parse_version;

pub const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageUpdate {
    pub name: String,
    pub installed: String,
    pub latest: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CheckState {
    last_check: u64,
}

pub fn state_file(managed: &Path) -> PathBuf {
    managed.join(".forge-update-check.json")
}

/// Whether a day has passed since the last check (or it never ran).
pub fn due(state: &Path, now: u64) -> bool {
    let last = std::fs::read_to_string(state)
        .ok()
        .and_then(|s| serde_json::from_str::<CheckState>(&s).ok())
        .map(|s| s.last_check);
    match last {
        None => true,
        Some(last) => now.saturating_sub(last) >= CHECK_INTERVAL_SECS || last > now,
    }
}

pub fn mark_checked(state: &Path, now: u64) {
    if let Some(dir) = state.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let text = serde_json::to_string(&CheckState { last_check: now }).unwrap_or_default();
    if let Err(e) = std::fs::write(state, text) {
        tracing::warn!("cannot write {}: {e}", state.display());
    }
}

/// `latest` is a higher release than `installed` (pre-releases are never offered).
pub fn is_newer(latest: &str, installed: &str) -> bool {
    if latest.contains('-') {
        return false;
    }
    match (parse_version(latest), parse_version(installed)) {
        (Some(l), Some(i)) => l > i,
        _ => false,
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("1.14.4", "1.12.6"));
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(!is_newer("1.12.6", "1.12.6"));
        assert!(!is_newer("1.12.5", "1.12.6"));
        assert!(!is_newer("2.0.0-beta.1", "1.12.6"));
    }

    #[test]
    fn checks_once_a_day() {
        let tmp = tempfile::tempdir().unwrap();
        let state = state_file(tmp.path());
        assert!(due(&state, 1000), "never checked");
        mark_checked(&state, 1000);
        assert!(!due(&state, 1000 + CHECK_INTERVAL_SECS - 1));
        assert!(due(&state, 1000 + CHECK_INTERVAL_SECS));
        assert!(due(&state, 10), "clock went backwards");
    }
}
