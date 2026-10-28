//! Whether a tool call may run: the allowed tool groups, "require approval for write/exec
//! tools", Safe Mode (no project code in untrusted projects), the project's AI switch
//! (`ai.enabled`) and `.aiignore` (via `forge_ai_editor`'s policy). Results that list files
//! are filtered so excluded files never reach the client.

use std::collections::BTreeSet;

use forge_ai_editor::AiIgnore;
use serde::Serialize;
use serde_json::Value;

use crate::tools::{ToolGroup, ToolSpec};

/// A snapshot of the settings and project state that decide about calls.
#[derive(Debug, Clone)]
pub struct CallPolicy {
    /// A project is open (every tool works on the open project).
    pub project_open: bool,
    pub enabled_groups: BTreeSet<ToolGroup>,
    /// `mcp.requireApproval`: ask before write and exec tools.
    pub require_approval: bool,
    /// The project is untrusted (Safe Mode).
    pub safe_mode: bool,
    /// `ai.enabled` of the project.
    pub ai_enabled: bool,
    /// `.aiignore` / `.aiexclude` of the project.
    pub ignore: AiIgnore,
}

impl Default for CallPolicy {
    fn default() -> Self {
        Self {
            project_open: true,
            enabled_groups: ToolGroup::ALL
                .into_iter()
                .filter(|g| g.default_enabled())
                .collect(),
            require_approval: true,
            safe_mode: false,
            ai_enabled: true,
            ignore: AiIgnore::default(),
        }
    }
}

/// The verdict for one call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "decision", rename_all = "camelCase")]
pub enum Decision {
    Allow,
    /// The user must approve the call first.
    Ask,
    /// Refused; the message goes back to the client.
    Deny { reason: String },
}

fn deny(reason: impl Into<String>) -> Decision {
    Decision::Deny {
        reason: reason.into(),
    }
}

/// A project-relative path in normal form (`/` separators, no `./`), or why it is refused.
pub fn normalize_path(path: &str) -> Result<String, String> {
    let p = path.trim().replace('\\', "/");
    let looks_absolute = p.starts_with('/')
        || p.starts_with('~')
        || p.as_bytes().get(1) == Some(&b':')
        || p.contains("://");
    if looks_absolute {
        return Err(format!(
            "{path}: use a path relative to the project root"
        ));
    }
    let mut parts = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => return Err(format!("{path}: paths outside the project are not allowed")),
            s => parts.push(s),
        }
    }
    Ok(parts.join("/"))
}

/// Paths named by a call's arguments (strings or arrays of strings).
pub fn path_arguments(spec: &ToolSpec, args: &Value) -> Vec<String> {
    let mut out = Vec::new();
    for name in spec.path_args {
        match args.get(*name) {
            Some(Value::String(s)) => out.push(s.clone()),
            Some(Value::Array(items)) => {
                out.extend(items.iter().filter_map(Value::as_str).map(str::to_string))
            }
            _ => {}
        }
    }
    out
}

impl CallPolicy {
    /// Whether `tools/list` shows the tool.
    pub fn lists(&self, spec: &ToolSpec) -> bool {
        self.enabled_groups.contains(&spec.group)
    }

    pub fn is_ignored(&self, path: &str) -> bool {
        normalize_path(path).is_ok_and(|p| !p.is_empty() && self.ignore.is_ignored(&p))
    }

    /// Decide about a call (before any approval).
    pub fn decide(&self, spec: &ToolSpec, args: &Value) -> Decision {
        if !self.enabled_groups.contains(&spec.group) {
            return deny(format!(
                "{} is turned off in Forge (Settings | Tools | MCP Server: {})",
                spec.name,
                spec.group.title()
            ));
        }
        if !self.ai_enabled {
            return deny("AI features are turned off for this project in Forge");
        }
        if !self.project_open {
            return deny("No project is open in Forge");
        }
        if spec.runs_project_code && self.safe_mode {
            return deny(format!(
                "{} runs project code, which Forge does not allow in Safe Mode. Trust the project in Forge first.",
                spec.name
            ));
        }
        for path in path_arguments(spec, args) {
            let normal = match normalize_path(&path) {
                Ok(p) => p,
                Err(e) => return deny(e),
            };
            if !normal.is_empty() && self.ignore.is_ignored(&normal) {
                return deny(format!("{normal} is excluded from AI by .aiignore"));
            }
        }
        if spec.always_asks() || (self.require_approval && spec.needs_approval()) {
            return Decision::Ask;
        }
        Decision::Allow
    }

    /// Drop entries of `value` (array items, object fields keyed by path) that name a file
    /// excluded by `.aiignore`. Returns how many were removed.
    pub fn filter_result(&self, value: &mut Value) -> usize {
        if self.ignore.is_empty() {
            return 0;
        }
        self.filter(value)
    }

    fn names_ignored(&self, v: &Value) -> bool {
        let Some(obj) = v.as_object() else {
            return v.as_str().is_some_and(|s| self.is_ignored(s));
        };
        ["path", "file", "relativePath"]
            .iter()
            .filter_map(|k| obj.get(*k).and_then(Value::as_str))
            .any(|p| self.is_ignored(p))
    }

    fn filter(&self, value: &mut Value) -> usize {
        match value {
            Value::Array(items) => {
                let before = items.len();
                items.retain(|v| !self.names_ignored(v));
                let mut removed = before - items.len();
                for v in items.iter_mut() {
                    removed += self.filter(v);
                }
                removed
            }
            Value::Object(map) => {
                let keys: Vec<String> = map
                    .keys()
                    .filter(|k| k.contains('.') || k.contains('/'))
                    .filter(|k| self.is_ignored(k))
                    .cloned()
                    .collect();
                let mut removed = keys.len();
                for k in keys {
                    map.remove(&k);
                }
                for v in map.values_mut() {
                    removed += self.filter(v);
                }
                removed
            }
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::find;
    use serde_json::json;

    fn policy() -> CallPolicy {
        CallPolicy {
            ignore: AiIgnore::parse(".env\nsecrets/\n"),
            ..CallPolicy::default()
        }
    }

    #[test]
    fn read_tools_run_without_asking() {
        let p = policy();
        assert_eq!(
            p.decide(&find("read_file").unwrap(), &json!({ "path": "src/a.php" })),
            Decision::Allow
        );
    }

    #[test]
    fn write_and_exec_tools_ask_unless_turned_off() {
        let mut p = policy();
        p.enabled_groups.insert(ToolGroup::Terminal);
        let edit = find("replace_text_in_file").unwrap();
        let args = json!({ "path": "a.php", "oldText": "a", "newText": "b" });
        assert_eq!(p.decide(&edit, &args), Decision::Ask);
        p.require_approval = false;
        assert_eq!(p.decide(&edit, &args), Decision::Allow);
        // Terminal commands always ask.
        let term = find("execute_terminal_command").unwrap();
        assert_eq!(p.decide(&term, &json!({ "command": "ls" })), Decision::Ask);
    }

    #[test]
    fn disabled_groups_and_ai_switch_refuse() {
        let mut p = policy();
        let term = find("execute_terminal_command").unwrap();
        assert!(matches!(
            p.decide(&term, &json!({ "command": "ls" })),
            Decision::Deny { reason } if reason.contains("turned off")
        ));
        assert!(!p.lists(&term));
        p.ai_enabled = false;
        assert!(matches!(
            p.decide(&find("git_status").unwrap(), &json!({})),
            Decision::Deny { reason } if reason.contains("AI features")
        ));
    }

    #[test]
    fn safe_mode_refuses_project_code() {
        let mut p = policy();
        p.safe_mode = true;
        p.require_approval = false;
        assert!(matches!(
            p.decide(&find("run_configuration").unwrap(), &json!({ "name": "tests" })),
            Decision::Deny { reason } if reason.contains("Safe Mode")
        ));
        // Reading and editing still work in Safe Mode.
        assert_eq!(
            p.decide(&find("read_file").unwrap(), &json!({ "path": "a" })),
            Decision::Allow
        );
    }

    #[test]
    fn aiignore_and_project_boundary() {
        let p = policy();
        let read = find("read_file").unwrap();
        for (path, why) in [
            (".env", "excluded from AI"),
            ("./secrets/key.pem", "excluded from AI"),
            ("../other/x", "outside the project"),
            ("/etc/passwd", "relative to the project root"),
            ("C:\\Windows\\win.ini", "relative to the project root"),
        ] {
            assert!(
                matches!(p.decide(&read, &json!({ "path": path })), Decision::Deny { reason } if reason.contains(why)),
                "{path}"
            );
        }
        let diff = find("git_diff").unwrap();
        assert!(matches!(
            p.decide(&diff, &json!({ "paths": ["a.php", ".env"] })),
            Decision::Deny { .. }
        ));
        assert_eq!(normalize_path("./src//a.php").unwrap(), "src/a.php");
    }

    #[test]
    fn filters_excluded_files_from_results() {
        let p = policy();
        let mut v = json!({
            "files": [
                { "path": "src/a.php", "matches": [] },
                { "path": "secrets/db.php", "matches": [] },
                { "path": ".env", "matches": [] },
            ],
            "byFile": { "src/a.php": 1, ".env": 2 },
            "names": ["src/b.php", "deploy/.env"],
        });
        assert_eq!(p.filter_result(&mut v), 4);
        assert_eq!(v["files"].as_array().unwrap().len(), 1);
        assert!(v["byFile"].get(".env").is_none());
        assert_eq!(v["names"], json!(["src/b.php"]));
    }
}
