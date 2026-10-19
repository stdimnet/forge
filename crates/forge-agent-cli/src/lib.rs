//! Drive AI coding agent CLIs (Claude Code, Gemini CLI, ...) headless and expose their
//! output as a normalized [`AgentEvent`] stream for Forge's chat UI.
//!
//! Forge never ships credentials: each adapter runs the user's own installed CLI with its
//! own login or API key.

pub mod claude;
pub mod cli;
pub mod event;
pub mod gemini;
pub mod prompt;
pub mod session;

use std::sync::Arc;

pub use claude::ClaudeCode;
pub use cli::{AgentCli, AgentInfo, CommandSpec, EventParser, PermissionMode, StartOptions};
pub use event::{AgentEvent, Usage};
pub use gemini::GeminiCli;
pub use prompt::{compose_prompt, MessageContext, Selection};
pub use session::{AgentSession, EventSink};

/// All built-in adapters, in picker order.
pub fn builtin_agents() -> Vec<Arc<dyn AgentCli>> {
    vec![Arc::new(ClaudeCode), Arc::new(GeminiCli)]
}

/// Look up a built-in adapter by id.
pub fn agent_by_id(id: &str) -> Option<Arc<dyn AgentCli>> {
    builtin_agents().into_iter().find(|a| a.id() == id)
}
