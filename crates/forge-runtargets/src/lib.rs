//! Run targets, like IntelliJ's "Run on": a run, test or debug run of any pack executes on
//! the local machine, in a Docker image, in a Docker Compose service, over SSH, in WSL or in
//! a Dev Container.
//!
//! - [`mapping`]: local ↔ target path mappings (also used by the PHP CLI interpreters).
//! - [`target`]: the [`RunTarget`] of a run configuration (`runOn`), its automatic
//!   mappings, the command line that starts an [`Invocation`] there ([`wrap`]) and what a
//!   debugger needs ([`DebugTarget`]).
//!
//! Pure (no I/O): the desktop backend wraps commands before it starts them, browser mode
//! runs the same code as WebAssembly. `forge-devenv` (Dev Container projects) and the PHP
//! CLI interpreters build their command lines with it too.

pub mod mapping;
pub mod target;

pub use mapping::{Mapper, PathMapping};
pub use target::{
    automatic, context, debug_target, effective, wrap, DebugDirection, DebugPort, DebugTarget,
    Invocation, Launcher, Lifecycle, RunTarget, TargetContext, TargetKind, Wrapped,
};
