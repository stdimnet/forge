//! Pure SQL text handling shared by `forge-db` and the browser-mode UI (WebAssembly):
//! dialect quoting, statement splitting and the statement at the caret, cell values as
//! JavaScript-safe JSON, row change statements and export.

pub mod dialect;
pub mod edit;
pub mod export;
pub mod split;
pub mod value;

pub use dialect::Dialect;
pub use edit::{change_sql, ChangeKind, Refused, RowChange};
pub use export::{export, ExportFormat};
pub use split::{leading_keyword, split_statements, statement_at, Statement};
