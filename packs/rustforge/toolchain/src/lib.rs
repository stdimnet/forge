//! RustForge's view of the Rust installation on the user's machine (I/O; the parsers live in
//! `rustforge-lang-rust`):
//!
//! - [`detect`]: rustup and its toolchains, the active toolchain of a folder, standalone
//!   `rustc` installs, sysroots, the standard library's sources, components, tools
//!   (rust-analyzer, rustfmt, Clippy) and cargo plugins (nextest, llvm-cov). Every rustup
//!   call runs with `RUSTUP_AUTO_INSTALL=0`, so looking never installs anything.
//! - [`install`]: rustup command lines for installing toolchains and components (the UI
//!   asks first and runs them in the Run tool window).
//! - [`analyzer`]: the consented rust-analyzer download from its GitHub releases (pinned
//!   release, SHA-256 checked against the release's asset digest).
//! - [`gdb`]: GDB with rustc's pretty printers as the `rust-gdb` debug adapter.

pub mod analyzer;
pub mod detect;
pub mod gdb;
pub mod install;

pub use detect::{RustEnv, RustStatus, ToolStatus};
