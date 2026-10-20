//! Forge's generic Language Server Protocol client.
//!
//! - [`framing`]: `Content-Length` JSON-RPC framing over stdio.
//! - [`LspClient`]: one server process; request/response correlation, notifications,
//!   answers to server→client requests, document sync and typed feature requests.
//! - [`LspManager`]: routes files to servers by extension and starts them lazily; several
//!   servers may serve one file and their results are merged.
//! - [`model`]: editor-friendly result types consumed by the UI.
//! - [`launch`]: how server processes start ([`LocalLauncher`] here, or on a remote host).
//!
//! Nothing here is language specific; language packs contribute [`LspServerConfig`]s.

pub mod client;
pub mod config;
pub mod framing;
pub mod launch;
pub mod manager;
pub mod model;

pub use client::{path_to_uri, uri_to_path, LspClient, LspEvent, ServerState};
pub use config::{find_on_path, LspServerConfig};
pub use launch::{LocalLauncher, ServerIo, ServerLauncher, ServerProcess};
pub use lsp_types;
pub use manager::{LspManager, ServerStatus, COMPLETION_SERVER_KEY};
