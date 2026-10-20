//! Remote development over SSH, like JetBrains Gateway: Forge connects to a host, installs
//! its headless agent (`crates/forge-remote-agent`) there and opens the project through a
//! [`forge_backend::RemoteBackend`], so indexing, Intelephense, git, terminals and runs all
//! happen next to the files.
//!
//! Pure modules (also built for WebAssembly without the `client` feature):
//! - [`ssh_config`]: `~/.ssh/config` host blocks.
//! - [`known_hosts`]: host key verification and fingerprints.
//! - [`deploy`]: probing a host and planning the agent upload.
//! - [`config`]: saved SSH configurations.
//!
//! With `client` (the default):
//! - [`session`]: SSH connections with russh (password, key file, ssh-agent; known_hosts
//!   check with a fingerprint confirmation).
//! - [`agent`]: installing and starting the agent.
//! - [`connection`]: a live connection to an agent with reconnects and latency.
//! - [`store`]: saved configurations on disk and their secrets in the keychain.

pub mod config;
pub mod deploy;
pub mod known_hosts;
pub mod ssh_config;

#[cfg(feature = "client")]
pub mod agent;
#[cfg(feature = "client")]
pub mod connection;
#[cfg(feature = "client")]
pub mod session;
#[cfg(feature = "client")]
pub mod store;

pub use config::{AuthMethod, SshConfiguration, SshConfigurations};
pub use known_hosts::HostKeyStatus;
