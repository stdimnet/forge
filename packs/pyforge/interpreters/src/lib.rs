//! PyForge: Python interpreters (Settings | Project | Python Interpreter, the interpreter
//! status bar widget and its popup).
//!
//! - [`model`]: system, pyenv, venv/virtualenv, uv, Poetry, conda, Pipenv, Hatch, Docker,
//!   Docker Compose, SSH and WSL interpreters and their settings (`python.interpreters`,
//!   `python.projectInterpreters`, `python.interpreter`). Remote ones run through the PHP
//!   interpreters' command lines and path mappings (`phpforge-interpreters`).
//! - [`probe`]: parsers for `python --version`, the probe script, `pyvenv.cfg`,
//!   `conda-meta`, `py -0p` and the environment tools' answers.
//! - [`detect`]: interpreters on this machine and the project's environments.
//! - [`create`]: the commands that create a new environment.
//!
//! Everything except [`detect`] is pure and also builds to WebAssembly.

pub mod create;
#[cfg(not(target_arch = "wasm32"))]
pub mod detect;
pub mod model;
pub mod probe;

pub use model::{EnvKind, InterpreterSet, PythonInterpreter};
pub use probe::InterpreterInfo;
