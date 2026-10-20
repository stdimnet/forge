---
title: forge-agent-cli
description: "Runs AI coding agent CLIs (Claude Code, Gemini CLI) headless and normalizes their output."
order: 10
tags: [crate, rust]
---

# forge-agent-cli

`crates/forge-agent-cli` — Runs AI coding agent CLIs (Claude Code, Gemini CLI) headless and normalizes their output.

## Purpose

Drive AI coding agent CLIs (Claude Code, Gemini CLI, ...) headless and expose their
output as a normalized `AgentEvent` stream for Forge's chat UI.

Forge never ships credentials: each adapter runs the user's own installed CLI with its
own login or API key.

Every CLI is an `AgentCli` adapter: its command line, how messages reach it
(`Transport`), an `EventParser` from its stream format to `AgentEvent`s, permission
prompts and session resume. `profile` describes each one (models, permission modes,
install hint).

`conversation` folds the events into the chat as the UI renders it (with `diff`s of
edits and Markdown ASTs). It, the event/prompt/profile modules and the Gemini parser are
pure; the adapters and sessions need the `runtime` feature (default).

## Modules

| Module | Summary |
| --- | --- |
| `claude.rs` | Claude Code adapter: `claude -p --input-format stream-json --output-format stream-json`. |
| `cli.rs` | The adapter trait every agent CLI implements, plus start options and detection. |
| `conversation.rs` | The chat conversation as the UI renders it: the `AgentEvent` stream folded into turns and blocks (text with its Markdown AST, thinking, tool cards with diffs, permission requests, errors), plus the derived texts the cards show. |
| `diff.rs` | Line diffs of edit tool calls (`Edit`, `MultiEdit`, `Write`) for the chat's edit cards and permission previews. |
| `event.rs` | The normalized event stream every agent adapter produces. The UI only knows these events, so adding a new CLI never touches the chat UI. |
| `gemini.rs` | Gemini CLI adapter: `gemini --output-format stream-json`, one process per turn. |
| `profile.rs` | Static descriptions of the built-in agent CLIs: models, permission modes as each CLI understands them, and how to install it. Pure, so the browser-mode mock shows the same picker entries as the desktop app. |
| `prompt.rs` | Turns a chat message plus editor context (attached files, selection) into the prompt text sent to the agent. |
| `session.rs` | Runs agent CLI processes for a conversation and streams their normalized events: one long-running process (`Transport::Persistent`) or one process per message (`Transport::PerTurn`). |

## Main public items

```rust
pub use claude::ClaudeCode;
pub use cli::{…};
pub use conversation::{Chat, ChatUpdate, PermissionStatus};
pub use event::{AgentEvent, EventParser, Usage};
pub use gemini::GeminiCli;
pub use profile::{AgentProfile, PermissionMode, PermissionModeInfo};
pub use prompt::{compose_prompt, MessageContext, Selection};
pub use session::{AgentSession, EventSink};
pub fn builtin_agents() -> Vec<Arc<dyn AgentCli>> { … }
pub fn agent_by_id(id: &str) -> Option<Arc<dyn AgentCli>> { … }
pub fn pure_parser(agent_id: &str) -> Option<Box<dyn EventParser>> { … }
```

## Used by

- IPC: `agent_list`, `agent_start`, `agent_send`, `agent_respond_permission`, `agent_interrupt`, `agent_stop` (`app/src-tauri/src/commands/agent.rs`)
- UI: `app/ui/src/features/ai-chat`
- Browser mode: built into `forge-wasm`

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md), [`forge-markdown`](forge-markdown.md)
- External: `serde`, `serde_json`, `similar`, `tokio`, `tracing`
- Features: `default` = `runtime`; `runtime` = `dep:forge-core`, `dep:tokio`, `dep:tracing`

## Tests

10 source files, about 3354 lines; 27 unit tests and 14 integration tests in `tests/` (`gemini_session.rs`, `parse_fixtures.rs`, `session.rs`).

```sh
cargo test -p forge-agent-cli
```
