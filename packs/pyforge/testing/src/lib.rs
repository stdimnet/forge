//! PyForge's test runners, debugger, console and profiler support (PyCharm's Python
//! tools), pure so the desktop app and the WebAssembly build share it:
//!
//! - [`source`]: a scanner of Python source (logical lines, classes, functions,
//!   decorators).
//! - [`discover`]: pytest and unittest tests of a file with their parametrized cases, as
//!   gutter targets with temporary configurations ([`config`]).
//! - [`detect`]: whether a project uses pytest or tox, and the runner of a file.
//! - [`command`]: the command lines of pytest, unittest (Forge's runner,
//!   `helpers/forge_unittest.py`), tox, debugpy, cProfile and the console
//!   (`helpers/forge_console.py`).
//! - [`rerun`]: Rerun Failed Tests as pytest node ids or unittest ids.
//! - [`tox`]: tox environments from `tox.ini`, `setup.cfg`, `tox.toml` and `pyproject.toml`.
//! - [`pstats`]: cProfile's `.pstats` files: hot spots and the call tree.
//! - [`console`]: the Python Console's reply protocol.
//! - [`provider`]: the `forge_testing::TestFrameworkProvider` of the three runners.

pub mod command;
pub mod config;
pub mod console;
pub mod detect;
pub mod discover;
pub mod provider;
pub mod pstats;
pub mod rerun;
pub mod source;
pub mod tox;

pub use discover::{Runner, Scan};
pub use provider::PythonTestFrameworks;
