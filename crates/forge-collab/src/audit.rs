//! The host's session log: who knocked, who was admitted or removed, permission changes, what
//! guests changed and what the host refused. It is written as JSON Lines into the user data
//! directory (`<data dir>/forge/code-with-me/<date>-<session>.jsonl`), never into the project,
//! and the most recent entries are kept for the Session Log in the UI. Logs older than
//! [`MAX_AGE_DAYS`] are deleted when a new session starts.
//!
//! Terminal keystrokes are never logged (decision 7 of the plan); nothing here takes file
//! contents.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::permissions::{PermissionLevel, Permissions};
use crate::presence::ParticipantId;

/// Logs older than this are deleted.
pub const MAX_AGE_DAYS: u64 = 30;
/// Entries kept in memory for the Session Log.
pub const RECENT: usize = 1000;

/// What happened to a join request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KnockOutcome {
    Accepted,
    Declined,
    Unanswered,
    /// The connection did not prove it has the link; nobody was asked.
    WrongProof,
}

/// A guest's change of the project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileAction {
    Saved,
    Written,
    Created,
    CreatedFolder,
    Renamed,
    Deleted,
}

/// What happened to a shared terminal. Keys typed are never logged, only that a guest typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalAction {
    /// The host shared its terminal tab with the guests.
    Shared,
    /// The host stopped sharing it.
    Unshared,
    /// A guest opened a terminal on the host.
    Opened,
    /// A guest typed into the terminal (once per guest and terminal).
    Typed,
    /// A guest closed its terminal.
    Closed,
}

/// What a guest did with the host's runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunAction {
    Started,
    Stopped,
    /// Console input (once per guest and run).
    Typed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase")]
pub enum AuditEvent {
    #[serde(rename_all = "camelCase")]
    SessionStarted {
        host: String,
        permissions: Permissions,
    },
    #[serde(rename_all = "camelCase")]
    SessionEnded {
        reason: String,
    },
    #[serde(rename_all = "camelCase")]
    Knock {
        /// The name the guest gave (empty for a wrong proof).
        name: String,
        /// The guest's endpoint key, shortened.
        peer: String,
        outcome: KnockOutcome,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        permissions: Option<Permissions>,
    },
    #[serde(rename_all = "camelCase")]
    Left {
        participant: ParticipantId,
        name: String,
    },
    #[serde(rename_all = "camelCase")]
    Removed {
        participant: ParticipantId,
        name: String,
    },
    /// `participant: None` is the session's permission (every guest).
    #[serde(rename_all = "camelCase")]
    PermissionsChanged {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        participant: Option<ParticipantId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        permissions: Permissions,
    },
    LinkRegenerated,
    #[serde(rename_all = "camelCase")]
    File {
        participant: ParticipantId,
        name: String,
        action: FileAction,
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        to: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Git {
        participant: ParticipantId,
        name: String,
        action: String,
    },
    /// A shared terminal; `participant: None` is the host.
    #[serde(rename_all = "camelCase")]
    Terminal {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        participant: Option<ParticipantId>,
        name: String,
        action: TerminalAction,
        terminal: String,
    },
    /// A guest started, stopped or typed into one of the host's runs.
    #[serde(rename_all = "camelCase")]
    Run {
        participant: ParticipantId,
        name: String,
        action: RunAction,
        run: String,
    },
    #[serde(rename_all = "camelCase")]
    Denied {
        participant: ParticipantId,
        name: String,
        request: String,
        reason: String,
    },
}

fn level_title(p: &Permissions) -> &'static str {
    match p.level() {
        PermissionLevel::ReadOnly => "Read-only",
        PermissionLevel::EditFiles => "Edit files",
        PermissionLevel::FullAccess => "Full access",
        PermissionLevel::Custom => "Custom",
    }
}

impl AuditEvent {
    /// One line for the Session Log.
    pub fn message(&self) -> String {
        match self {
            Self::SessionStarted { host, permissions } => format!(
                "{host} started the session (guests: {})",
                level_title(permissions)
            ),
            Self::SessionEnded { reason } => format!("Session ended: {reason}"),
            Self::Knock {
                name,
                peer,
                outcome,
                permissions,
            } => match outcome {
                KnockOutcome::Accepted => format!(
                    "{name} joined ({})",
                    permissions.as_ref().map(level_title).unwrap_or("Read-only")
                ),
                KnockOutcome::Declined => format!("Declined the request of {name}"),
                KnockOutcome::Unanswered => format!("The request of {name} was not answered"),
                KnockOutcome::WrongProof => {
                    format!("Dropped a connection without a valid link ({peer})")
                }
            },
            Self::Left { name, .. } => format!("{name} left"),
            Self::Removed { name, .. } => format!("Removed {name}"),
            Self::PermissionsChanged {
                name, permissions, ..
            } => match name {
                Some(n) => format!("Permission of {n}: {}", level_title(permissions)),
                None => format!("Permission of all guests: {}", level_title(permissions)),
            },
            Self::LinkRegenerated => "Regenerated the session link".into(),
            Self::File {
                name,
                action,
                path,
                to,
                ..
            } => match action {
                FileAction::Saved => format!("{name} saved {path}"),
                FileAction::Written => format!("{name} wrote {path}"),
                FileAction::Created => format!("{name} created {path}"),
                FileAction::CreatedFolder => format!("{name} created the folder {path}"),
                FileAction::Renamed => {
                    format!("{name} renamed {path} to {}", to.as_deref().unwrap_or("?"))
                }
                FileAction::Deleted => format!("{name} deleted {path}"),
            },
            Self::Git { name, action, .. } => format!("{name}: git {action}"),
            Self::Terminal {
                name,
                action,
                terminal,
                ..
            } => match action {
                TerminalAction::Shared => format!("{name} shared the terminal {terminal}"),
                TerminalAction::Unshared => {
                    format!("{name} stopped sharing the terminal {terminal}")
                }
                TerminalAction::Opened => format!("{name} opened the terminal {terminal}"),
                TerminalAction::Typed => format!("{name} typed in the terminal {terminal}"),
                TerminalAction::Closed => format!("{name} closed the terminal {terminal}"),
            },
            Self::Run {
                name, action, run, ..
            } => match action {
                RunAction::Started => format!("{name} ran '{run}'"),
                RunAction::Stopped => format!("{name} stopped '{run}'"),
                RunAction::Typed => format!("{name} typed in the console of '{run}'"),
            },
            Self::Denied {
                name,
                request,
                reason,
                ..
            } => format!("Refused {request} of {name}: {reason}"),
        }
    }

    /// Denied requests and dropped connections, shown as warnings.
    pub fn is_warning(&self) -> bool {
        matches!(
            self,
            Self::Denied { .. }
                | Self::Knock {
                    outcome: KnockOutcome::WrongProof,
                    ..
                }
        )
    }
}

/// One line of the log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    /// Unix time in milliseconds.
    pub time: u64,
    #[serde(flatten)]
    pub event: AuditEvent,
}

/// An entry as the Session Log shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub time: u64,
    pub message: String,
    pub warning: bool,
}

impl From<&AuditEntry> for LogLine {
    fn from(e: &AuditEntry) -> Self {
        Self {
            time: e.time,
            message: e.event.message(),
            warning: e.event.is_warning(),
        }
    }
}

/// Where the host's session writes its log.
pub trait AuditSink: Send + Sync {
    fn record(&self, event: AuditEvent);
}

pub type SharedAudit = Arc<dyn AuditSink>;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `YYYY-MM-DD` (UTC) of a Unix time in seconds.
pub fn date_of(unix_secs: u64) -> String {
    // Howard Hinnant's civil_from_days.
    let z = (unix_secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// The session log: a file plus the recent entries in memory. Without a file (the setting
/// is off) only the memory part is kept, so the Session Log still shows the session.
pub struct SessionLog {
    file: Mutex<Option<File>>,
    path: Option<PathBuf>,
    recent: Mutex<VecDeque<AuditEntry>>,
}

impl SessionLog {
    /// Only in memory.
    pub fn memory() -> Arc<Self> {
        Arc::new(Self {
            file: Mutex::new(None),
            path: None,
            recent: Mutex::new(VecDeque::new()),
        })
    }

    /// A new log file in `dir` for the session `session` (hex id); older logs are pruned.
    pub fn create(dir: &Path, session: &str) -> std::io::Result<Arc<Self>> {
        std::fs::create_dir_all(dir)?;
        prune(dir, Duration::from_secs(MAX_AGE_DAYS * 86_400));
        let secs = now_ms() / 1000;
        let path = dir.join(format!("{}-{session}.jsonl", date_of(secs)));
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Arc::new(Self {
            file: Mutex::new(Some(file)),
            path: Some(path),
            recent: Mutex::new(VecDeque::new()),
        }))
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The recent entries, oldest first.
    pub fn recent(&self) -> Vec<AuditEntry> {
        self.recent
            .lock()
            .expect("log lock poisoned")
            .iter()
            .cloned()
            .collect()
    }

    pub fn lines(&self) -> Vec<LogLine> {
        self.recent().iter().map(LogLine::from).collect()
    }
}

impl AuditSink for SessionLog {
    fn record(&self, event: AuditEvent) {
        let entry = AuditEntry {
            time: now_ms(),
            event,
        };
        if let Some(file) = self.file.lock().expect("log lock poisoned").as_mut() {
            if let Ok(mut line) = serde_json::to_vec(&entry) {
                line.push(b'\n');
                if let Err(e) = file.write_all(&line) {
                    tracing_warn(&format!("cannot write the session log: {e}"));
                }
            }
        }
        let mut recent = self.recent.lock().expect("log lock poisoned");
        if recent.len() == RECENT {
            recent.pop_front();
        }
        recent.push_back(entry);
    }
}

#[cfg(feature = "runtime")]
fn tracing_warn(msg: &str) {
    tracing::warn!("Code With Me: {msg}");
}

#[cfg(not(feature = "runtime"))]
fn tracing_warn(_msg: &str) {}

/// Delete the session logs in `dir` last changed more than `max_age` ago.
pub fn prune(dir: &Path, max_age: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("jsonl") {
            continue;
        }
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age > max_age);
        if old {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// A short form of an endpoint key for the log (first 8 bytes, hex).
pub fn short_peer(key: &[u8]) -> String {
    data_encoding::HEXLOWER.encode(&key[..key.len().min(8)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_utc_calendar_days() {
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(951_782_400), "2000-02-29");
        assert_eq!(date_of(1_793_318_400), "2026-10-30");
        assert_eq!(date_of(1_793_404_799), "2026-10-30");
    }

    #[test]
    fn entries_are_json_lines_with_a_message() {
        let dir = tempfile::tempdir().unwrap();
        let log = SessionLog::create(dir.path(), "abc123").unwrap();
        log.record(AuditEvent::SessionStarted {
            host: "Robert".into(),
            permissions: Permissions::READ_ONLY,
        });
        log.record(AuditEvent::Denied {
            participant: ParticipantId(1),
            name: "Alex".into(),
            request: "write_file".into(),
            reason: "Your permission is Read-only".into(),
        });
        let path = log.path().unwrap().to_path_buf();
        assert!(path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .ends_with("-abc123.jsonl"));
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<AuditEntry> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert!(text.contains("\"event\":\"denied\""), "{text}");
        let shown = log.lines();
        assert_eq!(
            shown[0].message,
            "Robert started the session (guests: Read-only)"
        );
        assert!(!shown[0].warning);
        assert_eq!(
            shown[1].message,
            "Refused write_file of Alex: Your permission is Read-only"
        );
        assert!(shown[1].warning);
    }

    #[test]
    fn memory_keeps_the_most_recent_entries() {
        let log = SessionLog::memory();
        for _ in 0..RECENT + 5 {
            log.record(AuditEvent::LinkRegenerated);
        }
        assert_eq!(log.recent().len(), RECENT);
        assert!(log.path().is_none());
    }

    #[test]
    fn old_logs_are_pruned_and_other_files_kept() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("old.jsonl"), "{}\n").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "keep").unwrap();
        prune(dir.path(), Duration::from_secs(3600));
        assert!(dir.path().join("old.jsonl").exists(), "fresh files stay");
        prune(dir.path(), Duration::ZERO);
        std::thread::sleep(Duration::from_millis(10));
        prune(dir.path(), Duration::ZERO);
        assert!(!dir.path().join("old.jsonl").exists());
        assert!(dir.path().join("notes.txt").exists());
    }

    #[test]
    fn messages_name_what_happened() {
        let renamed = AuditEvent::File {
            participant: ParticipantId(2),
            name: "Alex".into(),
            action: FileAction::Renamed,
            path: "a.php".into(),
            to: Some("b.php".into()),
        };
        assert_eq!(renamed.message(), "Alex renamed a.php to b.php");
        let knock = AuditEvent::Knock {
            name: String::new(),
            peer: "0102".into(),
            outcome: KnockOutcome::WrongProof,
            permissions: None,
        };
        assert!(knock.is_warning());
        let all = AuditEvent::PermissionsChanged {
            participant: None,
            name: None,
            permissions: Permissions::FULL_ACCESS,
        };
        assert_eq!(all.message(), "Permission of all guests: Full access");
        let typed = AuditEvent::Terminal {
            participant: Some(ParticipantId(2)),
            name: "Alex".into(),
            action: TerminalAction::Typed,
            terminal: "Local".into(),
        };
        assert_eq!(typed.message(), "Alex typed in the terminal Local");
        let json = serde_json::to_string(&typed).unwrap();
        assert!(json.contains("\"event\":\"terminal\""), "{json}");
        assert!(!json.contains("ls"), "no keys in the log: {json}");
        let run = AuditEvent::Run {
            participant: ParticipantId(2),
            name: "Alex".into(),
            action: RunAction::Started,
            run: "UserTest".into(),
        };
        assert_eq!(run.message(), "Alex ran 'UserTest'");
    }
}
