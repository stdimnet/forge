//! The adapter trait every agent CLI implements, plus start options and detection.

use std::path::{Path, PathBuf};

use forge_core::Result;
use serde::{Deserialize, Serialize};

use crate::event::AgentEvent;

/// How much the agent may do without asking. Mirrors Claude Code's permission modes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionMode {
    /// Ask before edits and commands (the CLI's own default).
    #[default]
    Default,
    /// Apply file edits without asking.
    AcceptEdits,
    /// Read-only planning, no edits.
    Plan,
    /// Never ask. Only for trusted, sandboxed projects.
    BypassPermissions,
}

impl PermissionMode {
    pub fn as_cli_arg(self) -> Option<&'static str> {
        match self {
            Self::Default => None,
            Self::AcceptEdits => Some("acceptEdits"),
            Self::Plan => Some("plan"),
            Self::BypassPermissions => Some("bypassPermissions"),
        }
    }
}

/// Options for starting an agent session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StartOptions {
    /// Model name or alias (`sonnet`, `opus`, ...). `None` uses the CLI's default.
    pub model: Option<String>,
    pub permission_mode: PermissionMode,
    /// Conversation id to resume (as reported in `SessionStarted`).
    pub resume: Option<String>,
    /// Tools allowed without prompting, e.g. `["Read", "Bash(git diff:*)"]`.
    pub allowed_tools: Vec<String>,
    /// Explicit executable path; otherwise the CLI is looked up on `PATH`.
    pub executable: Option<String>,
    /// Stream partial text deltas (smoother UI, more events).
    pub partial_messages: Option<bool>,
    /// Ask Forge (instead of failing the tool call) when a tool needs permission. On by
    /// default; the UI answers with [`PermissionDecision`]s.
    pub permission_prompts: Option<bool>,
}

impl StartOptions {
    pub fn partial(&self) -> bool {
        self.partial_messages.unwrap_or(true)
    }

    /// Whether permission requests are routed to Forge. Pointless when nothing is asked.
    pub fn prompts_for_permissions(&self) -> bool {
        self.permission_prompts.unwrap_or(true)
            && self.permission_mode != PermissionMode::BypassPermissions
    }
}

/// How long an "allow" answer to a permission request applies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AllowScope {
    /// Only this tool call.
    #[default]
    Once,
    /// Every matching call for the rest of the session (see [`session_rule`]).
    Session,
}

/// The user's answer to a permission request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "behavior",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PermissionDecision {
    Allow {
        /// Tool input to run with; `None` keeps the requested input.
        #[serde(default)]
        updated_input: Option<serde_json::Value>,
        #[serde(default)]
        scope: AllowScope,
    },
    Deny {
        /// Told to the agent, e.g. "The user denied this edit".
        #[serde(default)]
        message: Option<String>,
    },
}

/// Key under which an "allow for session" answer is remembered. Shell commands are only
/// allowed again verbatim; other tools by name.
pub fn session_rule(tool_name: &str, input: &serde_json::Value) -> String {
    match input.get("command").and_then(|c| c.as_str()) {
        Some(cmd) if tool_name == "Bash" => format!("Bash({})", cmd.trim()),
        _ => tool_name.to_string(),
    }
}

/// A command line to spawn: program + arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
}

/// Static description of an adapter, shown in the agent picker.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInfo {
    pub id: String,
    pub name: String,
    /// Whether Forge can run chat sessions with this CLI yet.
    pub supported: bool,
    /// Path of the detected executable, if installed.
    pub path: Option<String>,
    pub installed: bool,
    pub models: Vec<String>,
}

/// Turns raw stdout lines into normalized events. Stateful, one per session, so adapters
/// can de-duplicate streamed deltas against the final message.
pub trait EventParser: Send {
    fn parse_line(&mut self, line: &str) -> Vec<AgentEvent>;
}

/// An agent CLI Forge can drive headless (Claude Code, Gemini CLI, ...).
pub trait AgentCli: Send + Sync {
    /// Stable id used over IPC, e.g. `claude-code`.
    fn id(&self) -> &'static str;
    /// Display name, e.g. `Claude Code`.
    fn name(&self) -> &'static str;
    /// Executable base names to look for on `PATH` (without Windows extensions).
    fn executable_names(&self) -> &'static [&'static str];
    /// Model aliases offered in the model picker.
    fn models(&self) -> Vec<String>;
    /// Whether chat sessions are implemented for this CLI.
    fn supported(&self) -> bool {
        true
    }
    /// Command line for a long-running session process.
    fn command(&self, executable: &Path, opts: &StartOptions) -> Result<CommandSpec>;
    /// Encode one user message as a line for the process' stdin (without newline).
    fn encode_user_message(&self, text: &str) -> Result<String>;
    /// A fresh output parser for one session.
    fn parser(&self, opts: &StartOptions) -> Box<dyn EventParser>;

    /// Encode the answer to a permission request as a stdin line (without newline).
    /// `input` is the tool input of the request.
    fn encode_permission_response(
        &self,
        _request_id: &str,
        _input: &serde_json::Value,
        _decision: &PermissionDecision,
    ) -> Result<String> {
        Err(forge_core::ForgeError::Other(format!(
            "{} does not support permission prompts",
            self.name()
        )))
    }

    /// Locate the executable: explicit override first, then `PATH` and well-known
    /// install locations.
    fn detect(&self, override_path: Option<&str>) -> Option<PathBuf> {
        if let Some(p) = override_path.filter(|p| !p.trim().is_empty()) {
            let p = PathBuf::from(p);
            return p.is_file().then_some(p);
        }
        find_executable(self.executable_names())
    }

    fn info(&self, override_path: Option<&str>) -> AgentInfo {
        let path = self.detect(override_path);
        AgentInfo {
            id: self.id().into(),
            name: self.name().into(),
            supported: self.supported(),
            installed: path.is_some(),
            path: path.map(|p| p.display().to_string()),
            models: self.models(),
        }
    }
}

/// Search `PATH` (and a few user-level install dirs) for one of `names`. On Windows the
/// `PATHEXT` extensions (`.cmd`, `.exe`, ...) are tried, so npm shims like `claude.cmd`
/// are found.
pub fn find_executable(names: &[&str]) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }) {
        let home = PathBuf::from(home);
        dirs.push(home.join(".claude").join("local"));
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".npm-global").join("bin"));
        if cfg!(windows) {
            if let Some(appdata) = std::env::var_os("APPDATA") {
                dirs.push(PathBuf::from(appdata).join("npm"));
            }
        }
    }
    find_in_dirs(names, &dirs, &exe_extensions())
}

fn exe_extensions() -> Vec<String> {
    if cfg!(windows) {
        let pathext = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        pathext
            .split(';')
            .filter(|e| !e.is_empty())
            .map(|e| e.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    }
}

/// Testable core of [`find_executable`].
pub fn find_in_dirs(names: &[&str], dirs: &[PathBuf], extensions: &[String]) -> Option<PathBuf> {
    for dir in dirs {
        for name in names {
            for ext in extensions {
                let candidate = dir.join(format!("{name}{ext}"));
                if is_executable(&candidate) {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn is_executable(p: &Path) -> bool {
    let Ok(meta) = p.metadata() else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_executable_with_extension() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("claude.cmd");
        std::fs::write(&file, "@echo off").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let exts = vec![".exe".to_string(), ".cmd".to_string()];
        let found = find_in_dirs(&["claude"], &[dir.path().to_path_buf()], &exts);
        assert_eq!(found, Some(file));
        assert_eq!(
            find_in_dirs(&["gemini"], &[dir.path().to_path_buf()], &exts),
            None
        );
    }

    #[test]
    fn decisions_and_session_rules() {
        let d: PermissionDecision =
            serde_json::from_str(r#"{"behavior":"allow","scope":"session"}"#).unwrap();
        assert_eq!(
            d,
            PermissionDecision::Allow {
                updated_input: None,
                scope: AllowScope::Session
            }
        );
        let d: PermissionDecision = serde_json::from_str(r#"{"behavior":"deny"}"#).unwrap();
        assert_eq!(d, PermissionDecision::Deny { message: None });
        let bash = serde_json::json!({"command": " ls -la "});
        assert_eq!(session_rule("Bash", &bash), "Bash(ls -la)");
        assert_eq!(session_rule("Edit", &bash), "Edit");
        let bypass = StartOptions {
            permission_mode: PermissionMode::BypassPermissions,
            ..Default::default()
        };
        assert!(!bypass.prompts_for_permissions());
        assert!(StartOptions::default().prompts_for_permissions());
    }

    #[test]
    fn permission_mode_args() {
        assert_eq!(PermissionMode::Default.as_cli_arg(), None);
        assert_eq!(PermissionMode::Plan.as_cli_arg(), Some("plan"));
        let m: PermissionMode = serde_json::from_str("\"acceptEdits\"").unwrap();
        assert_eq!(m, PermissionMode::AcceptEdits);
    }
}
