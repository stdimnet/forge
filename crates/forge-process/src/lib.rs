//! Forge processes: everything the IDE starts on behalf of the user.
//!
//! - [`pty`]: interactive terminal sessions on a pseudo terminal (Terminal tool window).
//! - [`runner`]: plain processes with piped output for run configurations (Run tool window),
//!   with exit codes and process-tree kill.
//! - [`config`]: Run/Debug configurations, their persistence in
//!   `<project>/.forge/runConfigurations.toml` and the registry of configuration kinds that
//!   turns a configuration into a command line.

pub mod args;
pub mod config;
pub mod pty;
pub mod runner;
pub mod shell;
mod utf8;

pub use args::{quote_arg, split_args};
pub use config::{
    BeforeLaunchTask, BuildContext, KindRegistry, RunConfiguration, RunConfigurationKind,
    RunConfigurations,
};
pub use pty::{PtyEvent, PtyOptions, PtySession, PtySink};
pub use runner::{CommandLine, OutputStream, RunEvent, RunSink, RunningProcess};
pub use shell::default_shell;
pub use utf8::Utf8Decoder;
