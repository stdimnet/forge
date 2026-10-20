//! Forge search: a persistent trigram index behind Find in Files and file-name search.

mod index;
mod search;
mod service;
mod symbols;
mod trigram;

pub use index::RefreshStats;
pub use search::{FileMatches, Match, Outcome, Query};
pub use service::{FileHit, SearchService};
pub use symbols::SymbolHit;

#[cfg(test)]
mod tests;
