//! Commit Message History (`Ctrl+M` in the commit message): the messages committed from
//! Forge, kept per project in Forge's data directory, plus the user's own recent commits.

use serde::{Deserialize, Serialize};

/// Default number of messages kept (Settings | Version Control | Commit).
pub const DEFAULT_LIMIT: usize = 20;

/// Messages committed from this project, most recent first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageHistory {
    #[serde(default)]
    pub messages: Vec<String>,
}

/// Trailing whitespace per line and surrounding blank lines removed, like
/// `--cleanup=strip` leaves a message.
pub fn normalize(message: &str) -> String {
    let lines: Vec<&str> = message.lines().map(str::trim_end).collect();
    lines.join("\n").trim_matches('\n').to_string()
}

impl MessageHistory {
    pub fn from_json(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }

    /// Put `message` first (moving it up when it is already known) and keep `limit`.
    pub fn record(&mut self, message: &str, limit: usize) {
        let message = normalize(message);
        if message.is_empty() {
            return;
        }
        self.messages.retain(|m| *m != message);
        self.messages.insert(0, message);
        self.messages.truncate(limit.max(1));
    }

    /// The popup's list: Forge's history, then the user's own commits it does not hold
    /// yet (newest first), at most `limit` entries.
    pub fn merged(&self, own_commits: &[String], limit: usize) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for m in self
            .messages
            .iter()
            .map(|m| normalize(m))
            .chain(own_commits.iter().map(|m| normalize(m)))
        {
            if !m.is_empty() && !out.contains(&m) {
                out.push(m);
            }
            if out.len() >= limit.max(1) {
                break;
            }
        }
        out
    }
}

/// Split `git log --format=%B%x1e` output into messages.
pub fn parse_messages(out: &str) -> Vec<String> {
    out.split('\x1e')
        .map(normalize)
        .filter(|m| !m.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_newest_first_without_duplicates() {
        let mut h = MessageHistory::default();
        h.record("Add search  \n\nBody line  \n", 3);
        h.record("Fix typo", 3);
        h.record("Add search\n\nBody line", 3);
        assert_eq!(h.messages, vec!["Add search\n\nBody line", "Fix typo"]);
        h.record("  ", 3);
        h.record("A", 3);
        h.record("B", 3);
        assert_eq!(h.messages, vec!["B", "A", "Add search\n\nBody line"]);
        let back = MessageHistory::from_json(&h.to_json());
        assert_eq!(back, h);
        assert_eq!(
            MessageHistory::from_json("garbage"),
            MessageHistory::default()
        );
    }

    #[test]
    fn merges_own_commits() {
        let h = MessageHistory {
            messages: vec!["B".into(), "A".into()],
        };
        let own = parse_messages("C\n\x1e\nA\n\x1e\nD\n\x1e\n");
        assert_eq!(own, vec!["C", "A", "D"]);
        assert_eq!(h.merged(&own, 10), vec!["B", "A", "C", "D"]);
        assert_eq!(h.merged(&own, 3), vec!["B", "A", "C"]);
    }
}
