//! Vim emulation for the Forge editor, following PHPStorm's IdeaVim plugin.
//!
//! - [`engine`]: the state machine ([`Vim`]): Normal, Insert, Visual (character, line,
//!   block), Replace and command-line modes, operators, motions, text objects, registers,
//!   dot repeat, macros, marks, search and mappings. The UI forwards key presses with the
//!   document changes since the last call and applies the returned edits and effects.
//! - [`command`]: parsing of Normal and Visual mode commands.
//! - [`motions`], [`textobj`]: cursor motions and text objects over a [`buffer::Buffer`].
//! - [`ex`]: ex commands (`:w`, `:s`, `:g`, `:normal`, `:set`, `:map`, `:action`, …).
//! - [`rc`]: `~/.ideavimrc` parsing.
//! - [`handlers`]: Vim or IDE handler per Ctrl shortcut (Settings | Editor | Vim).
//!
//! The crate is pure so it compiles to WebAssembly: every key press in the editor is
//! handled synchronously in-process (`crates/forge-wasm`).

pub mod buffer;
pub mod command;
pub mod engine;
pub mod ex;
pub mod handlers;
pub mod keys;
pub mod motions;
pub mod options;
pub mod pattern;
pub mod rc;
pub mod registers;
pub mod textobj;

pub use engine::{Change, Effect, Handle, Response, SelRange, Sync, Viewport, Vim};
pub use handlers::{HandlerOverride, HandlerRow};
pub use keys::DomKey;
pub use rc::RcReport;
