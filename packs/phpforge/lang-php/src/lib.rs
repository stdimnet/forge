//! PHPForge language pack: PHP support built on tree-sitter.
//!
//! - [`parser`] indexes a file into a [`model::FileIndex`] (namespaces, imports, classes,
//!   members, functions, docblocks and attributes).
//! - [`docblock`] parses PHPDoc including Doctrine/Symfony annotations.
//! - [`index::ProjectIndex`] indexes a workspace in parallel with an mtime-keyed cache,
//!   fuzzy symbol search, file structure and lazy vendor resolution.
//! - [`hover`] renders index-based hover markdown.
//! - [`generate`] implements the "Generate" actions (constructor, accessors, overrides, ...).
//! - [`navigate`] Go to Declaration, Find Usages, implementations and gutter markers.
//! - [`rename`] Rename on the index: dialog labels and name clashes.
//! - [`create`] Create symbols from usage (methods, fields, constants, functions, classes).
//! - [`stubs`] PHP's own symbols from phpstorm-stubs (the "PHP Runtime").
//! - [`lsp`] describes how to launch Intelephense / phpactor.
//! - [`style`] brace placement and blank lines of the PHP code style.
//! - [`imports`], [`rearrange`] and [`cleanup`]: Optimize Imports, Rearrange Code and Code
//!   Cleanup (also run as Actions on Save).

pub mod cleanup;
pub mod composer;
pub mod create;
pub mod docblock;
pub mod fuzzy;
pub mod generate;
pub mod highlight;
pub mod hover;
pub mod imports;
pub mod index;
pub mod intentions;
pub mod lsp;
pub mod model;
pub mod navigate;
pub mod offsets;
pub mod parser;
pub mod rearrange;
pub mod rename;
pub mod resolve;
pub mod service;
pub mod structure;
pub mod stubs;
pub mod style;

pub use index::{IndexConfig, IndexStats, ProjectIndex};
pub use model::*;

/// Errors of this crate. The app maps them to `forge_core::ForgeError::Other`.
#[derive(Debug, thiserror::Error)]
pub enum PhpError {
    #[error("{0}")]
    Generate(String),
    #[error("io error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("cache error: {0}")]
    Cache(String),
}

pub type Result<T> = std::result::Result<T, PhpError>;
