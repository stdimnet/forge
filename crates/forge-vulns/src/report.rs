//! The result of checking one [`Target`], what makes it, and the cache entry that saves
//! running the tool again while the manifest and lock file stay the same.

use serde::{Deserialize, Serialize};

use crate::model::Findings;
use crate::parse;
use crate::tool::{Target, Tool};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// The tool ran (or its cached result is still valid).
    Ok,
    /// It cannot run yet (no lock file).
    Blocked,
    /// The tool is not installed.
    Missing,
    /// The tool ran but its report could not be read.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetResult {
    #[serde(flatten)]
    pub target: Target,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Answered from the cache.
    #[serde(default)]
    pub cached: bool,
    /// Unix seconds of the run the findings come from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<u64>,
    #[serde(flatten)]
    pub findings: Findings,
}

impl TargetResult {
    pub fn new(target: Target, status: Status, message: Option<String>) -> Self {
        TargetResult {
            target,
            status,
            message,
            cached: false,
            checked_at: None,
            findings: Findings::default(),
        }
    }

    pub fn blocked(target: Target) -> Self {
        let message = target.blocked.clone();
        Self::new(target, Status::Blocked, message)
    }

    pub fn missing(target: Target) -> Self {
        let message = Some(format!(
            "{} is not installed. {}",
            target.tool.label(),
            target.tool.install_hint()
        ));
        Self::new(target, Status::Missing, message)
    }
}

/// Whether the output says the tool itself is missing (`No module named pip_audit`,
/// `no such command: audit`).
pub fn tool_missing(tool: Tool, stdout: &str, stderr: &str) -> bool {
    let text = format!("{stdout}\n{stderr}");
    match tool {
        Tool::PipAudit => text.contains("No module named pip_audit"),
        Tool::CargoAudit => text.contains("no such command") || text.contains("no such subcommand"),
        Tool::Composer => text.contains("Command \"audit\" is not defined"),
        _ => false,
    }
}

/// The result of a run: audit tools exit with an error code when they find something, so
/// only an unreadable report counts as a failure (with the tool's last error line).
pub fn from_output(target: Target, stdout: &str, stderr: &str, checked_at: u64) -> TargetResult {
    if tool_missing(target.tool, stdout, stderr) {
        return TargetResult::missing(target);
    }
    match parse::parse(target.tool, stdout) {
        Ok(findings) => TargetResult {
            status: Status::Ok,
            message: None,
            cached: false,
            checked_at: Some(checked_at),
            findings,
            target,
        },
        Err(e) => {
            let last = stderr
                .lines()
                .map(str::trim)
                .rfind(|l| !l.is_empty())
                .map(str::to_string);
            let message = match last {
                Some(l) if stdout.trim().is_empty() => l,
                _ => e,
            };
            TargetResult::new(target, Status::Failed, Some(message))
        }
    }
}

/// A cached result (`<cache dir>/forge/vulnerabilities/<project>/<tool>.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheEntry {
    /// [`crate::tool::cache_key`] of the inputs it was made from.
    pub key: String,
    pub checked_at: u64,
    pub findings: Findings,
}

impl CacheEntry {
    /// Valid for `key` and younger than `max_age` seconds.
    pub fn is_fresh(&self, key: &str, now: u64, max_age: u64) -> bool {
        self.key == key && now.saturating_sub(self.checked_at) < max_age
    }

    pub fn result(self, target: Target) -> TargetResult {
        TargetResult {
            target,
            status: Status::Ok,
            message: None,
            cached: true,
            checked_at: Some(self.checked_at),
            findings: self.findings,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(tool: Tool) -> Target {
        Target {
            tool,
            dir: String::new(),
            manifest: "x".into(),
            lockfile: None,
            annotate: vec![],
            inputs: vec![],
            blocked: None,
        }
    }

    #[test]
    fn classifies_runs() {
        let r = from_output(
            target(Tool::PipAudit),
            "",
            "/usr/bin/python3: No module named pip_audit",
            1,
        );
        assert_eq!(r.status, Status::Missing);
        let r = from_output(target(Tool::Npm), "", "npm ERR! something broke\n", 1);
        assert_eq!(r.status, Status::Failed);
        assert_eq!(r.message.as_deref(), Some("npm ERR! something broke"));
        let r = from_output(
            target(Tool::Npm),
            r#"{"auditReportVersion": 2, "vulnerabilities": {}}"#,
            "",
            7,
        );
        assert_eq!(r.status, Status::Ok);
        assert_eq!(r.checked_at, Some(7));
        let json = serde_json::to_value(&r).unwrap();
        assert_eq!(json["tool"], "npm");
        assert_eq!(json["vulnerabilities"], serde_json::json!([]));
    }

    #[test]
    fn cache_entries_expire() {
        let e = CacheEntry {
            key: "k".into(),
            checked_at: 100,
            findings: Findings::default(),
        };
        assert!(e.is_fresh("k", 150, 100));
        assert!(!e.is_fresh("k", 250, 100));
        assert!(!e.is_fresh("other", 150, 100));
        assert!(e.result(target(Tool::Npm)).cached);
    }
}
