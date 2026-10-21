//! Forge processes: everything the IDE starts on behalf of the user.
//!
//! - [`pty`]: interactive terminal sessions on a pseudo terminal (Terminal tool window).
//! - [`runner`]: plain processes with piped output for run configurations (Run tool window),
//!   with exit codes and process-tree kill.
//! - [`config`]: Run/Debug configurations, their persistence in
//!   `<project>/.forge/runConfigurations.toml` and the registry of configuration kinds that
//!   turns a configuration into a command line.
//! - [`output`]: the pure output models of the `forge-output` crate (ANSI, console lines,
//!   file links, TeamCity test trees), re-exported for the run backend.

pub mod args;
pub mod config;
pub mod integration;
pub mod pty;
pub mod runner;
pub mod shell;
pub mod termenv;
mod utf8;

pub use forge_output as output;

pub use args::{quote_arg, split_args};
pub use config::{
    BeforeLaunchTask, BuildContext, KindRegistry, RunConfiguration, RunConfigurationKind,
    RunConfigurations, TemplateKind,
};
pub use pty::{PtyEvent, PtyOptions, PtySession, PtySink};
pub use runner::{CommandLine, OutputStream, RunEvent, RunSink, RunningProcess};
pub use shell::default_shell;
pub use termenv::{terminal_env, utf8_locale_fallback};
pub use utf8::Utf8Decoder;
