//! The Database Explorer model of Forge (pure; also built to WebAssembly for browser mode):
//!
//! - [`model`]: the object tree of a data source ([`DbModel`]), cached per data source.
//! - [`filter`]: the schema filter patterns of a data source's Schemas tab.
//! - [`virtuals`]: virtual foreign keys and columns kept in the project.
//! - [`ddl`]: DDL data sources, SQL files parsed into the same model.
//! - [`search`]: full-text search queries per dialect and cell matching.
//! - [`generate`], [`doc`]: DDL from the model and Quick Documentation of tree nodes.
//! - [`design`]: the Create/Modify Table dialogs (CREATE and rename-aware ALTER per
//!   dialect), Drop and Rename; [`script`]: Generate DDL in dependency order.

pub mod ddl;
pub mod design;
pub mod doc;
pub mod filter;
pub mod generate;
pub mod model;
pub mod script;
pub mod search;
pub mod virtuals;

pub use doc::{document, ObjectDoc, ObjectRef};
pub use filter::{apply_patterns, SchemaFilter};
pub use model::*;
pub use search::{CellMatcher, SearchMode, SearchOptions};
pub use virtuals::{VirtualColumn, VirtualForeignKey, VirtualObjects, VIRTUAL_FILE};
