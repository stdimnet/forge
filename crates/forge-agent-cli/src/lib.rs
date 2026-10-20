//! Drive AI coding agent CLIs (Claude Code, Gemini CLI, ...) headless and expose their
//! output as a normalized [`AgentEvent`] stream for Forge's chat UI.
//!
//! Forge never ships credentials: each adapter runs the user's own installed CLI with its
//! own login or API key.
//!
//! [`conversation`] folds the events into the chat as the UI renders it (with [`diff`]s of
//! edits and Markdown ASTs). It and the event/prompt modules are pure; the adapters and
//! sessions need the `runtime` feature (default).

#[cfg(feature = "runtime")]
pub mod claude;
#[cfg(feature = "runtime")]
pub mod cli;
pub mod conversation;
pub mod diff;
pub mod event;
#[cfg(feature = "runtime")]
pub mod gemini;
pub mod prompt;
#[cfg(feature = "runtime")]
pub mod session;

#[cfg(feature = "runtime")]
use std::sync::Arc;

#[cfg(feature = "runtime")]
pub use claude::ClaudeCode;
#[cfg(feature = "runtime")]
pub use cli::{
    configured_executable, session_rule, setting_key, AgentCli, AgentInfo, AllowScope, CommandSpec,
    EventParser, PermissionDecision, PermissionMode, StartOptions,
};
pub use conversation::{Chat, ChatUpdate, PermissionStatus};
pub use event::{AgentEvent, Usage};
#[cfg(feature = "runtime")]
pub use gemini::GeminiCli;
pub use prompt::{compose_prompt, MessageContext, Selection};
#[cfg(feature = "runtime")]
pub use session::{AgentSession, EventSink};

/// All built-in adapters, in picker order.
#[cfg(feature = "runtime")]
pub fn builtin_agents() -> Vec<Arc<dyn AgentCli>> {
    vec![Arc::new(ClaudeCode), Arc::new(GeminiCli)]
}

/// Look up a built-in adapter by id.
#[cfg(feature = "runtime")]
pub fn agent_by_id(id: &str) -> Option<Arc<dyn AgentCli>> {
    builtin_agents().into_iter().find(|a| a.id() == id)
}
