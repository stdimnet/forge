//! Permission requests of agent CLIs (Claude Code, Gemini CLI) working in a sandbox: file
//! tools are allowed inside the sandbox (writes inside the task's scope), shell commands run
//! when allowlisted or approved, network tools are refused.

use serde_json::Value;

use crate::commands::{allowlisted, refused, split_command};
use crate::workspace::Workspace;

/// What a CLI tool does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolClass {
    Read,
    Write,
    Shell,
    Network,
    /// Planning and bookkeeping tools that touch nothing (todo lists, sub-agents).
    Harmless,
    Unknown,
}

/// Classify a tool name of Claude Code or Gemini CLI.
pub fn classify(tool: &str) -> ToolClass {
    match tool {
        "Read"
        | "Glob"
        | "Grep"
        | "LS"
        | "NotebookRead"
        | "read_file"
        | "read_many_files"
        | "list_directory"
        | "glob"
        | "search_file_content" => ToolClass::Read,
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" | "write_file" | "replace" => {
            ToolClass::Write
        }
        "Bash" | "run_shell_command" => ToolClass::Shell,
        "WebFetch" | "WebSearch" | "web_fetch" | "google_web_search" => ToolClass::Network,
        "TodoWrite" | "Task" | "ExitPlanMode" | "BashOutput" | "KillShell" | "save_memory" => {
            ToolClass::Harmless
        }
        _ => ToolClass::Unknown,
    }
}

/// The answer to a CLI's permission request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliVerdict {
    Allow,
    /// Refused, with the message told to the agent.
    Deny(String),
    /// Ask the user whether this command may run.
    Ask(String),
}

const PATH_KEYS: [&str; 5] = [
    "file_path",
    "path",
    "notebook_path",
    "absolute_path",
    "dir_path",
];

/// Decide a permission request of an agent CLI running in `ws`.
pub fn permission(tool: &str, input: &Value, ws: &Workspace, allowlist: &[String]) -> CliVerdict {
    let paths: Vec<&str> = PATH_KEYS
        .iter()
        .filter_map(|k| input.get(*k).and_then(Value::as_str))
        .chain(
            input
                .get("paths")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        )
        .collect();
    let check_paths = |write: bool| -> CliVerdict {
        for p in &paths {
            let rel = match ws.resolve(p) {
                Ok((rel, _)) => rel,
                Err(e) => return CliVerdict::Deny(e),
            };
            if write && !ws.in_scope(&rel) {
                return CliVerdict::Deny(format!(
                    "{rel} is outside the task's scope ({})",
                    ws.scope().join(", ")
                ));
            }
        }
        CliVerdict::Allow
    };
    match classify(tool) {
        ToolClass::Read => check_paths(false),
        ToolClass::Write if paths.is_empty() => CliVerdict::Deny("no file path given".into()),
        ToolClass::Write => check_paths(true),
        ToolClass::Harmless => CliVerdict::Allow,
        ToolClass::Network => {
            CliVerdict::Deny("Network tools are not available in agent tasks.".into())
        }
        ToolClass::Shell => {
            let command = input
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string();
            match split_command(&command) {
                // A shell runs CLI commands, so operators are possible; let the user judge.
                Err(_) if !command.is_empty() => CliVerdict::Ask(command),
                Err(e) => CliVerdict::Deny(e),
                Ok(words) => {
                    if let Some(why) = refused(&words) {
                        CliVerdict::Deny(why)
                    } else if allowlisted(&words, allowlist) {
                        CliVerdict::Allow
                    } else {
                        CliVerdict::Ask(command)
                    }
                }
            }
        }
        ToolClass::Unknown => CliVerdict::Deny(format!("{tool} is not available in agent tasks.")),
    }
}

#[cfg(test)]
mod tests {
    use forge_ai_editor::AiIgnore;
    use serde_json::json;

    use super::*;

    #[test]
    fn maps_cli_tools_to_the_sandbox() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        let ws = Workspace::new(dir.path(), AiIgnore::parse(".env"), vec!["src".into()]);
        let allow = vec!["composer test".to_string()];
        let inside = dir.path().join("src/A.php").to_string_lossy().into_owned();
        assert_eq!(
            permission("Read", &json!({ "file_path": inside }), &ws, &allow),
            CliVerdict::Allow
        );
        assert_eq!(
            permission("Edit", &json!({ "file_path": inside }), &ws, &allow),
            CliVerdict::Allow
        );
        assert_eq!(
            permission("Grep", &json!({ "pattern": "x" }), &ws, &allow),
            CliVerdict::Allow
        );
        assert!(matches!(
            permission("Write", &json!({ "file_path": "README.md" }), &ws, &allow),
            CliVerdict::Deny(m) if m.contains("scope")
        ));
        assert!(matches!(
            permission("Read", &json!({ "file_path": "/etc/passwd" }), &ws, &allow),
            CliVerdict::Deny(m) if m.contains("outside")
        ));
        assert!(matches!(
            permission("read_file", &json!({ "absolute_path": dir.path().join(".env").to_string_lossy() }), &ws, &allow),
            CliVerdict::Deny(m) if m.contains(".aiignore")
        ));
        assert_eq!(
            permission(
                "Bash",
                &json!({ "command": "composer test --filter A" }),
                &ws,
                &allow
            ),
            CliVerdict::Allow
        );
        assert_eq!(
            permission("Bash", &json!({ "command": "rm -rf src" }), &ws, &allow),
            CliVerdict::Ask("rm -rf src".into())
        );
        assert_eq!(
            permission("Bash", &json!({ "command": "ls | wc -l" }), &ws, &allow),
            CliVerdict::Ask("ls | wc -l".into())
        );
        assert!(matches!(
            permission("Bash", &json!({ "command": "wget x" }), &ws, &allow),
            CliVerdict::Deny(_)
        ));
        assert!(matches!(
            permission("WebFetch", &json!({ "url": "x" }), &ws, &allow),
            CliVerdict::Deny(_)
        ));
        assert!(matches!(
            permission("mcp__x__y", &json!({}), &ws, &allow),
            CliVerdict::Deny(_)
        ));
        assert_eq!(
            permission("TodoWrite", &json!({}), &ws, &allow),
            CliVerdict::Allow
        );
    }
}
