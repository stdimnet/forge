//! The normalized event stream every agent adapter produces. The UI only knows these
//! events, so adding a new CLI never touches the chat UI.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Token usage of a turn (or of one API call when reported mid-turn).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_read_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: u64,
}

/// One normalized event of an agent session. Serialized as `{ "type": "...", ...fields }`
/// with camelCase names, which is what the UI receives on `agent://<sessionId>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AgentEvent {
    /// The CLI started (or resumed) a conversation.
    SessionStarted {
        session_id: String,
        model: Option<String>,
        tools: Vec<String>,
        cwd: Option<String>,
        permission_mode: Option<String>,
    },
    /// Assistant prose. `delta: true` appends to the text block being streamed;
    /// `delta: false` is a complete block.
    AssistantText {
        text: String,
        delta: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<String>,
    },
    /// Extended thinking, same delta semantics as `AssistantText`.
    Thinking { text: String, delta: bool },
    /// The agent invoked a tool.
    ToolUse {
        id: String,
        name: String,
        input: Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        parent_tool_use_id: Option<String>,
    },
    /// Result of a tool invocation.
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
    /// Intermediate token usage / cost report.
    Usage { usage: Usage, cost_usd: Option<f64> },
    /// The CLI asks whether a tool may run. Answer with `AgentSession::respond_permission`;
    /// the CLI waits until then.
    PermissionRequest {
        request_id: String,
        tool_name: String,
        input: Value,
        /// The tool call this request is about, when the CLI reports it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_use_id: Option<String>,
    },
    /// A turn completed (successfully or not).
    TurnFinished {
        result: Option<String>,
        is_error: bool,
        duration_ms: Option<u64>,
        num_turns: Option<u64>,
        cost_usd: Option<f64>,
        usage: Option<Usage>,
    },
    /// The running turn was interrupted by the user.
    Interrupted,
    /// Something went wrong (CLI error, unparsable output, crash).
    Error { message: String },
    /// The CLI process ended.
    Exited { code: Option<i32> },
}
