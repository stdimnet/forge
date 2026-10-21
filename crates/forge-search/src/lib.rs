//! Forge search: a persistent trigram index behind Find in Files and file-name search.
//! The engine lives in `forge-search-core`; this crate walks the disk, persists and watches.

mod index;
mod lazy;
mod service;

pub use forge_search_core::{FileHit, FileMatches, Match, Outcome, Query, RefreshStats, SymbolHit};
pub use lazy::FoundText;
pub use service::{IndexOptions, SearchService};

#[cfg(test)]
mod tests;
