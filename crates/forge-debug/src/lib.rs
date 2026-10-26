//! Forge's PHP debugger: a DBGp server for Xdebug 3.
//!
//! - [`DebugServer`] listens for Xdebug (default port 9003), accepts any number of
//!   connections and turns each into a [`Session`].
//! - Sessions identify their server (`PHP_IDE_CONFIG`, `SERVER_NAME`), map server paths to
//!   project files ([`PathMapper`]), set the project's breakpoints and report state changes
//!   as [`DebugEvent`]s.
//! - Breakpoints live in `<project>/.forge/breakpoints.json`, servers with their path
//!   mappings in `<project>/.forge/php-servers.json`.

pub mod breakpoints;
pub mod config;
pub mod connection;
pub mod console;
pub mod error;
pub mod events;
pub mod mapping;
pub mod protocol;
pub mod server;
pub mod servers;
pub mod session;
pub mod validation;
pub mod web;
pub mod xml;

pub use breakpoints::{
    Breakpoint, BreakpointKind, BreakpointSpec, BreakpointStore, BreakpointsSnapshot,
};
pub use config::DebugConfig;
pub use error::{DebugError, DebugResult};
pub use events::{
    BreakReason, DebugEvent, EventSink, ExceptionFilter, ExceptionInfo, Location, SessionFeatures,
    SessionInfo, SessionStatus,
};
pub use mapping::{PathMapper, PathMapping};
pub use protocol::{ContextName, Property, RemoteBreakpoint};
pub use server::DebugServer;
pub use servers::{ServerConfig, ServerIdentity};
pub use session::{Frame, Session, StepKind};

/// IDE key Forge uses when it starts PHP itself.
pub const IDE_KEY: &str = "FORGE";

/// Environment for a PHP process that should connect to Forge's debugger, e.g. a CLI
/// script or PHPUnit started from a Debug run configuration.
pub fn launch_env(port: u16, server_name: Option<&str>) -> Vec<(String, String)> {
    let mut env = vec![
        ("XDEBUG_MODE".to_string(), "debug".to_string()),
        ("XDEBUG_SESSION".to_string(), IDE_KEY.to_string()),
        (
            "XDEBUG_CONFIG".to_string(),
            format!("client_host=127.0.0.1 client_port={port} idekey={IDE_KEY}"),
        ),
    ];
    if let Some(name) = server_name.filter(|n| !n.is_empty()) {
        env.push(("PHP_IDE_CONFIG".to_string(), format!("serverName={name}")));
    }
    env
}

#[cfg(test)]
mod tests {
    #[test]
    fn launch_env_points_xdebug_at_the_listener() {
        let env = super::launch_env(9010, Some("docker"));
        assert!(env.contains(&("XDEBUG_MODE".into(), "debug".into())));
        assert!(env
            .iter()
            .any(|(k, v)| k == "XDEBUG_CONFIG" && v.contains("client_port=9010")));
        assert!(env.contains(&("PHP_IDE_CONFIG".into(), "serverName=docker".into())));
    }
}
