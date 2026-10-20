//! Pure engine of Forge code search. No file system, threads or clocks: callers feed it file
//! contents and read files back through a closure, so it also builds for WebAssembly.

#[macro_use]
mod par;
mod index;
mod search;
mod symbols;
mod trigram;

pub use index::{
    Analysis, FileHit, Index, Loaded, Plan, RefreshStats, Scanned, Scope, MAX_CONTENT_BYTES,
};
pub use search::{run, Compiled, FileMatches, Match, Outcome, Query};
pub use symbols::{RawSymbol, SymbolHit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
