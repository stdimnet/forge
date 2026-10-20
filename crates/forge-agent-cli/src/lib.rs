//! Drive AI coding agent CLIs (Claude Code, Gemini CLI, ...) headless and expose their
//! output as a normalized [`AgentEvent`] stream for Forge's chat UI.
//!
//! Forge never ships credentials: each adapter runs the user's own installed CLI with its
//! own login or API key.
//!
//! Every CLI is an `AgentCli` adapter: its command line, how messages reach it
//! (`Transport`), an [`EventParser`] from its stream format to [`AgentEvent`]s, permission
//! prompts and session resume. [`profile`] describes each one (models, permission modes,
//! install hint).
//!
//! [`conversation`] folds the events into the chat as the UI renders it (with [`diff`]s of
//! edits and Markdown ASTs). It, the event/prompt/profile modules and the Gemini parser are
//! pure; the adapters and sessions need the `runtime` feature (default).

#[cfg(feature = "runtime")]
pub mod claude;
#[cfg(feature = "runtime")]
pub mod cli;
#[cfg(feature = "runtime")]
pub mod host;
pub mod conversation;
pub mod diff;
pub mod event;
pub mod gemini;
pub mod profile;
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
    PermissionDecision, StartOptions, Transport,
};
pub use conversation::{Chat, ChatUpdate, PermissionStatus};
pub use event::{AgentEvent, EventParser, Usage};
#[cfg(feature = "runtime")]
pub use gemini::GeminiCli;
#[cfg(feature = "runtime")]
pub use host::{AgentHost, LocalHost, SpawnedAgent};
pub use profile::{AgentProfile, PermissionMode, PermissionModeInfo};
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

/// A fresh output parser for the agents whose parser is part of the pure build (the browser
/// mock replays recorded CLI output through it).
pub fn pure_parser(agent_id: &str) -> Option<Box<dyn EventParser>> {
    match agent_id {
        "gemini-cli" => Some(Box::new(gemini::GeminiParser)),
        _ => None,
    }
}
