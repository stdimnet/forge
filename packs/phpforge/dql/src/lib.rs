//! Doctrine DQL for PHPForge, without I/O so it also runs as WebAssembly in browser mode.
//!
//! - [`model`]: entity mapping metadata (filled by `phpforge-orm-doctrine`).
//! - [`dql`]: tokenizer, parser, semantic highlighting, completion and diagnostics.
//! - [`site`]: DQL inside PHP strings (`createQuery()`, QueryBuilder calls) and the
//!   source-level functions over a [`site::SiteFinder`]; [`lexical`] is a finder without a
//!   PHP parser, [`editor`] the same functions with UTF-16 offsets.

pub mod dql;
pub mod editor;
pub mod lexical;
pub mod model;
pub mod site;

#[cfg(test)]
pub(crate) mod fixture {
    /// Entities of the `phpforge-orm-doctrine` test project, as its scanner reports them.
    pub fn snapshot() -> crate::model::Snapshot {
        serde_json::from_str(include_str!("../tests/fixture_snapshot.json")).expect("fixture")
    }
}
