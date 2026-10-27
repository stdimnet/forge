//! The SQL editor of Forge, modelled on DataGrip (pure; also built to WebAssembly):
//!
//! - [`lang`]: dialects per file and folder (SQL Dialects mappings), keywords and quirks.
//! - [`lex`]: an error-tolerant tokenizer with dialect quoting rules.
//! - [`resolve`]: names of a script mapped to the data source model ([`catalog`]) and to its
//!   own aliases, CTEs and subqueries.
//! - [`inspect`]: the SQL inspections and their quick fixes.
//! - [`navigate`]: Go to Declaration (tree node or DDL source), Find Usages in the file and
//!   alias rename.
//! - [`format`]: the formatter behind Reformat Code and the Code Style page.
//! - [`params`]: user parameters (`?`, `:name`, `$1`, `${name}`, `#{name}`).
//! - [`history`]: the query history model.
//! - [`api`]: IPC requests and answers with UTF-16 offsets.

pub mod api;
pub mod catalog;
pub mod format;
pub mod history;
pub mod inspect;
pub mod lang;
pub mod lex;
pub mod navigate;
pub mod params;
pub mod resolve;

pub use format::{format, SqlStyle};
pub use lang::{data_source_for, dialect_for, DialectMapping, SqlDialect};
pub use params::ParamPatterns;
