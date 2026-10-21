//! Containers for the Services tool window: Docker, Docker Compose and Kubernetes.
//!
//! Pure parts (no I/O, built for WebAssembly too):
//! - [`docker`]: Docker Engine API JSON (containers, images, volumes, networks) into models.
//! - [`compose`]: `compose.yaml` / `docker-compose.yml` parsing, project names, command lines
//!   and the mapping of containers to services by their labels.
//! - [`kube`]: kubeconfig parsing and Kubernetes API objects (pods, deployments, services).
//! - [`logs`]: the multiplexed Docker log stream and Kubernetes log lines.
//! - [`pull`]: image pull progress from the Engine's JSON messages.
//! - [`endpoint`]: where the Docker daemon is (`DOCKER_HOST`, settings, platform defaults).
//! - [`tree`]: the Services tree, details (Dashboard / Inspection) and toolbar state.
//! - [`gutter`]: ▶ markers for compose services and Dockerfiles.
//! - [`portforward`]: the Kubernetes port-forward channel protocol and active forwards.
//! - [`ws`]: WebSocket handshake and framing (Kubernetes streaming subresources).
//!
//! With the `runtime` feature: an HTTP/1.1 client over Unix sockets, Windows named pipes,
//! TCP (+TLS) and SSH-forwarded sockets ([`http`], [`ssh`]), the Docker Engine client
//! ([`engine`]), the Kubernetes API client ([`kubeclient`]), port-forward connections
//! ([`forward`]), compose / CLI processes ([`cli`]) and registry credentials in the OS
//! keychain ([`secrets`]), the open project's machine ([`project`]: compose files and runs
//! on a remote project's host) and the [`hub`] that holds the Services state for the app.

pub mod compose;
pub mod docker;
pub mod endpoint;
pub mod exec;
pub mod gutter;
pub mod kube;
pub mod logs;
pub mod portforward;
pub mod pull;
pub mod time;
pub mod tree;
pub mod ws;

#[cfg(feature = "runtime")]
pub mod cli;
#[cfg(feature = "runtime")]
pub mod engine;
#[cfg(feature = "runtime")]
pub mod forward;
#[cfg(feature = "runtime")]
pub mod http;
#[cfg(feature = "runtime")]
pub mod hub;
#[cfg(feature = "runtime")]
pub mod kubeclient;
#[cfg(feature = "runtime")]
pub mod project;
#[cfg(feature = "runtime")]
pub mod secrets;
#[cfg(feature = "runtime")]
pub mod ssh;

/// Errors of this crate, worded for balloons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error(format!("unexpected response: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(Error(msg.into()))
}
