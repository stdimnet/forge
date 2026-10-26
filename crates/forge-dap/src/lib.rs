//! Forge's shared debugging layer: a Debug Adapter Protocol (DAP) client used by every
//! language pack.
//!
//! - [`client::DapClient`] speaks the protocol (Content-Length framing, requests,
//!   responses, events and reverse requests) over any [`transport::Transport`]: an adapter
//!   process on stdio, a TCP connection to an adapter that listens ("Attach to DAP
//!   server"), or a TCP server the adapter connects back to.
//! - [`adapter::DebugAdapterDescriptor`] describes how to start an adapter and build its
//!   `launch`/`attach` arguments; packs implement it in Rust or declare adapters in their
//!   manifest (`[[contributions.debugAdapters]]`, [`adapter::ManifestAdapter`]).
//! - [`DapManager`] starts sessions (launch, attach to a process, attach to a DAP server,
//!   core dumps) and [`DapSession`] maps them onto Forge's debugger model (`forge_debug`):
//!   the Debug tool window drives Xdebug and DAP sessions alike. Breakpoints (line with
//!   conditions, hit counts and logpoints, function, exception filters, data breakpoints)
//!   come from the project's shared store.
//! - Native views: memory, disassembly, registers and modules ([`views`]).
//! - [`processes`] lists local processes for "Attach to Process…".
//! - [`fake`] is a simulated adapter (a small C program) for tests and demos, also
//!   available as the `forge-dap-fake` binary.

pub mod adapter;
pub mod client;
pub mod codec;
pub mod error;
pub mod fake;
pub mod manager;
pub mod processes;
pub mod protocol;
pub mod session;
pub mod transport;
pub mod views;

pub use adapter::{
    AdapterInfo, AdapterRegistry, DebugAdapterDescriptor, LaunchMode, LaunchRequest,
    ManifestAdapter, ResolvedLaunch,
};
pub use error::{DapError, DapResult};
pub use manager::{BreakpointHost, DapConfig, DapManager};
pub use processes::{list_processes, ProcessInfo};
pub use session::{DapSession, LoadedSource};
pub use transport::{AdapterCommand, Transport};
pub use views::{Instruction, MemoryBlock, MemoryRow};
