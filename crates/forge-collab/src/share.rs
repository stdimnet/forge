//! Shared terminals and runs (stage S5): what guests see of the host's terminal tabs and Run
//! tool window, and the buffers that let a guest who joins later see what happened before.
//!
//! - A **terminal** is shared when the host chose Share with Guests on its tab, or when a
//!   guest with Terminal Full access opened it on the host ("Alex: Local"). Guests with
//!   Terminal Read-only see its output live; with Full access they can type into it. The
//!   host's terminal size wins: guests render at its columns and rows.
//! - A **run** is every run the host's Run tool window starts from a run configuration.
//!   Guests with Run Read-only see its console and test tree; with Full access they start the
//!   host's configurations by name and stop runs. A guest never sends a command line.
//!
//! Everything here is pure (no tokio): the ids and views both sides exchange, and the bounded
//! buffers ([`Scrollback`], [`RunLog`]) the host and the guests keep.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::permissions::{Permissions, RunAccess, TerminalAccess};
use crate::presence::ParticipantId;

/// A shared terminal, numbered by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TerminalId(pub u64);

/// A shared run, numbered by the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RunId(pub u64);

/// A terminal as the participants see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedTerminal {
    pub id: TerminalId,
    /// The host's tab title, or "<guest>: Local" for a guest's terminal.
    pub title: String,
    /// Who opened it: the host, or the guest whose terminal it is.
    pub owner: ParticipantId,
    /// The host's size; guests render at it.
    pub cols: u16,
    pub rows: u16,
    pub running: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

/// A run of the host's Run tool window as the participants see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedRun {
    pub id: RunId,
    /// The run configuration's name (the tab title).
    pub title: String,
    /// The configuration's type id (for the icon), when the host knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Who started it (a guest's start is still the host's run).
    pub started_by: ParticipantId,
    pub running: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

/// Whether a guest with `permissions` sees shared terminals.
pub fn sees_terminals(permissions: &Permissions) -> bool {
    permissions.terminal >= TerminalAccess::ReadOnly
}

/// Whether a guest with `permissions` may type into shared terminals and open its own.
pub fn uses_terminals(permissions: &Permissions) -> bool {
    permissions.terminal == TerminalAccess::Full
}

/// Whether a guest with `permissions` sees the host's runs.
pub fn sees_runs(permissions: &Permissions) -> bool {
    permissions.run >= RunAccess::ReadOnly
}

/// Whether a guest with `permissions` may start and stop the host's runs.
pub fn controls_runs(permissions: &Permissions) -> bool {
    permissions.run == RunAccess::Full
}

/// Lines a terminal's scrollback keeps (the plan's snapshot of the last 2 000 lines).
pub const SCROLLBACK_LINES: usize = 2000;
/// Bytes a terminal's scrollback keeps at most (long lines without breaks).
pub const SCROLLBACK_BYTES: usize = 512 * 1024;

/// The recent output of a terminal, so a guest who joins (or a tab opened later) starts with
/// what is on screen. Trimmed at line breaks, so escape sequences are only ever cut at the
/// start of a line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Scrollback {
    text: String,
    lines: usize,
}

impl Scrollback {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, data: &str) {
        self.text.push_str(data);
        self.lines += data.matches('\n').count();
        while self.lines > SCROLLBACK_LINES || self.text.len() > SCROLLBACK_BYTES {
            match self.text.find('\n') {
                Some(at) => {
                    self.text.drain(..=at);
                    self.lines -= 1;
                }
                None => {
                    // One endless line: keep its end, at a character boundary.
                    let mut cut = self.text.len() - SCROLLBACK_BYTES;
                    while !self.text.is_char_boundary(cut) {
                        cut += 1;
                    }
                    self.text.drain(..cut);
                    break;
                }
            }
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

/// Bytes of rendered run events a run keeps for guests who join later.
pub const RUN_LOG_BYTES: usize = 8 * 1024 * 1024;

/// The rendered events of a run (`run://<runId>` JSON: console deltas, test tree updates, the
/// exit), so a guest who joins while it runs, or afterwards, sees the whole tab. Console
/// deltas are relative (append, replace the last line, drop from the front), so the oldest
/// console events can be dropped when the log is full; every other event is kept.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunLog {
    events: VecDeque<(bool, String)>,
    bytes: usize,
}

impl RunLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add one event (JSON text); `console` says whether it is a console delta.
    pub fn push(&mut self, event: String, console: bool) {
        self.bytes += event.len();
        self.events.push_back((console, event));
        while self.bytes > RUN_LOG_BYTES {
            let Some(at) = self.events.iter().position(|(c, _)| *c) else {
                break;
            };
            if let Some((_, e)) = self.events.remove(at) {
                self.bytes -= e.len();
            }
        }
    }

    pub fn events(&self) -> impl Iterator<Item = &str> {
        self.events.iter().map(|(_, e)| e.as_str())
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// What a run event (JSON text of `run://<runId>`) means for sharing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunEventKind {
    /// Raw process output: the UI renders the `console` events instead, so it is not shared.
    Raw,
    /// A console delta (may be dropped from a full [`RunLog`]).
    Console,
    /// The process exited, with its code.
    Exited(Option<i32>),
    /// Anything else (started, test tree, coverage, ...).
    Other,
}

/// Classify a run event by its `type` field.
pub fn run_event_kind(event: &serde_json::Value) -> RunEventKind {
    match event.get("type").and_then(|t| t.as_str()) {
        Some("output") => RunEventKind::Raw,
        Some("console") => RunEventKind::Console,
        Some("exited") => RunEventKind::Exited(
            event
                .get("code")
                .and_then(|c| c.as_i64())
                .and_then(|c| i32::try_from(c).ok()),
        ),
        _ => RunEventKind::Other,
    }
}

/// Replace the host's absolute project folder in a run event with the guest's placeholder
/// root (`/<project>`), so file links in the console open the guest's view of the file and
/// the host's home folder is not shown. Only whole path prefixes are replaced.
pub fn map_root(event: &str, host_root: &str, guest_root: &str) -> String {
    if host_root.len() < 2 || !event.contains(host_root) {
        return event.to_string();
    }
    let mut out = String::with_capacity(event.len());
    let mut rest = event;
    while let Some(at) = rest.find(host_root) {
        let after = &rest[at + host_root.len()..];
        out.push_str(&rest[..at]);
        // `/home/robert/shop2` is not inside `/home/robert/shop`.
        if after.is_empty() || after.starts_with(['/', '\\', '"']) {
            out.push_str(guest_root);
        } else {
            out.push_str(host_root);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrollback_keeps_the_last_lines() {
        let mut s = Scrollback::new();
        for i in 0..SCROLLBACK_LINES + 10 {
            s.push(&format!("line {i}\r\n"));
        }
        assert!(s.text().starts_with("line 10\r\n"), "{}", &s.text()[..20]);
        assert!(s
            .text()
            .ends_with(&format!("line {}\r\n", SCROLLBACK_LINES + 9)));
        s.push("prompt $ ");
        assert!(s.text().ends_with("\r\nprompt $ "));
    }

    #[test]
    fn scrollback_bounds_endless_lines_at_char_boundaries() {
        let mut s = Scrollback::new();
        s.push(&"ü".repeat(SCROLLBACK_BYTES));
        assert!(s.text().len() <= SCROLLBACK_BYTES);
        assert!(s.text().chars().all(|c| c == 'ü'));
    }

    #[test]
    fn run_logs_drop_old_console_events_first() {
        let mut log = RunLog::new();
        log.push(r#"{"type":"started"}"#.into(), false);
        let big = "x".repeat(RUN_LOG_BYTES / 4);
        for _ in 0..6 {
            log.push(big.clone(), true);
        }
        log.push(r#"{"type":"exited","code":0}"#.into(), false);
        let events: Vec<&str> = log.events().collect();
        assert_eq!(events[0], r#"{"type":"started"}"#);
        assert_eq!(*events.last().unwrap(), r#"{"type":"exited","code":0}"#);
        assert!(events.len() < 8);
    }

    #[test]
    fn run_events_are_classified() {
        let kind = |s: &str| run_event_kind(&serde_json::from_str(s).unwrap());
        assert_eq!(kind(r#"{"type":"output","text":"x"}"#), RunEventKind::Raw);
        assert_eq!(
            kind(r#"{"type":"console","lines":[]}"#),
            RunEventKind::Console
        );
        assert_eq!(
            kind(r#"{"type":"exited","code":3,"signal":null}"#),
            RunEventKind::Exited(Some(3))
        );
        assert_eq!(
            kind(r#"{"type":"exited","code":null}"#),
            RunEventKind::Exited(None)
        );
        assert_eq!(kind(r#"{"type":"tests","nodes":[]}"#), RunEventKind::Other);
    }

    #[test]
    fn host_paths_become_the_guest_root() {
        let e = r#"{"path":"/home/robert/shop/app/User.php","other":"/home/robert/shop2/x","root":"/home/robert/shop"}"#;
        assert_eq!(
            map_root(e, "/home/robert/shop", "/shop"),
            r#"{"path":"/shop/app/User.php","other":"/home/robert/shop2/x","root":"/shop"}"#
        );
        assert_eq!(map_root("nothing", "/home/robert/shop", "/shop"), "nothing");
    }

    #[test]
    fn access_follows_the_permission_table() {
        let ro = Permissions::READ_ONLY;
        assert!(sees_terminals(&ro) && !uses_terminals(&ro));
        assert!(sees_runs(&ro) && !controls_runs(&ro));
        let edit = Permissions::EDIT_FILES;
        assert!(!sees_terminals(&edit) && !sees_runs(&edit));
        let full = Permissions::FULL_ACCESS;
        assert!(uses_terminals(&full) && controls_runs(&full));
    }
}
