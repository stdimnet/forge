//! LuaForge language pack: Lua 5.1–5.4 and LuaJIT support. Pure Rust (no file system or
//! processes), so the same code backs the desktop app and the browser mode (WebAssembly).
//!
//! - [`lexer`] and [`parser`] index a file into a [`model::FileIndex`]: locals with their
//!   scopes, globals, table fields, functions and methods (`M.f`, `M:f`), references,
//!   `require` calls and the module's `return`.
//! - [`annotations`] parses EmmyLua / LuaLS doc comments (`---@class`, `---@field`,
//!   `---@param`, `---@return`, `---@type`, `---@alias`, …).
//! - [`require`] resolves module names through `package.path`-like templates.
//! - [`project::ProjectIndex`] holds a workspace's files and resolves values across them
//!   (Go to Declaration, Search Everywhere).
//! - [`hover`] renders documentation as the `forge-markdown` AST, [`structure`] builds the
//!   Structure tool window and [`run`] finds main chunks for gutter run icons.

pub mod annotations;
pub mod hover;
pub mod lexer;
pub mod model;
pub mod offsets;
pub mod parser;
pub mod project;
pub mod require;
pub mod run;
pub mod structure;

pub use project::{IndexStats, Location, ProjectIndex, SearchKind, SymbolHit, SymbolKind};
