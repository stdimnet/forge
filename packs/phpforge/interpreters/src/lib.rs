//! PHPForge: PHP CLI interpreters (Settings | Languages & Frameworks | PHP, "CLI
//! Interpreter" and the "CLI Interpreters" dialog).
//!
//! - [`model`]: local, Docker, Docker Compose, SSH and WSL interpreters and how they are
//!   stored in the user (`php.interpreters`) and project (`php.projectInterpreters`)
//!   settings; `php.interpreter` selects one by name.
//! - [`probe`]: version, php.ini, Xdebug and extensions from `php -v`, `php --ini` and
//!   `php -m`.
//! - [`detect`]: local interpreters on `PATH`, in system folders, Homebrew, version
//!   managers, Herd, Laragon and XAMPP.
//! - [`mapping`]: automatic and user path mappings, both directions.
//! - [`command`]: the command line that runs PHP (or Composer) through an interpreter.
//!
//! Everything except [`detect`] is pure and also builds to WebAssembly.

pub mod command;
#[cfg(not(target_arch = "wasm32"))]
pub mod detect;
pub mod mapping;
pub mod model;
pub mod probe;

pub use command::{wrap, Invocation, Program, SshTarget, WrapContext, Wrapped};
pub use mapping::{Mapper, PathMapping};
pub use model::{ConfigOption, InterpreterKind, InterpreterSet, Lifecycle, PhpInterpreter};
pub use probe::InterpreterInfo;
