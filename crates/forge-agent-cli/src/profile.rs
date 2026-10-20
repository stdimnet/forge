//! Static descriptions of the built-in agent CLIs: models, permission modes as each CLI
//! understands them, and how to install it. Pure, so the browser-mode mock shows the same
//! picker entries as the desktop app.

use serde::{Deserialize, Serialize};

/// How much the agent may do without asking. Forge's vocabulary (Claude Code's permission
/// modes); each adapter maps it to its CLI's own flags.
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

/// A permission mode as offered for one agent in the chat's mode picker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionModeInfo {
    pub value: PermissionMode,
    /// Menu entry, e.g. "Ask before edits".
    pub label: String,
    /// Toolbar text, e.g. "Auto-accept".
    pub short: String,
    /// What the mode allows, shown under the label.
    pub hint: String,
}

/// Static description of an agent CLI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfile {
    /// Stable id used over IPC and in setting keys, e.g. `gemini-cli`.
    pub id: String,
    pub name: String,
    /// Model names offered in the model picker; `default` lets the CLI decide.
    pub models: Vec<String>,
    /// Command that installs the CLI, shown when it is not found.
    pub install_hint: String,
    /// Whether the CLI can ask Forge before running a tool. Without it, the permission mode
    /// alone decides which tools the agent may use.
    pub permission_prompts: bool,
    /// The modes this CLI supports, in menu order.
    pub permission_modes: Vec<PermissionModeInfo>,
}

fn mode(value: PermissionMode, label: &str, short: &str, hint: &str) -> PermissionModeInfo {
    PermissionModeInfo {
        value,
        label: label.into(),
        short: short.into(),
        hint: hint.into(),
    }
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

pub fn claude_code() -> AgentProfile {
    AgentProfile {
        id: "claude-code".into(),
        name: "Claude Code".into(),
        models: strings(&["default", "sonnet", "opus", "haiku"]),
        install_hint: "npm install -g @anthropic-ai/claude-code".into(),
        permission_prompts: true,
        permission_modes: vec![
            mode(
                PermissionMode::Default,
                "Ask before edits",
                "Ask before edits",
                "Confirm every edit and command",
            ),
            mode(
                PermissionMode::AcceptEdits,
                "Auto-accept edits",
                "Auto-accept",
                "Apply file edits without asking",
            ),
            mode(
                PermissionMode::Plan,
                "Plan mode",
                "Plan",
                "Explore and plan, no changes",
            ),
            mode(
                PermissionMode::BypassPermissions,
                "Bypass permissions",
                "Bypass",
                "Never ask — trusted projects only",
            ),
        ],
    }
}

/// Gemini CLI cannot ask in headless mode: tools that would need a confirmation are not
/// offered to the model at all, so the approval mode decides what the agent can do.
pub fn gemini_cli() -> AgentProfile {
    AgentProfile {
        id: "gemini-cli".into(),
        name: "Gemini CLI".into(),
        models: strings(&[
            "default",
            "gemini-2.5-pro",
            "gemini-2.5-flash",
            "gemini-2.5-flash-lite",
        ]),
        install_hint: "npm install -g @google/gemini-cli".into(),
        permission_prompts: false,
        permission_modes: vec![
            mode(
                PermissionMode::Default,
                "Read-only",
                "Read-only",
                "Read and search, no edits or commands",
            ),
            mode(
                PermissionMode::AcceptEdits,
                "Auto-accept edits",
                "Auto-accept",
                "Edit files without asking, no commands",
            ),
            mode(
                PermissionMode::BypassPermissions,
                "YOLO",
                "YOLO",
                "Edits and commands, never asks",
            ),
        ],
    }
}

/// Profiles of all built-in agents, in picker order.
pub fn builtin_profiles() -> Vec<AgentProfile> {
    vec![claude_code(), gemini_cli()]
}

/// The profile of a built-in agent.
pub fn profile_by_id(id: &str) -> Option<AgentProfile> {
    builtin_profiles().into_iter().find(|p| p.id == id)
}

impl AgentProfile {
    /// `mode` if this agent offers it, else the closest mode it has (plan mode falls back to
    /// the most restrictive mode).
    pub fn effective_mode(&self, mode: PermissionMode) -> PermissionMode {
        if self.permission_modes.iter().any(|m| m.value == mode) {
            return mode;
        }
        self.permission_modes
            .first()
            .map_or(PermissionMode::Default, |m| m.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_serialize_for_the_ui() {
        let v = serde_json::to_value(gemini_cli()).unwrap();
        assert_eq!(v["installHint"], "npm install -g @google/gemini-cli");
        assert_eq!(v["permissionPrompts"], false);
        assert_eq!(v["permissionModes"][1]["value"], "acceptEdits");
        assert_eq!(
            builtin_profiles()
                .iter()
                .map(|p| p.id.as_str())
                .collect::<Vec<_>>(),
            ["claude-code", "gemini-cli"]
        );
        assert_eq!(claude_code().permission_modes.len(), 4);
    }

    #[test]
    fn unsupported_modes_fall_back_to_the_most_restrictive() {
        let gemini = gemini_cli();
        assert_eq!(
            gemini.effective_mode(PermissionMode::Plan),
            PermissionMode::Default
        );
        assert_eq!(
            gemini.effective_mode(PermissionMode::BypassPermissions),
            PermissionMode::BypassPermissions
        );
        assert_eq!(
            claude_code().effective_mode(PermissionMode::Plan),
            PermissionMode::Plan
        );
    }
}
