//! Forge as a Model Context Protocol server, like JetBrains' built-in MCP server: external
//! agents (Claude Code, Claude Desktop, Cursor, Gemini CLI, Codex, …) connect to the running
//! IDE and use its tools on the open project.
//!
//! - [`jsonrpc`], [`protocol`]: JSON-RPC 2.0 messages and the MCP lifecycle (version
//!   negotiation, capabilities, prompts, resource URIs).
//! - [`tools`]: the tool catalog with JSON Schemas, groups and kinds.
//! - [`policy`]: allowed groups, approvals, Safe Mode, `ai.enabled` and `.aiignore`.
//! - [`clients`]: "Copy client configuration" snippets.
//! - With the `server` feature: [`engine`] (sessions, calls, approvals, call log over an
//!   [`engine::McpHost`]), [`server`] (Streamable HTTP and HTTP+SSE on `127.0.0.1` with a
//!   bearer token), [`stdio`] (the `forge --mcp-stdio` proxy) and [`http`].

pub mod clients;
pub mod jsonrpc;
pub mod policy;
pub mod protocol;
pub mod tools;

#[cfg(feature = "server")]
pub mod engine;
#[cfg(feature = "server")]
pub mod http;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod stdio;
#[cfg(feature = "keychain")]
pub mod token;

pub use clients::{configs, ClientConfig, STDIO_FLAG};
pub use policy::{CallPolicy, Decision};
pub use tools::{catalog, ToolGroup, ToolKind, ToolSpec};

#[cfg(feature = "server")]
pub use engine::{
    Approval, ApprovalRequest, CallLogEntry, CallStatus, Engine, McpHost, ServerEvent,
    SessionInfo, ToolOutput,
};
#[cfg(feature = "server")]
pub use server::{McpServer, DEFAULT_PORT};
