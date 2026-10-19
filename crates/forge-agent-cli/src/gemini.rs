//! Gemini CLI adapter.
//!
//! Gemini CLI has a `--output-format stream-json` mode (JSONL with `init`, `message`,
//! `tool_use`, `tool_result`, `error` and `result` events), which this module already
//! parses. It has no streaming *input* mode though: every turn is a separate
//! `gemini -p <prompt>` process. Forge's session runner keeps one process per
//! conversation, so chat sessions report "not yet supported" until per-turn processes are
//! implemented.

use std::path::Path;

use forge_core::{ForgeError, Result};
use serde_json::Value;

use crate::cli::{AgentCli, CommandSpec, EventParser, StartOptions};
use crate::event::{AgentEvent, Usage};

pub struct GeminiCli;

const UNSUPPORTED: &str = "Gemini CLI chat is not yet supported in Forge; use Claude Code for now";

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

    fn models(&self) -> Vec<String> {
        ["gemini-2.5-pro", "gemini-2.5-flash"]
            .map(String::from)
            .to_vec()
    }

    fn supported(&self) -> bool {
        false
    }

    fn command(&self, _executable: &Path, _opts: &StartOptions) -> Result<CommandSpec> {
        Err(ForgeError::Other(UNSUPPORTED.into()))
    }

    fn encode_user_message(&self, _text: &str) -> Result<String> {
        Err(ForgeError::Other(UNSUPPORTED.into()))
    }

    fn parser(&self, _opts: &StartOptions) -> Box<dyn EventParser> {
        Box::new(GeminiParser)
    }
}

/// Parses Gemini CLI `--output-format stream-json` lines.
pub struct GeminiParser;

impl EventParser for GeminiParser {
    fn parse_line(&mut self, line: &str) -> Vec<AgentEvent> {
        let Ok(v) = serde_json::from_str::<Value>(line.trim()) else {
            return Vec::new();
        };
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(String::from);
        let event = match s("type").as_deref() {
            Some("init") => AgentEvent::SessionStarted {
                session_id: s("session_id").unwrap_or_default(),
                model: s("model"),
                tools: Vec::new(),
                cwd: None,
                permission_mode: None,
            },
            Some("message") if s("role").as_deref() == Some("assistant") => {
                AgentEvent::AssistantText {
                    text: s("content").unwrap_or_default(),
                    delta: v["delta"].as_bool().unwrap_or(false),
                    parent_tool_use_id: None,
                }
            }
            Some("tool_use") => AgentEvent::ToolUse {
                id: s("tool_id").unwrap_or_default(),
                name: s("tool_name").unwrap_or_default(),
                input: v["parameters"].clone(),
                parent_tool_use_id: None,
            },
            Some("tool_result") => {
                let is_error = s("status").as_deref() == Some("error");
                let content = s("output")
                    .or_else(|| v["error"]["message"].as_str().map(String::from))
                    .unwrap_or_default();
                AgentEvent::ToolResult {
                    tool_use_id: s("tool_id").unwrap_or_default(),
                    content,
                    is_error,
                }
            }
            Some("error") => AgentEvent::Error {
                message: s("message").unwrap_or_else(|| "unknown error".into()),
            },
            Some("result") => {
                let stats = &v["stats"];
                AgentEvent::TurnFinished {
                    result: None,
                    is_error: s("status").as_deref() != Some("success"),
                    duration_ms: stats["duration_ms"].as_u64(),
                    num_turns: None,
                    cost_usd: None,
                    usage: Some(Usage {
                        input_tokens: stats["input_tokens"].as_u64().unwrap_or(0),
                        output_tokens: stats["output_tokens"].as_u64().unwrap_or(0),
                        ..Default::default()
                    }),
                }
            }
            _ => return Vec::new(),
        };
        vec![event]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_are_not_supported_yet() {
        let err = GeminiCli
            .command(Path::new("gemini"), &StartOptions::default())
            .unwrap_err();
        assert!(err.to_string().contains("not yet supported"));
        assert!(!GeminiCli.info(None).supported);
    }
}
