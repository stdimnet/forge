//! Claude Code adapter: `claude -p --input-format stream-json --output-format stream-json`.
//!
//! The process stays alive for the whole conversation: every user message is one JSON line
//! on stdin, and stdout is JSONL with `system` (init), `stream_event` (partial deltas),
//! `assistant`, `user` (tool results) and `result` messages.

use std::collections::HashSet;
use std::path::Path;

use forge_core::Result;
use serde_json::{json, Value};

use crate::cli::{AgentCli, CommandSpec, EventParser, PermissionDecision, StartOptions};
use crate::event::{AgentEvent, Usage};

pub struct ClaudeCode;

impl AgentCli for ClaudeCode {
    fn id(&self) -> &'static str {
        "claude-code"
    }

    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn executable_names(&self) -> &'static [&'static str] {
        &["claude"]
    }

    fn models(&self) -> Vec<String> {
        ["default", "sonnet", "opus", "haiku"]
            .map(String::from)
            .to_vec()
    }

    fn command(&self, executable: &Path, opts: &StartOptions) -> Result<CommandSpec> {
        let mut args: Vec<String> = [
            "-p",
            "--output-format",
            "stream-json",
            "--input-format",
            "stream-json",
            "--verbose",
        ]
        .map(String::from)
        .to_vec();
        if opts.partial() {
            args.push("--include-partial-messages".into());
        }
        if let Some(model) = opts
            .model
            .as_deref()
            .filter(|m| !m.is_empty() && *m != "default")
        {
            args.extend(["--model".into(), model.into()]);
        }
        if let Some(mode) = opts.permission_mode.as_cli_arg() {
            args.extend(["--permission-mode".into(), mode.into()]);
        }
        if opts.prompts_for_permissions() {
            // Permission prompts arrive as `control_request`s on stdout and are answered
            // with `control_response`s on stdin.
            args.extend(["--permission-prompt-tool".into(), "stdio".into()]);
        }
        if let Some(id) = opts.resume.as_deref().filter(|s| !s.is_empty()) {
            args.extend(["--resume".into(), id.into()]);
        }
        if !opts.allowed_tools.is_empty() {
            args.push("--allowedTools".into());
            args.extend(opts.allowed_tools.iter().cloned());
        }
        Ok(CommandSpec {
            program: executable.to_path_buf(),
            args,
        })
    }

    fn encode_user_message(&self, text: &str) -> Result<String> {
        Ok(json!({
            "type": "user",
            "message": { "role": "user", "content": [{ "type": "text", "text": text }] }
        })
        .to_string())
    }

    fn parser(&self, _opts: &StartOptions) -> Box<dyn EventParser> {
        Box::new(ClaudeParser::default())
    }

    fn encode_permission_response(
        &self,
        request_id: &str,
        input: &Value,
        decision: &PermissionDecision,
    ) -> Result<String> {
        let result = match decision {
            PermissionDecision::Allow { updated_input, .. } => json!({
                "behavior": "allow",
                "updatedInput": updated_input.as_ref().unwrap_or(input),
            }),
            PermissionDecision::Deny { message } => json!({
                "behavior": "deny",
                "message": message.as_deref().unwrap_or("The user denied this tool call."),
            }),
        };
        Ok(json!({
            "type": "control_response",
            "response": { "subtype": "success", "request_id": request_id, "response": result }
        })
        .to_string())
    }
}

/// Parses Claude Code's stream-json output.
#[derive(Default)]
pub struct ClaudeParser {
    /// Message ids whose text/thinking was already streamed via `stream_event` deltas, so
    /// the final `assistant` message must not repeat it.
    streamed: HashSet<String>,
    /// Id of the message currently being streamed.
    current_message: Option<String>,
}

impl EventParser for ClaudeParser {
    fn parse_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let line = line.trim();
        if line.is_empty() {
            return Vec::new();
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            // Non-JSON output (warnings etc.) is not part of the protocol; ignore it.
            tracing::debug!("claude: ignoring non-JSON line: {line}");
            return Vec::new();
        };
        match str_at(&v, "type") {
            Some("system") => self.system(&v),
            Some("stream_event") => self.stream_event(&v),
            Some("assistant") => self.assistant(&v),
            Some("user") => tool_results(&v),
            Some("result") => vec![result(&v)],
            Some("control_request") => permission_request(&v),
            _ => Vec::new(),
        }
    }
}

impl ClaudeParser {
    fn system(&mut self, v: &Value) -> Vec<AgentEvent> {
        if str_at(v, "subtype") != Some("init") {
            return Vec::new();
        }
        vec![AgentEvent::SessionStarted {
            session_id: str_at(v, "session_id").unwrap_or_default().into(),
            model: str_at(v, "model").map(String::from),
            tools: v["tools"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            cwd: str_at(v, "cwd").map(String::from),
            permission_mode: str_at(v, "permissionMode").map(String::from),
        }]
    }

    fn stream_event(&mut self, v: &Value) -> Vec<AgentEvent> {
        let ev = &v["event"];
        match str_at(ev, "type") {
            Some("message_start") => {
                self.current_message = str_at(&ev["message"], "id").map(String::from);
                Vec::new()
            }
            Some("content_block_delta") => {
                let delta = &ev["delta"];
                let event = match str_at(delta, "type") {
                    Some("text_delta") => AgentEvent::AssistantText {
                        text: str_at(delta, "text").unwrap_or_default().into(),
                        delta: true,
                        parent_tool_use_id: str_at(v, "parent_tool_use_id").map(String::from),
                    },
                    Some("thinking_delta") => AgentEvent::Thinking {
                        text: str_at(delta, "thinking").unwrap_or_default().into(),
                        delta: true,
                    },
                    _ => return Vec::new(),
                };
                if let Some(id) = &self.current_message {
                    self.streamed.insert(id.clone());
                }
                vec![event]
            }
            _ => Vec::new(),
        }
    }

    fn assistant(&mut self, v: &Value) -> Vec<AgentEvent> {
        let msg = &v["message"];
        let streamed = str_at(msg, "id").is_some_and(|id| self.streamed.contains(id));
        let parent = str_at(v, "parent_tool_use_id").map(String::from);
        let Some(blocks) = msg["content"].as_array() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for block in blocks {
            match str_at(block, "type") {
                Some("text") if !streamed => out.push(AgentEvent::AssistantText {
                    text: str_at(block, "text").unwrap_or_default().into(),
                    delta: false,
                    parent_tool_use_id: parent.clone(),
                }),
                Some("thinking") if !streamed => out.push(AgentEvent::Thinking {
                    text: str_at(block, "thinking").unwrap_or_default().into(),
                    delta: false,
                }),
                Some("tool_use") => out.push(AgentEvent::ToolUse {
                    id: str_at(block, "id").unwrap_or_default().into(),
                    name: str_at(block, "name").unwrap_or_default().into(),
                    input: block["input"].clone(),
                    parent_tool_use_id: parent.clone(),
                }),
                _ => {}
            }
        }
        out
    }
}

fn tool_results(v: &Value) -> Vec<AgentEvent> {
    let Some(blocks) = v["message"]["content"].as_array() else {
        return Vec::new();
    };
    blocks
        .iter()
        .filter(|b| str_at(b, "type") == Some("tool_result"))
        .map(|b| AgentEvent::ToolResult {
            tool_use_id: str_at(b, "tool_use_id").unwrap_or_default().into(),
            content: content_text(&b["content"]),
            is_error: b["is_error"].as_bool().unwrap_or(false),
        })
        .collect()
}

fn result(v: &Value) -> AgentEvent {
    let is_error = v["is_error"].as_bool().unwrap_or(false)
        || str_at(v, "subtype").is_some_and(|s| s.starts_with("error"));
    let result = str_at(v, "result").map(String::from).or_else(|| {
        is_error.then(|| {
            str_at(v, "subtype")
                .unwrap_or("error")
                .replace('_', " ")
                .to_string()
        })
    });
    AgentEvent::TurnFinished {
        result,
        is_error,
        duration_ms: v["duration_ms"].as_u64(),
        num_turns: v["num_turns"].as_u64(),
        cost_usd: v["total_cost_usd"].as_f64(),
        usage: v.get("usage").map(usage),
    }
}

fn permission_request(v: &Value) -> Vec<AgentEvent> {
    let req = &v["request"];
    if str_at(req, "subtype") != Some("can_use_tool") {
        return Vec::new();
    }
    vec![AgentEvent::PermissionRequest {
        request_id: str_at(v, "request_id").unwrap_or_default().into(),
        tool_name: str_at(req, "tool_name").unwrap_or_default().into(),
        input: req["input"].clone(),
        tool_use_id: str_at(req, "tool_use_id").map(String::from),
    }]
}

fn usage(u: &Value) -> Usage {
    Usage {
        input_tokens: u["input_tokens"].as_u64().unwrap_or(0),
        output_tokens: u["output_tokens"].as_u64().unwrap_or(0),
        cache_read_tokens: u["cache_read_input_tokens"].as_u64().unwrap_or(0),
        cache_write_tokens: u["cache_creation_input_tokens"].as_u64().unwrap_or(0),
    }
}

/// Tool result content is either a string or a list of `{type: "text", text}` blocks.
fn content_text(c: &Value) -> String {
    match c {
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .filter_map(|i| str_at(i, "text"))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn str_at<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::PermissionMode;

    #[test]
    fn builds_command_line() {
        let opts = StartOptions {
            model: Some("opus".into()),
            permission_mode: PermissionMode::AcceptEdits,
            resume: Some("abc".into()),
            allowed_tools: vec!["Read".into()],
            ..Default::default()
        };
        let spec = ClaudeCode
            .command(Path::new("/usr/bin/claude"), &opts)
            .unwrap();
        assert_eq!(
            spec.args.join(" "),
            "-p --output-format stream-json --input-format stream-json --verbose \
             --include-partial-messages --model opus --permission-mode acceptEdits \
             --permission-prompt-tool stdio --resume abc --allowedTools Read"
        );
    }

    #[test]
    fn encodes_permission_responses() {
        let input = json!({"command": "ls"});
        let allow = PermissionDecision::Allow {
            updated_input: None,
            scope: Default::default(),
        };
        let v: Value = serde_json::from_str(
            &ClaudeCode
                .encode_permission_response("r1", &input, &allow)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(v["type"], "control_response");
        assert_eq!(v["response"]["subtype"], "success");
        assert_eq!(v["response"]["request_id"], "r1");
        assert_eq!(v["response"]["response"]["behavior"], "allow");
        assert_eq!(v["response"]["response"]["updatedInput"], input);

        let deny = PermissionDecision::Deny {
            message: Some("no".into()),
        };
        let v: Value = serde_json::from_str(
            &ClaudeCode
                .encode_permission_response("r2", &input, &deny)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            v["response"]["response"],
            json!({"behavior": "deny", "message": "no"})
        );
    }

    #[test]
    fn parses_permission_request() {
        let mut p = ClaudeParser::default();
        let ev = p.parse_line(
            r#"{"type":"control_request","request_id":"r9","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"ls"},"tool_use_id":"toolu_1"}}"#,
        );
        assert_eq!(
            ev,
            vec![AgentEvent::PermissionRequest {
                request_id: "r9".into(),
                tool_name: "Bash".into(),
                input: json!({"command": "ls"}),
                tool_use_id: Some("toolu_1".into()),
            }]
        );
    }

    #[test]
    fn default_model_and_mode_add_no_flags() {
        let opts = StartOptions {
            model: Some("default".into()),
            partial_messages: Some(false),
            ..Default::default()
        };
        let spec = ClaudeCode.command(Path::new("claude"), &opts).unwrap();
        assert!(!spec
            .args
            .iter()
            .any(|a| a == "--model" || a == "--permission-mode"));
        assert!(!spec.args.iter().any(|a| a == "--include-partial-messages"));
    }

    #[test]
    fn encodes_user_message() {
        let line = ClaudeCode.encode_user_message("hi \"there\"").unwrap();
        let v: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(v["type"], "user");
        assert_eq!(v["message"]["content"][0]["text"], "hi \"there\"");
    }

    #[test]
    fn ignores_garbage() {
        let mut p = ClaudeParser::default();
        assert!(p.parse_line("not json").is_empty());
        assert!(p.parse_line("").is_empty());
        assert!(p.parse_line(r#"{"type":"keep_alive"}"#).is_empty());
    }

    #[test]
    fn tool_result_array_content() {
        let mut p = ClaudeParser::default();
        let ev = p.parse_line(
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"a"},{"type":"text","text":"b"}],"is_error":true}]}}"#,
        );
        assert_eq!(
            ev,
            vec![AgentEvent::ToolResult {
                tool_use_id: "t1".into(),
                content: "a\nb".into(),
                is_error: true
            }]
        );
    }
}
