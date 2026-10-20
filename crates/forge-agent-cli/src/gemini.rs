//! Gemini CLI adapter: `gemini --output-format stream-json`, one process per turn.
//!
//! Gemini CLI has no streaming *input* mode, so every user message is a separate headless
//! run: the prompt goes to stdin (which keeps long prompts and Windows `gemini.cmd` shims
//! free of quoting problems), stdin is closed, and the conversation continues with
//! `--resume <session id>` from the `init` event of the first turn.
//!
//! Its stdout is JSONL with `init`, `message`, `tool_use`, `tool_result`, `error` and
//! `result` events; unknown events and non-JSON lines are ignored. Tool calls are mapped to
//! Forge's common tool vocabulary (`read_file` → `Read`, `replace` → `Edit`,
//! `run_shell_command` → `Bash`, ...) so the chat renders them like any other agent's.
//!
//! Headless Gemini CLI cannot ask before running a tool: depending on `--approval-mode` it
//! simply does not offer edit and shell tools to the model. Forge's permission modes map to
//! approval modes (see [`approval_mode`]).
//!
//! The parser is pure (also used by the browser-mode mock through WebAssembly); the
//! adapter needs the `runtime` feature.

use serde_json::{Map, Value};

use crate::event::{AgentEvent, EventParser, Usage};
use crate::profile::PermissionMode;

/// Gemini CLI's `--approval-mode` for a Forge permission mode. Headless runs cannot ask,
/// so "ask" and plan mode both become `default`, in which edit and shell tools are not
/// available (read-only).
pub fn approval_mode(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Default | PermissionMode::Plan => "default",
        PermissionMode::AcceptEdits => "auto_edit",
        PermissionMode::BypassPermissions => "yolo",
    }
}

/// Parses Gemini CLI `--output-format stream-json` lines.
#[derive(Debug, Default)]
pub struct GeminiParser;

impl EventParser for GeminiParser {
    fn parse_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let line = line.trim();
        if !line.starts_with('{') {
            // Warnings and banners the CLI prints outside the protocol.
            return Vec::new();
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(String::from);
        let event = match s("type").as_deref() {
            Some("init") => AgentEvent::SessionStarted {
                session_id: s("session_id").unwrap_or_default(),
                model: s("model"),
                tools: Vec::new(),
                cwd: s("cwd"),
                permission_mode: None,
            },
            // The user's own message is echoed back; the chat already shows it.
            Some("message") if s("role").as_deref() == Some("assistant") => {
                let text = s("content").unwrap_or_default();
                if text.is_empty() {
                    return Vec::new();
                }
                AgentEvent::AssistantText {
                    text,
                    delta: v["delta"].as_bool().unwrap_or(false),
                    parent_tool_use_id: None,
                }
            }
            Some("tool_use") => {
                let (name, input) = normalize_tool(
                    &s("tool_name").unwrap_or_default(),
                    v.get("parameters").cloned().unwrap_or(Value::Null),
                );
                AgentEvent::ToolUse {
                    id: s("tool_id").unwrap_or_default(),
                    name,
                    input,
                    parent_tool_use_id: None,
                }
            }
            Some("tool_result") => {
                let is_error = s("status").as_deref() == Some("error");
                let error = error_message(&v["error"]);
                let content = if is_error {
                    error.or_else(|| s("output"))
                } else {
                    s("output")
                }
                .unwrap_or_default();
                AgentEvent::ToolResult {
                    tool_use_id: s("tool_id").unwrap_or_default(),
                    content,
                    is_error,
                }
            }
            // Warnings (e.g. a loop detection notice) are not errors of the turn.
            Some("error") if s("severity").as_deref() != Some("warning") => AgentEvent::Error {
                message: s("message").unwrap_or_else(|| "Gemini CLI reported an error".into()),
            },
            Some("result") => {
                let is_error = s("status").as_deref().is_some_and(|st| st != "success");
                let stats = &v["stats"];
                let num = |k: &str| stats.get(k).and_then(Value::as_u64);
                let cached = num("cached").unwrap_or(0);
                // `input_tokens` includes cached tokens; `input` (newer CLIs) excludes them.
                let input = num("input")
                    .unwrap_or_else(|| num("input_tokens").unwrap_or(0).saturating_sub(cached));
                let usage = stats.is_object().then(|| Usage {
                    input_tokens: input,
                    output_tokens: num("output_tokens").unwrap_or(0),
                    cache_read_tokens: cached,
                    cache_write_tokens: 0,
                });
                AgentEvent::TurnFinished {
                    result: if is_error {
                        error_message(&v["error"]).or_else(|| Some("Gemini CLI failed".into()))
                    } else {
                        None
                    },
                    is_error,
                    duration_ms: num("duration_ms"),
                    num_turns: None,
                    cost_usd: None,
                    usage,
                }
            }
            _ => return Vec::new(),
        };
        vec![event]
    }
}

/// `{"type": ..., "message": ...}` or a plain string.
fn error_message(e: &Value) -> Option<String> {
    match e {
        Value::String(s) => Some(s.clone()),
        Value::Object(o) => o.get("message").and_then(Value::as_str).map(String::from),
        _ => None,
    }
}

/// Map a Gemini CLI built-in tool to Forge's common tool name and input keys. Unknown
/// tools (MCP servers, `save_memory`, ...) keep their name and input.
pub fn normalize_tool(name: &str, input: Value) -> (String, Value) {
    let Value::Object(mut map) = input else {
        return (name.to_string(), input);
    };
    let rename = |map: &mut Map<String, Value>, from: &[&str], to: &str| {
        if map.contains_key(to) {
            return;
        }
        if let Some(v) = from.iter().find_map(|k| map.remove(*k)) {
            map.insert(to.to_string(), v);
        }
    };
    let common = match name {
        "read_file" => {
            rename(&mut map, &["absolute_path", "path"], "file_path");
            "Read"
        }
        "write_file" => {
            rename(&mut map, &["absolute_path", "path"], "file_path");
            "Write"
        }
        "replace" | "edit" => {
            rename(&mut map, &["absolute_path", "path"], "file_path");
            "Edit"
        }
        "run_shell_command" => "Bash",
        "glob" => {
            rename(&mut map, &["dir_path"], "path");
            "Glob"
        }
        "search_file_content" | "grep" | "grep_search" => {
            rename(&mut map, &["dir_path"], "path");
            "Grep"
        }
        "google_web_search" | "web_search" => "WebSearch",
        "web_fetch" => {
            // The tool takes a prompt that contains the URLs; show the first one.
            if !map.contains_key("url") {
                let prompt = map
                    .get("prompt")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if let Some(url) = prompt
                    .split_whitespace()
                    .find(|w| w.starts_with("http://") || w.starts_with("https://"))
                {
                    let url = url.trim_end_matches(['.', ',', ')', '"', '\'']).to_string();
                    map.insert("url".into(), Value::String(url));
                }
            }
            "WebFetch"
        }
        "write_todos" => {
            if let Some(Value::Array(todos)) = map.get_mut("todos") {
                for todo in todos.iter_mut().filter_map(Value::as_object_mut) {
                    rename(todo, &["description"], "content");
                }
            }
            "TodoWrite"
        }
        other => return (other.to_string(), Value::Object(map)),
    };
    (common.to_string(), Value::Object(map))
}

#[cfg(feature = "runtime")]
pub use adapter::GeminiCli;

#[cfg(feature = "runtime")]
mod adapter {
    use std::path::Path;

    use forge_core::Result;

    use super::{approval_mode, GeminiParser};
    use crate::cli::{AgentCli, AgentProfile, CommandSpec, EventParser, StartOptions, Transport};

    pub struct GeminiCli;

    impl AgentCli for GeminiCli {
        fn id(&self) -> &'static str {
            "gemini-cli"
        }

        fn name(&self) -> &'static str {
            "Gemini CLI"
        }

        fn executable_names(&self) -> &'static [&'static str] {
            &["gemini"]
        }

        fn profile(&self) -> AgentProfile {
            crate::profile::gemini_cli()
        }

        fn transport(&self) -> Transport {
            Transport::PerTurn
        }

        fn command(&self, executable: &Path, opts: &StartOptions) -> Result<CommandSpec> {
            let mut args: Vec<String> = vec!["--output-format".into(), "stream-json".into()];
            if let Some(model) = opts
                .model
                .as_deref()
                .filter(|m| !m.is_empty() && *m != "default")
            {
                args.extend(["--model".into(), model.into()]);
            }
            args.extend([
                "--approval-mode".into(),
                approval_mode(opts.permission_mode).into(),
            ]);
            if let Some(id) = opts.resume.as_deref().filter(|s| !s.is_empty()) {
                args.extend(["--resume".into(), id.into()]);
            }
            for tool in &opts.allowed_tools {
                args.extend(["--allowed-tools".into(), tool.clone()]);
            }
            Ok(CommandSpec {
                program: executable.to_path_buf(),
                args,
            })
        }

        /// The prompt itself: the whole of stdin is the message of the turn.
        fn encode_user_message(&self, text: &str) -> Result<String> {
            Ok(text.to_string())
        }

        fn parser(&self, _opts: &StartOptions) -> Box<dyn EventParser> {
            Box::new(GeminiParser)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(line: &str) -> Vec<AgentEvent> {
        GeminiParser.parse_line(line)
    }

    #[test]
    fn approval_modes() {
        assert_eq!(approval_mode(PermissionMode::Default), "default");
        assert_eq!(approval_mode(PermissionMode::Plan), "default");
        assert_eq!(approval_mode(PermissionMode::AcceptEdits), "auto_edit");
        assert_eq!(approval_mode(PermissionMode::BypassPermissions), "yolo");
    }

    #[test]
    fn maps_builtin_tools_to_the_common_vocabulary() {
        let (name, input) = normalize_tool(
            "read_file",
            json!({"absolute_path": "/p/a.php", "offset": 3}),
        );
        assert_eq!(
            (name.as_str(), input),
            ("Read", json!({"file_path": "/p/a.php", "offset": 3}))
        );
        let (name, input) = normalize_tool(
            "replace",
            json!({"file_path": "/p/a.php", "old_string": "a", "new_string": "b"}),
        );
        assert_eq!(name, "Edit");
        assert_eq!(input["old_string"], "a");
        assert_eq!(
            normalize_tool("run_shell_command", json!({"command": "ls"})).0,
            "Bash"
        );
        assert_eq!(
            normalize_tool(
                "search_file_content",
                json!({"pattern": "x", "dir_path": "app"})
            ),
            ("Grep".into(), json!({"pattern": "x", "path": "app"}))
        );
        assert_eq!(
            normalize_tool(
                "web_fetch",
                json!({"prompt": "Summarize https://laravel.com/docs."})
            )
            .1["url"],
            "https://laravel.com/docs"
        );
        assert_eq!(
            normalize_tool(
                "write_todos",
                json!({"todos": [{"description": "Add scope", "status": "in_progress"}]})
            ),
            (
                "TodoWrite".into(),
                json!({"todos": [{"content": "Add scope", "status": "in_progress"}]})
            )
        );
        assert_eq!(
            normalize_tool("mcp_tool", json!({"q": 1})),
            ("mcp_tool".into(), json!({"q": 1}))
        );
        assert_eq!(
            normalize_tool("read_file", Value::Null),
            ("read_file".into(), Value::Null)
        );
    }

    #[test]
    fn parses_results_errors_and_ignores_noise() {
        assert!(parse("Loaded cached credentials.").is_empty());
        assert!(parse(r#"{"type":"message","role":"user","content":"hi"}"#).is_empty());
        assert!(parse(r#"{"type":"something_new","x":1}"#).is_empty());
        assert!(parse(r#"{"type":"error","severity":"warning","message":"loop"}"#).is_empty());
        assert_eq!(
            parse(r#"{"type":"error","severity":"error","message":"quota exceeded"}"#),
            vec![AgentEvent::Error {
                message: "quota exceeded".into()
            }]
        );
        let ev = parse(
            r#"{"type":"result","status":"error","error":{"type":"FatalTurnLimitedError","message":"Reached max session turns"},"stats":{"total_tokens":10,"input_tokens":8,"output_tokens":2,"cached":6,"duration_ms":40,"tool_calls":0}}"#,
        );
        let AgentEvent::TurnFinished {
            result,
            is_error,
            usage,
            duration_ms,
            ..
        } = &ev[0]
        else {
            panic!("{ev:?}")
        };
        assert!(*is_error);
        assert_eq!(result.as_deref(), Some("Reached max session turns"));
        assert_eq!(*duration_ms, Some(40));
        let usage = usage.as_ref().unwrap();
        assert_eq!(
            (
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_read_tokens
            ),
            (2, 2, 6)
        );
    }

    #[cfg(feature = "runtime")]
    #[test]
    fn builds_turn_command_lines() {
        use crate::cli::{AgentCli, StartOptions, Transport};
        use std::path::Path;
        let opts = StartOptions {
            model: Some("gemini-2.5-pro".into()),
            permission_mode: PermissionMode::AcceptEdits,
            resume: Some("c2f1".into()),
            ..Default::default()
        };
        let spec = GeminiCli.command(Path::new("gemini"), &opts).unwrap();
        assert_eq!(
            spec.args.join(" "),
            "--output-format stream-json --model gemini-2.5-pro --approval-mode auto_edit --resume c2f1"
        );
        let spec = GeminiCli
            .command(
                Path::new("gemini"),
                &StartOptions {
                    model: Some("default".into()),
                    permission_mode: PermissionMode::Plan,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            spec.args.join(" "),
            "--output-format stream-json --approval-mode default"
        );
        assert_eq!(GeminiCli.transport(), Transport::PerTurn);
        assert_eq!(GeminiCli.encode_user_message("a\nb").unwrap(), "a\nb");
        assert!(GeminiCli.info(None).supported);
    }
}
