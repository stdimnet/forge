//! Schema tools of Forge's database support, modelled on DataGrip (pure; also built to
//! WebAssembly for browser mode):
//!
//! - [`diagram`]: ER diagrams of a schema or of selected tables — node sizes, a layered
//!   layout (referenced tables above the tables that reference them) and foreign-key edges
//!   routed between column rows.
//! - [`compare`]: the structural difference of two schemas (tables, views, columns, primary
//!   keys, indexes, foreign keys, checks, comments) as a diff tree.
//! - [`migrate`]: a dialect-aware migration script that turns the target schema into the
//!   source schema (SQLite tables are rebuilt where `ALTER TABLE` cannot change them).
//! - [`framework`]: the same migration as a Laravel or Doctrine migration file.
//! - [`plan`]: the `EXPLAIN` statement of each dialect and its output parsed into a plan tree
//!   with costs and rows, plus the flame-graph bars of the plan.

pub mod compare;
pub mod diagram;
pub mod framework;
pub mod migrate;
pub mod plan;

pub use compare::{
    compare, CompareOptions, DiffKind, ItemCategory, ItemDiff, SchemaDiff, TableDiff,
};
pub use diagram::{
    layout, ColumnsMode, Diagram, DiagramColumn, DiagramEdge, DiagramNode, DiagramRequest, Point,
};
pub use framework::{framework_migration, FrameworkKind, MigrationFile};
pub use migrate::{migration, Migration, MigrationOptions, MigrationStatement};
pub use plan::{
    explain_script, parse_plan, ExplainScript, FlameBar, Plan, PlanFormat, PlanNode, PlanProperty,
};
