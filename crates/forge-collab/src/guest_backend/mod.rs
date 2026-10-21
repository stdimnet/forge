//! The guest's project backend: a guest's Forge shows the host's project as if it were open
//! locally, the way SSH projects are shown (`forge_backend::RemoteBackend`).
//!
//! - [`calls`]: [`calls::HostCall`], the restricted requests a guest may make. It is its own
//!   protocol, not the Forge agent's: there is no raw git, process or language server start.
//! - [`policy`]: the permission check of each call (`Policy::check_call`).
//! - `host_calls` (a child module of `crate::host`, which owns the session state): the host's
//!   side: enforcement (paths resolved through symlinks, `.git/` and
//!   secret files hidden, read-only versus edit) and the answers.
//! - [`git`]: typed git queries and actions, and how a guest's git commands become them.
//! - [`lsp_host`]: the host's language servers answering guests' requests.
//! - [`guest`]: [`guest::CollabBackend`], `forge_backend::Backend` over a guest session.

pub mod calls;
pub mod git;
pub mod guest;
pub mod lsp_host;
pub mod policy;

pub use calls::{GitAction, GitQuery, HostCall, LspQuery, OpenedProject};
pub use guest::CollabBackend;
