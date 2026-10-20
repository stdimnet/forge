//! Doctrine DQL for PHPForge, without I/O so it also runs as WebAssembly in browser mode.
//!
//! - [`model`]: entity mapping metadata (filled by `phpforge-orm-doctrine`).
//! - [`dql`]: tokenizer, parser, semantic highlighting, completion and diagnostics.

pub mod dql;
pub mod model;

#[cfg(test)]
pub(crate) mod fixture {
    /// Entities of the `phpforge-orm-doctrine` test project, as its scanner reports them.
    pub fn snapshot() -> crate::model::Snapshot {
        serde_json::from_str(include_str!("../tests/fixture_snapshot.json")).expect("fixture")
    }
}
