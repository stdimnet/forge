//! RubyForge's view of Ruby on the user's machine (I/O; the language logic is in
//! `rubyforge-lang-ruby`):
//!
//! - [`detect`]: interpreters of version managers (rbenv, rvm, chruby, asdf, mise, frum, rv),
//!   Homebrew, RubyInstaller and the system; the project's choice; gem executables; the
//!   environment of Ruby commands.
//! - [`lsp`]: Ruby LSP — the user's install, or an install with the user's `gem` into
//!   Forge's data folder (with consent).
//! - [`rdbg`]: the `rdbg` debug adapter (the `debug` gem's DAP server).

pub mod detect;
pub mod lsp;
pub mod rdbg;

pub use detect::{command_env, detect, Chosen, Interpreter, Probe, RubyEnv, RubyStatus};
pub use rdbg::{RdbgAdapter, ADAPTER};
