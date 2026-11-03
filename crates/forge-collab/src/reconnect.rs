//! Reconnecting after a dropped connection: the guest's backoff and give-up rule, and the
//! connection state the UI shows ("Reconnecting…").
//!
//! The rules (see `docs/plans/code-with-me.md`, "S6 reconnect as built"):
//! - The host keeps a guest whose connection dropped for [`HOST_GRACE`]: its participant id,
//!   color, permissions, open documents, terminals and runs stay. Everyone sees it as
//!   reconnecting. A guest that comes back in time resumes with the token from its `Welcome`;
//!   later, the host has removed it and it has to join again with the link.
//! - The guest tries again after 1, 2, 4, 8 and then every 15 seconds and gives up after
//!   [`GUEST_GIVE_UP`], which is shorter than the host's grace so its last attempt still
//!   finds its place.
//!
//! - File changes a watching guest misses while it is away are collected
//!   ([`MissedChanges`]) and sent when it resumes, so its project tree and git state catch up.
//!
//! Pure: no clock, no runtime. The session code passes the elapsed time.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// How long the host keeps the place of a guest whose connection dropped.
pub const HOST_GRACE: Duration = Duration::from_secs(120);

/// How long a guest keeps trying to reach the host again.
pub const GUEST_GIVE_UP: Duration = Duration::from_secs(105);

/// Waits between attempts; the last one repeats.
pub const BACKOFF: &[Duration] = &[
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(15),
];

/// When a guest tries again and when it gives up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconnectPolicy {
    /// Waits before each attempt; the last one repeats. Empty: never reconnect.
    pub backoff: Vec<Duration>,
    /// No attempt starts after this long without a connection.
    pub give_up_after: Duration,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            backoff: BACKOFF.to_vec(),
            give_up_after: GUEST_GIVE_UP,
        }
    }
}

impl ReconnectPolicy {
    /// A policy that ends the session at the first dropped connection.
    pub fn never() -> Self {
        Self {
            backoff: Vec::new(),
            give_up_after: Duration::ZERO,
        }
    }

    /// The wait before attempt `attempt` (0-based), `elapsed` after the connection dropped.
    /// `None`: give up (the attempt would start after [`Self::give_up_after`]).
    pub fn delay(&self, attempt: u32, elapsed: Duration) -> Option<Duration> {
        let last = self.backoff.last()?;
        let wait = *self.backoff.get(attempt as usize).unwrap_or(last);
        (elapsed + wait <= self.give_up_after).then_some(wait)
    }
}

/// How a guest's connection to the host is doing (`SessionView.connection`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ConnectionState {
    Connected,
    /// The connection dropped; attempt `attempt` (1-based) starts in `next_retry_secs`.
    #[serde(rename_all = "camelCase")]
    Reconnecting {
        attempt: u32,
        next_retry_secs: u64,
        /// Why the previous attempt failed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
}

impl ConnectionState {
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }
}

/// The message of a guest that gave up.
pub const GAVE_UP: &str =
    "The connection to the host was lost and could not be restored. Join again with the session link.";

/// The answer while the connection is being restored.
pub const RECONNECTING: &str =
    "The connection to the host is being restored. Try again in a moment.";

/// Compare two tokens without leaking where they differ.
pub fn same_token(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// At most this many paths are kept for an away guest; it re-lists its open folders after a
/// resume anyway, so an overflow only loses the per-file updates (open tabs, bookmarks).
pub const MAX_MISSED_PATHS: usize = 10_000;

/// Project file changes a guest missed while its connection was down, merged: the latest
/// state of each path wins (created then deleted is "removed", deleted then created is
/// "changed").
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MissedChanges {
    /// Path -> it exists now.
    paths: BTreeMap<String, bool>,
    git: bool,
    overflowed: bool,
}

impl MissedChanges {
    /// Add one batch of the host's watcher.
    pub fn record(&mut self, changed: &[String], removed: &[String], git: bool) {
        self.git |= git;
        let batch = changed
            .iter()
            .map(|p| (p, true))
            .chain(removed.iter().map(|p| (p, false)));
        for (path, exists) in batch {
            if self.paths.len() >= MAX_MISSED_PATHS && !self.paths.contains_key(path) {
                self.overflowed = true;
                continue;
            }
            self.paths.insert(path.clone(), exists);
        }
    }

    /// Nothing was missed.
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && !self.git
    }

    /// More paths changed than were kept.
    pub fn overflowed(&self) -> bool {
        self.overflowed
    }

    /// The merged changes: `(changed, removed, git)`.
    pub fn into_parts(self) -> (Vec<String>, Vec<String>, bool) {
        let (changed, removed): (Vec<_>, Vec<_>) =
            self.paths.into_iter().partition(|(_, exists)| *exists);
        let names = |v: Vec<(String, bool)>| v.into_iter().map(|(p, _)| p).collect();
        (names(changed), names(removed), self.git)
    }
}

#[cfg(test)]
mod tests {
    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn missed_changes_keep_the_latest_state_of_each_path() {
        let mut m = MissedChanges::default();
        assert!(m.is_empty());
        m.record(&strings(&["a.php", "b.php"]), &strings(&["old.php"]), false);
        m.record(&strings(&["old.php"]), &strings(&["a.php"]), true);
        m.record(&strings(&["b.php"]), &[], false);
        assert!(!m.is_empty());
        let (changed, removed, git) = m.into_parts();
        assert_eq!(changed, strings(&["b.php", "old.php"]));
        assert_eq!(removed, strings(&["a.php"]));
        assert!(git);
    }

    #[test]
    fn missed_git_changes_alone_count() {
        let mut m = MissedChanges::default();
        m.record(&[], &[], true);
        assert!(!m.is_empty());
        assert_eq!(m.into_parts(), (vec![], vec![], true));
    }

    #[test]
    fn missed_changes_are_bounded() {
        let mut m = MissedChanges::default();
        let many: Vec<String> = (0..MAX_MISSED_PATHS + 5).map(|i| format!("f{i}")).collect();
        m.record(&many, &[], false);
        assert!(m.overflowed());
        // A path already kept still follows its latest state.
        m.record(&[], &strings(&["f0"]), false);
        let (changed, removed, _) = m.into_parts();
        assert_eq!(changed.len() + removed.len(), MAX_MISSED_PATHS);
        assert_eq!(removed, strings(&["f0"]));
    }

    use super::*;

    const S: fn(u64) -> Duration = Duration::from_secs;

    #[test]
    fn backoff_doubles_then_repeats_the_last_wait() {
        let p = ReconnectPolicy::default();
        let waits: Vec<u64> = (0..7)
            .map(|a| p.delay(a, Duration::ZERO).unwrap().as_secs())
            .collect();
        assert_eq!(waits, [1, 2, 4, 8, 15, 15, 15]);
    }

    #[test]
    fn gives_up_when_the_next_attempt_would_start_too_late() {
        let p = ReconnectPolicy::default();
        assert_eq!(p.delay(5, S(90)), Some(S(15)));
        assert_eq!(p.delay(5, S(91)), None);
        assert_eq!(p.delay(0, GUEST_GIVE_UP), None);
    }

    #[test]
    fn the_guest_gives_up_before_the_host_frees_its_place() {
        // The last attempt starts at the latest at GUEST_GIVE_UP and takes a few seconds.
        assert!(GUEST_GIVE_UP + S(10) < HOST_GRACE);
        // Attempts in the default schedule: at 1, 3, 7, 15, 30, 45, ... seconds.
        let p = ReconnectPolicy::default();
        let (mut at, mut n) = (Duration::ZERO, 0);
        while let Some(wait) = p.delay(n, at) {
            at += wait;
            n += 1;
        }
        assert!(n >= 9, "{n} attempts");
        assert!(at <= GUEST_GIVE_UP);
    }

    #[test]
    fn never_reconnects_without_a_backoff() {
        assert_eq!(ReconnectPolicy::never().delay(0, Duration::ZERO), None);
    }

    #[test]
    fn state_serializes_for_the_status_bar() {
        let s = ConnectionState::Reconnecting {
            attempt: 2,
            next_retry_secs: 4,
            error: None,
        };
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["state"], "reconnecting");
        assert_eq!(json["nextRetrySecs"], 4);
        assert!(json.get("error").is_none());
        let c = serde_json::to_value(ConnectionState::Connected).unwrap();
        assert_eq!(c["state"], "connected");
    }

    #[test]
    fn tokens_compare_in_full() {
        assert!(same_token(&[1, 2, 3], &[1, 2, 3]));
        assert!(!same_token(&[1, 2, 3], &[1, 2, 4]));
        assert!(!same_token(&[1, 2], &[1, 2, 3]));
    }
}
