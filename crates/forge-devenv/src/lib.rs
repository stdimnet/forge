//! Dev Containers and WSL projects, like PHPStorm / JetBrains Gateway: the project's
//! services run in a container or a WSL distribution through the Forge agent
//! (`crates/forge-remote-agent`), which Forge starts over the stdio of `docker exec` or
//! `wsl.exe` instead of SSH; the project then opens as a [`forge_backend::RemoteBackend`].
//!
//! Pure modules (also built for WebAssembly without the `runtime` feature):
//! - [`jsonc`]: JSON with comments and trailing commas.
//! - [`devcontainer`]: `devcontainer.json` locations, parsing and resolving.
//! - [`docker`]: Docker CLI command lines and the `docker ps` list of dev containers.
//! - [`wsl`]: `wsl.exe -l -v` (UTF-16) parsing, command lines and Windows ↔ Linux paths.
//!
//! With `runtime` (the default):
//! - [`runner`]: processes on this machine behind a trait (fakes in tests).
//! - [`agent`]: copying the agent in and starting it over a process's stdio.
//! - [`connection`]: a live agent connection that restarts the agent when it ends.
//! - [`up`]: building / starting a dev container with a build log.

pub mod devcontainer;
pub mod docker;
pub mod jsonc;
pub mod wsl;

#[cfg(feature = "runtime")]
pub mod agent;
#[cfg(feature = "runtime")]
pub mod connection;
#[cfg(feature = "runtime")]
pub mod runner;
#[cfg(feature = "runtime")]
pub mod up;
