//! The chat conversation as the UI renders it: the [`AgentEvent`] stream folded into turns
//! and blocks (text with its Markdown AST, thinking, tool cards with diffs, permission
//! requests, errors), plus the derived texts the cards show.
//!
//! The backend keeps a [`Chat`] per agent session and emits a [`ChatUpdate`] (the changed
//! blocks of the current turn) after every event; the UI only applies those.

use std::collections::BTreeSet;

use forge_markdown::Flavor;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::diff::{edit_diff, EditDiff};
use crate::event::{AgentEvent, Usage};
use crate::prompt::MessageContext;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolStatus {
    Running,
    Done,
    Error,
}

/// A tool call of a sub-agent (e.g. inside a `Task`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubTool {
    pub id: String,
    pub name: String,
    pub input: Value,
    pub status: ToolStatus,
    /// What it works on: a workspace-relative path, pattern or command.
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolBlock {
    pub id: String,
    pub name: String,
    pub input: Value,
    pub status: ToolStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    pub children: Vec<SubTool>,
    /// Workspace-relative path the tool works on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The change of an edit tool.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<EditDiff>,
    /// Non-empty lines of the result.
    pub result_lines: usize,
    /// An older todo list (a newer `TodoWrite` exists in the turn).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub superseded: bool,
}

/// Pending: waiting for the user. Expired: the turn ended before an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PermissionStatus {
    Pending,
    Allowed,
    AllowedForSession,
    Denied,
    Expired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionBlock {
    pub request_id: String,
    pub tool_name: String,
    pub input: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    pub status: PermissionStatus,
    /// Headline, e.g. "run this command" / "edit Order.php".
    pub action: String,
    pub subject: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<EditDiff>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Block {
    Text {
        text: String,
        streaming: bool,
        markdown: Vec<forge_markdown::Block>,
    },
    Thinking {
        text: String,
        streaming: bool,
        started_at: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
        /// "Thinking…", "Thought for 3s".
        label: String,
    },
    Tool(ToolBlock),
    Error {
        message: String,
    },
    Permission(PermissionBlock),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TurnStatus {
    Running,
    Done,
    Error,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnSummary {
    pub duration_ms: Option<u64>,
    pub num_turns: Option<u64>,
    pub cost_usd: Option<f64>,
    pub usage: Option<Usage>,
    /// "25s", "4.2s".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_text: Option<String>,
    /// "48k tokens".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens_text: Option<String>,
    /// "$0.012".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Turn {
    /// Chosen by the UI when it sends the message.
    pub id: u64,
    pub prompt: String,
    pub context: MessageContext,
    pub blocks: Vec<Block>,
    pub status: TurnStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<TurnSummary>,
}

/// What changed, for the UI to apply to its copy of the conversation. Serialized with
/// `"type": "chat"` so it travels on the session's event channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename = "chat", rename_all = "camelCase")]
pub struct ChatUpdate {
    pub conversation_id: Option<String>,
    pub model: Option<String>,
    pub cwd: Option<String>,
    /// The current turn (absent before the first message).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<TurnUpdate>,
    /// Workspace-relative files changed by this event (reload them).
    pub changed_files: Vec<String>,
    /// The turn ended (finished, interrupted or the process exited).
    pub turn_ended: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnUpdate {
    pub id: u64,
    pub status: TurnStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<TurnSummary>,
    /// Changed blocks with their index (appended when the index is the current length).
    pub blocks: Vec<(usize, Block)>,
    /// Files edited in the turn, in order of first edit.
    pub changed_files: Vec<String>,
    /// Waiting for the user to answer a permission request.
    pub awaiting_permission: bool,
    /// Running and neither streaming text nor waiting for the user ("Working…").
    pub working: bool,
}

/// Tools whose successful result means a file in the workspace changed.
pub const EDIT_TOOLS: &[&str] = &[
    "Edit",
    "MultiEdit",
    "Write",
    "NotebookEdit",
    "replace",
    "write_file",
];

fn is_edit(name: &str) -> bool {
    EDIT_TOOLS.contains(&name)
}

/// Path a tool operates on, if any.
pub fn tool_path(input: &Value) -> Option<&str> {
    ["file_path", "notebook_path", "absolute_path", "path"]
        .iter()
        .find_map(|k| input.get(k))
        .and_then(Value::as_str)
}

/// Strip the agent's working directory so paths are workspace-relative.
pub fn relative_path(path: &str, cwd: Option<&str>) -> String {
    if let Some(cwd) = cwd {
        let root = cwd.trim_end_matches(['/', '\\']);
        if path == root {
            return String::new();
        }
        for sep in ['/', '\\'] {
            if let Some(rest) = path.strip_prefix(root).and_then(|r| r.strip_prefix(sep)) {
                return rest.replace('\\', "/");
            }
        }
    }
    path.to_string()
}

pub fn format_tokens(n: u64) -> String {
    if n >= 1000 {
        let k = n as f64 / 1000.0;
        if n >= 10_000 {
            format!("{}k", k.round())
        } else {
            format!("{k:.1}k")
        }
    } else {
        n.to_string()
    }
}

pub fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        return format!("{ms}ms");
    }
    let s = ms as f64 / 1000.0;
    if s < 60.0 {
        return if s < 10.0 {
            format!("{s:.1}s")
        } else {
            format!("{}s", s.round())
        };
    }
    format!("{}m {}s", (s / 60.0).floor(), (s % 60.0).round())
}

fn str_of(input: &Value, key: &str) -> String {
    input
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Headline and short subject of a permission request.
pub fn permission_summary(tool: &str, input: &Value, cwd: Option<&str>) -> (String, String) {
    let file = tool_path(input)
        .map(|p| {
            let rel = relative_path(p, cwd);
            rel.rsplit('/')
                .next()
                .filter(|n| !n.is_empty())
                .unwrap_or(p)
                .to_string()
        })
        .unwrap_or_default();
    match tool {
        "Bash" => ("run this command".into(), str_of(input, "command")),
        "Write" | "write_file" => (format!("create {file}"), file),
        t if is_edit(t) => (format!("edit {file}"), file),
        "WebFetch" => ("fetch a web page".into(), str_of(input, "url")),
        t => (
            format!("use {t}"),
            if file.is_empty() { t.to_string() } else { file },
        ),
    }
}

fn thinking_label(streaming: bool, duration_ms: Option<u64>) -> String {
    if streaming {
        "Thinking…".into()
    } else {
        match duration_ms {
            Some(d) if d > 0 => format!("Thought for {}s", ((d as f64) / 1000.0).round().max(1.0)),
            _ => "Thought".into(),
        }
    }
}

/// One agent session's view of the conversation.
#[derive(Debug, Default)]
pub struct Chat {
    conversation_id: Option<String>,
    model: Option<String>,
    cwd: Option<String>,
    turn: Option<Turn>,
    dirty: BTreeSet<usize>,
}

impl Chat {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn turn(&self) -> Option<&Turn> {
        self.turn.as_ref()
    }

    /// The user sent a message.
    pub fn begin_turn(&mut self, id: u64, prompt: &str, context: MessageContext) -> ChatUpdate {
        self.turn = Some(Turn {
            id,
            prompt: prompt.to_string(),
            context,
            blocks: Vec::new(),
            status: TurnStatus::Running,
            summary: None,
        });
        self.dirty.clear();
        self.update(Vec::new(), false)
    }

    /// The user answered a permission request.
    pub fn set_permission(
        &mut self,
        request_id: &str,
        status: PermissionStatus,
    ) -> Option<ChatUpdate> {
        let turn = self.turn.as_mut()?;
        let idx = turn.blocks.iter().position(
            |b| matches!(b, Block::Permission(p) if p.request_id == request_id && p.status == PermissionStatus::Pending),
        )?;
        if let Block::Permission(p) = &mut turn.blocks[idx] {
            p.status = status;
        }
        self.dirty.insert(idx);
        Some(self.update(Vec::new(), false))
    }

    fn push(&mut self, block: Block) {
        let turn = self.turn.as_mut().expect("turn");
        turn.blocks.push(block);
        self.dirty.insert(turn.blocks.len() - 1);
    }

    /// End the streaming of the last block.
    fn settle(&mut self, now: u64) {
        let Some(turn) = self.turn.as_mut() else {
            return;
        };
        let idx = turn.blocks.len().wrapping_sub(1);
        match turn.blocks.last_mut() {
            Some(Block::Text { streaming, .. }) if *streaming => {
                *streaming = false;
                self.dirty.insert(idx);
            }
            Some(Block::Thinking {
                streaming,
                started_at,
                duration_ms,
                ..
            }) if *streaming => {
                *streaming = false;
                *duration_ms = Some(now.saturating_sub(*started_at));
                self.dirty.insert(idx);
            }
            _ => {}
        }
    }

    fn tool_index(&self, id: &str) -> Option<usize> {
        self.turn
            .as_ref()?
            .blocks
            .iter()
            .position(|b| matches!(b, Block::Tool(t) if t.id == id))
    }

    fn finish_tools(&mut self, status: ToolStatus) {
        let Some(turn) = self.turn.as_mut() else {
            return;
        };
        for (i, b) in turn.blocks.iter_mut().enumerate() {
            match b {
                Block::Tool(t) if t.status == ToolStatus::Running => {
                    t.status = status;
                    self.dirty.insert(i);
                }
                // Unanswered requests can no longer be answered once the turn is over.
                Block::Permission(p) if p.status == PermissionStatus::Pending => {
                    p.status = PermissionStatus::Expired;
                    self.dirty.insert(i);
                }
                _ => {}
            }
        }
    }

    fn has_error(&self) -> bool {
        self.turn
            .as_ref()
            .is_some_and(|t| t.blocks.iter().any(|b| matches!(b, Block::Error { .. })))
    }

    /// Apply one event (`now` in milliseconds) and return what changed.
    pub fn apply(&mut self, ev: &AgentEvent, now: u64) -> ChatUpdate {
        if let AgentEvent::SessionStarted {
            session_id,
            model,
            cwd,
            ..
        } = ev
        {
            if !session_id.is_empty() {
                self.conversation_id = Some(session_id.clone());
            }
            self.model = model.clone();
            if cwd.is_some() {
                self.cwd = cwd.clone();
            }
            return self.update(Vec::new(), false);
        }
        let Some(turn) = self.turn.as_ref() else {
            return self.update(Vec::new(), false);
        };
        let running = turn.status == TurnStatus::Running;
        let mut changed = Vec::new();
        let mut ended = false;
        match ev {
            AgentEvent::SessionStarted { .. } | AgentEvent::Usage { .. } => {}
            AgentEvent::AssistantText {
                text,
                delta,
                parent_tool_use_id,
            } => {
                // Sub-agent prose stays inside its tool card.
                if parent_tool_use_id.is_none() {
                    let turn = self.turn.as_mut().expect("turn");
                    let idx = turn.blocks.len().wrapping_sub(1);
                    match turn.blocks.last_mut() {
                        Some(Block::Text {
                            text: t, streaming, ..
                        }) if *delta && *streaming => {
                            t.push_str(text);
                            self.dirty.insert(idx);
                        }
                        _ => {
                            self.settle(now);
                            self.push(Block::Text {
                                text: text.clone(),
                                streaming: *delta,
                                markdown: Vec::new(),
                            });
                        }
                    }
                }
            }
            AgentEvent::Thinking { text, delta } => {
                let turn = self.turn.as_mut().expect("turn");
                let idx = turn.blocks.len().wrapping_sub(1);
                match turn.blocks.last_mut() {
                    Some(Block::Thinking {
                        text: t, streaming, ..
                    }) if *delta && *streaming => {
                        t.push_str(text);
                        self.dirty.insert(idx);
                    }
                    _ => {
                        self.settle(now);
                        self.push(Block::Thinking {
                            text: text.clone(),
                            streaming: *delta,
                            started_at: now,
                            duration_ms: if *delta { None } else { Some(0) },
                            label: String::new(),
                        });
                    }
                }
            }
            AgentEvent::ToolUse {
                id,
                name,
                input,
                parent_tool_use_id,
            } => {
                let input = if input.is_null() {
                    Value::Object(Default::default())
                } else {
                    input.clone()
                };
                let parent = parent_tool_use_id
                    .as_deref()
                    .and_then(|p| self.tool_index(p));
                if let Some(pi) = parent {
                    if let Some(Block::Tool(p)) =
                        self.turn.as_mut().expect("turn").blocks.get_mut(pi)
                    {
                        p.children.push(SubTool {
                            id: id.clone(),
                            name: name.clone(),
                            input,
                            status: ToolStatus::Running,
                            subject: String::new(),
                        });
                    }
                    self.dirty.insert(pi);
                } else {
                    self.settle(now);
                    self.push(Block::Tool(ToolBlock {
                        id: id.clone(),
                        name: name.clone(),
                        input,
                        status: ToolStatus::Running,
                        result: None,
                        children: Vec::new(),
                        path: None,
                        diff: None,
                        result_lines: 0,
                        superseded: false,
                    }));
                    if name == "TodoWrite" {
                        // Older todo lists collapse once a newer one exists.
                        let turn = self.turn.as_mut().expect("turn");
                        let last = turn.blocks.len() - 1;
                        for (i, b) in turn.blocks.iter_mut().enumerate().take(last) {
                            if let Block::Tool(t) = b {
                                if t.name == "TodoWrite" && !t.superseded {
                                    t.superseded = true;
                                    self.dirty.insert(i);
                                }
                            }
                        }
                    }
                }
            }
            AgentEvent::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                let status = if *is_error {
                    ToolStatus::Error
                } else {
                    ToolStatus::Done
                };
                let cwd = self.cwd.clone();
                match self.tool_index(tool_use_id) {
                    Some(i) => {
                        let turn = self.turn.as_mut().expect("turn");
                        if let Block::Tool(t) = &mut turn.blocks[i] {
                            t.status = status;
                            t.result = Some(content.clone());
                            if !is_error && is_edit(&t.name) {
                                if let Some(p) = tool_path(&t.input) {
                                    changed.push(relative_path(p, cwd.as_deref()));
                                }
                            }
                        }
                        self.dirty.insert(i);
                    }
                    None => {
                        let turn = self.turn.as_mut().expect("turn");
                        for (i, b) in turn.blocks.iter_mut().enumerate() {
                            if let Block::Tool(t) = b {
                                if let Some(c) =
                                    t.children.iter_mut().find(|c| c.id == *tool_use_id)
                                {
                                    c.status = status;
                                    self.dirty.insert(i);
                                }
                            }
                        }
                    }
                }
            }
            AgentEvent::PermissionRequest {
                request_id,
                tool_name,
                input,
                tool_use_id,
            } => {
                self.settle(now);
                self.push(Block::Permission(PermissionBlock {
                    request_id: request_id.clone(),
                    tool_name: tool_name.clone(),
                    input: if input.is_null() {
                        Value::Object(Default::default())
                    } else {
                        input.clone()
                    },
                    tool_use_id: tool_use_id.clone(),
                    status: PermissionStatus::Pending,
                    action: String::new(),
                    subject: String::new(),
                    path: None,
                    diff: None,
                }));
            }
            AgentEvent::Error { message } => {
                self.settle(now);
                self.push(Block::Error {
                    message: message.clone(),
                });
            }
            AgentEvent::TurnFinished {
                result,
                is_error,
                duration_ms,
                num_turns,
                cost_usd,
                usage,
            } => {
                self.settle(now);
                self.finish_tools(if *is_error {
                    ToolStatus::Error
                } else {
                    ToolStatus::Done
                });
                let had_error = self.has_error();
                if *is_error && !had_error {
                    if let Some(r) = result {
                        self.push(Block::Error { message: r.clone() });
                    }
                }
                let turn = self.turn.as_mut().expect("turn");
                turn.status = if *is_error {
                    TurnStatus::Error
                } else {
                    TurnStatus::Done
                };
                let tokens = usage.as_ref().map_or(0, |u| {
                    u.input_tokens + u.output_tokens + u.cache_read_tokens + u.cache_write_tokens
                });
                turn.summary = Some(TurnSummary {
                    duration_ms: *duration_ms,
                    num_turns: *num_turns,
                    cost_usd: *cost_usd,
                    usage: usage.clone(),
                    duration_text: duration_ms.map(format_duration),
                    tokens_text: (tokens > 0).then(|| format!("{} tokens", format_tokens(tokens))),
                    cost_text: cost_usd.map(|c| {
                        if c < 0.1 {
                            format!("${c:.3}")
                        } else {
                            format!("${c:.2}")
                        }
                    }),
                });
                ended = true;
            }
            AgentEvent::Interrupted => {
                self.settle(now);
                if running {
                    self.finish_tools(ToolStatus::Error);
                    self.turn.as_mut().expect("turn").status = TurnStatus::Interrupted;
                }
                ended = true;
            }
            AgentEvent::Exited { code } => {
                ended = true;
                if running {
                    self.settle(now);
                    self.finish_tools(ToolStatus::Error);
                    self.turn.as_mut().expect("turn").status = TurnStatus::Error;
                    if !self.has_error() {
                        let code = code.map_or("unknown".to_string(), |c| c.to_string());
                        self.push(Block::Error {
                            message: format!("The agent exited unexpectedly (code {code})."),
                        });
                    }
                }
            }
        }
        self.update(changed, ended)
    }

    /// Fill the derived render fields of a block.
    fn render(&self, block: &Block) -> Block {
        let cwd = self.cwd.as_deref();
        let mut b = block.clone();
        match &mut b {
            Block::Text { text, markdown, .. } => {
                *markdown = forge_markdown::parse(text, Flavor::Chat)
            }
            Block::Thinking {
                streaming,
                duration_ms,
                label,
                ..
            } => *label = thinking_label(*streaming, *duration_ms),
            Block::Tool(t) => {
                t.path = tool_path(&t.input).map(|p| relative_path(p, cwd));
                if is_edit(&t.name) {
                    t.diff = Some(edit_diff(&t.name, &t.input));
                }
                t.result_lines = t
                    .result
                    .as_deref()
                    .map_or(0, |r| r.lines().filter(|l| !l.trim().is_empty()).count());
                for c in &mut t.children {
                    c.subject = match tool_path(&c.input) {
                        Some(p) => relative_path(p, cwd),
                        None => {
                            let v = c.input.get("pattern").or_else(|| c.input.get("command"));
                            relative_path(v.and_then(Value::as_str).unwrap_or_default(), cwd)
                        }
                    };
                }
            }
            Block::Permission(p) => {
                let (action, subject) = permission_summary(&p.tool_name, &p.input, cwd);
                p.action = action;
                p.subject = subject;
                p.path = tool_path(&p.input).map(|x| relative_path(x, cwd));
                if is_edit(&p.tool_name) {
                    p.diff = Some(edit_diff(&p.tool_name, &p.input));
                }
            }
            Block::Error { .. } => {}
        }
        b
    }

    /// Files edited during the turn, in order of first edit.
    fn changed_files_of(&self, turn: &Turn) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for b in &turn.blocks {
            if let Block::Tool(t) = b {
                if t.status == ToolStatus::Done && is_edit(&t.name) {
                    if let Some(p) = tool_path(&t.input) {
                        let rel = relative_path(p, self.cwd.as_deref());
                        if !out.contains(&rel) {
                            out.push(rel);
                        }
                    }
                }
            }
        }
        out
    }

    fn update(&mut self, changed_files: Vec<String>, turn_ended: bool) -> ChatUpdate {
        let dirty = std::mem::take(&mut self.dirty);
        let turn = self.turn.as_ref().map(|t| {
            let pending = t.blocks.iter().any(
                |b| matches!(b, Block::Permission(p) if p.status == PermissionStatus::Pending),
            );
            let streaming = t.blocks.iter().any(|b| {
                matches!(
                    b,
                    Block::Text {
                        streaming: true,
                        ..
                    }
                )
            });
            TurnUpdate {
                id: t.id,
                status: t.status,
                summary: t.summary.clone(),
                blocks: dirty
                    .iter()
                    .filter_map(|&i| t.blocks.get(i).map(|b| (i, self.render(b))))
                    .collect(),
                changed_files: self.changed_files_of(t),
                awaiting_permission: pending,
                working: t.status == TurnStatus::Running && !pending && !streaming,
            }
        });
        ChatUpdate {
            conversation_id: self.conversation_id.clone(),
            model: self.model.clone(),
            cwd: self.cwd.clone(),
            turn,
            changed_files,
            turn_ended,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const CWD: &str = "/home/dev/shop";

    fn run(events: Vec<AgentEvent>) -> (Chat, Vec<ChatUpdate>, Vec<Block>) {
        let mut chat = Chat::new();
        let mut blocks: Vec<Block> = Vec::new();
        let first = chat.begin_turn(1, "Add a scope", MessageContext::default());
        assert!(first.turn.unwrap().blocks.is_empty());
        let mut t = 0;
        let mut updates = Vec::new();
        for e in events {
            t += 1000;
            let u = chat.apply(&e, t);
            for (i, b) in u.turn.iter().flat_map(|t| t.blocks.clone()) {
                if i == blocks.len() {
                    blocks.push(b);
                } else {
                    blocks[i] = b;
                }
            }
            updates.push(u);
        }
        (chat, updates, blocks)
    }

    fn tool(id: &str, name: &str, input: Value) -> AgentEvent {
        AgentEvent::ToolUse {
            id: id.into(),
            name: name.into(),
            input,
            parent_tool_use_id: None,
        }
    }

    fn finished(is_error: bool) -> AgentEvent {
        AgentEvent::TurnFinished {
            result: None,
            is_error,
            duration_ms: Some(24_610),
            num_turns: Some(3),
            cost_usd: Some(0.01),
            usage: Some(Usage {
                input_tokens: 48_000,
                output_tokens: 210,
                ..Usage::default()
            }),
        }
    }

    #[test]
    fn permission_requests_are_pending_until_the_turn_ends() {
        let (mut chat, updates, blocks) = run(vec![
            tool("t1", "Bash", json!({ "command": "php artisan test" })),
            AgentEvent::PermissionRequest {
                request_id: "r1".into(),
                tool_name: "Bash".into(),
                input: json!({ "command": "php artisan test" }),
                tool_use_id: Some("t1".into()),
            },
        ]);
        assert!(updates[1].turn.as_ref().unwrap().awaiting_permission);
        let Block::Permission(p) = &blocks[1] else {
            panic!()
        };
        assert_eq!(
            (p.action.as_str(), p.subject.as_str()),
            ("run this command", "php artisan test")
        );
        let u = chat.apply(&finished(false), 9000);
        let turn = u.turn.unwrap();
        assert!(!turn.awaiting_permission);
        assert!(turn.blocks.iter().any(
            |(_, b)| matches!(b, Block::Permission(p) if p.status == PermissionStatus::Expired)
        ));
        assert!(chat
            .set_permission("r1", PermissionStatus::Allowed)
            .is_none());
    }

    #[test]
    fn folds_a_streamed_turn_into_blocks() {
        let edit = json!({ "file_path": format!("{CWD}/app/Order.php"), "old_string": "a", "new_string": "b" });
        let (_, updates, blocks) = run(vec![
            AgentEvent::SessionStarted {
                session_id: "s1".into(),
                model: Some("sonnet".into()),
                tools: vec![],
                cwd: Some(CWD.into()),
                permission_mode: None,
            },
            AgentEvent::Thinking {
                text: "Let me ".into(),
                delta: true,
            },
            AgentEvent::Thinking {
                text: "look.".into(),
                delta: true,
            },
            AgentEvent::AssistantText {
                text: "I'll ".into(),
                delta: true,
                parent_tool_use_id: None,
            },
            AgentEvent::AssistantText {
                text: "read **it**.".into(),
                delta: true,
                parent_tool_use_id: None,
            },
            tool(
                "t1",
                "Read",
                json!({ "file_path": format!("{CWD}/app/Order.php") }),
            ),
            AgentEvent::ToolResult {
                tool_use_id: "t1".into(),
                content: "<?php\n\nclass Order {}".into(),
                is_error: false,
            },
            tool("t2", "Edit", edit),
            AgentEvent::ToolResult {
                tool_use_id: "t2".into(),
                content: "updated".into(),
                is_error: false,
            },
            AgentEvent::AssistantText {
                text: "Done.".into(),
                delta: false,
                parent_tool_use_id: None,
            },
            finished(false),
        ]);
        assert_eq!(updates[0].conversation_id.as_deref(), Some("s1"));
        let kinds: Vec<&str> = blocks
            .iter()
            .map(|b| match b {
                Block::Thinking { .. } => "thinking",
                Block::Text { .. } => "text",
                Block::Tool(_) => "tool",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["thinking", "text", "tool", "tool", "text"]);
        let Block::Thinking {
            text,
            duration_ms,
            label,
            ..
        } = &blocks[0]
        else {
            panic!()
        };
        assert_eq!(
            (text.as_str(), *duration_ms, label.as_str()),
            ("Let me look.", Some(2000), "Thought for 2s")
        );
        let Block::Text {
            text,
            streaming,
            markdown,
        } = &blocks[1]
        else {
            panic!()
        };
        assert_eq!((text.as_str(), *streaming), ("I'll read **it**.", false));
        assert!(matches!(
            &markdown[0],
            forge_markdown::Block::Paragraph { .. }
        ));
        let Block::Tool(read) = &blocks[2] else {
            panic!()
        };
        assert_eq!(
            (read.path.as_deref(), read.result_lines),
            (Some("app/Order.php"), 2)
        );
        let Block::Tool(edit) = &blocks[3] else {
            panic!()
        };
        assert_eq!(
            edit.diff.as_ref().map(|d| (d.added, d.removed)),
            Some((1, 1))
        );
        assert_eq!(updates[8].changed_files, ["app/Order.php"]);
        let last = updates.last().unwrap();
        assert!(last.turn_ended);
        let turn = last.turn.as_ref().unwrap();
        assert_eq!(turn.status, TurnStatus::Done);
        assert_eq!(turn.changed_files, ["app/Order.php"]);
        let s = turn.summary.as_ref().unwrap();
        assert_eq!(
            (
                s.duration_text.as_deref(),
                s.tokens_text.as_deref(),
                s.cost_text.as_deref()
            ),
            (Some("25s"), Some("48k tokens"), Some("$0.010"))
        );
    }

    #[test]
    fn marks_running_tools_as_failed_when_interrupted() {
        let (chat, _, blocks) = run(vec![
            tool("t1", "Bash", json!({ "command": "sleep 10" })),
            AgentEvent::Interrupted,
            AgentEvent::Exited { code: None },
        ]);
        assert_eq!(chat.turn().unwrap().status, TurnStatus::Interrupted);
        assert!(matches!(&blocks[0], Block::Tool(t) if t.status == ToolStatus::Error));
        assert_eq!(blocks.len(), 1);
    }

    #[test]
    fn reports_a_crash_when_the_process_exits_mid_turn() {
        let (chat, _, blocks) = run(vec![
            AgentEvent::Error {
                message: "not logged in".into(),
            },
            AgentEvent::Exited { code: Some(1) },
        ]);
        assert_eq!(chat.turn().unwrap().status, TurnStatus::Error);
        assert_eq!(
            blocks,
            [Block::Error {
                message: "not logged in".into()
            }]
        );
    }

    #[test]
    fn nests_sub_agent_tool_calls_and_supersedes_todo_lists() {
        let (_, _, blocks) = run(vec![
            tool("todo1", "TodoWrite", json!({ "todos": [] })),
            tool("task", "Task", json!({ "description": "Explore" })),
            AgentEvent::ToolUse {
                id: "c1".into(),
                name: "Grep".into(),
                input: json!({ "pattern": "x" }),
                parent_tool_use_id: Some("task".into()),
            },
            AgentEvent::AssistantText {
                text: "sub-agent prose".into(),
                delta: false,
                parent_tool_use_id: Some("task".into()),
            },
            AgentEvent::ToolResult {
                tool_use_id: "c1".into(),
                content: String::new(),
                is_error: false,
            },
            tool("todo2", "TodoWrite", json!({ "todos": [] })),
        ]);
        assert_eq!(blocks.len(), 3);
        let Block::Tool(task) = &blocks[1] else {
            panic!()
        };
        assert_eq!(task.children.len(), 1);
        assert_eq!(
            (task.children[0].status, task.children[0].subject.as_str()),
            (ToolStatus::Done, "x")
        );
        assert!(matches!(&blocks[0], Block::Tool(t) if t.superseded));
        assert!(matches!(&blocks[2], Block::Tool(t) if !t.superseded));
    }

    #[test]
    fn relativizes_paths_formats_numbers_and_summarizes_permissions() {
        assert_eq!(
            relative_path(&format!("{CWD}/a/b.php"), Some(CWD)),
            "a/b.php"
        );
        assert_eq!(relative_path("C:\\p\\a\\b.php", Some("C:\\p")), "a/b.php");
        assert_eq!(relative_path("/elsewhere/x", Some(CWD)), "/elsewhere/x");
        assert_eq!(format_tokens(48210), "48k");
        assert_eq!(format_tokens(1240), "1.2k");
        assert_eq!(format_duration(24610), "25s");
        assert_eq!(format_duration(4200), "4.2s");
        let file = json!({ "file_path": format!("{CWD}/app/Order.php") });
        assert_eq!(
            permission_summary("Edit", &file, Some(CWD)).0,
            "edit Order.php"
        );
        assert_eq!(
            permission_summary("Write", &file, Some(CWD)).0,
            "create Order.php"
        );
        assert_eq!(
            permission_summary("mcp__db__query", &json!({}), Some(CWD)).0,
            "use mcp__db__query"
        );
    }
}
