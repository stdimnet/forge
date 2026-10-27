//! Moving data in and out of databases (pure; also built to WebAssembly):
//!
//! - [`read`]: import sources (CSV/TSV with quirks and encodings, Excel, JSON / NDJSON).
//! - [`detect`]: column types of a source and the SQL types of a new target table.
//! - [`insert`]: batched `INSERT` statements of an import plan, per dialect.
//! - [`xlsx`]: the Excel workbook of "Export to Excel".
//! - [`dump`]: command lines of the native dump and restore tools.

pub mod detect;
pub mod dump;
pub mod insert;
pub mod read;
pub mod xlsx;

pub use detect::{detect, ColumnGuess, ValueKind};
pub use dump::{DumpAction, DumpRequest, DumpTarget, DumpTool, ToolCommand};
pub use insert::{Batch, ColumnMapping, ImportPlan, OnError};
pub use read::{read, ReadOptions, SourceFormat, SourceTable};

/// The import dialog's preview of a source: the first rows and the detected columns.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub table: SourceTable,
    pub guesses: Vec<ColumnGuess>,
    /// Rows of the whole source.
    pub total_rows: usize,
}

/// Read a source and keep its first `rows` rows; types are detected on up to 1000 rows.
pub fn preview(bytes: &[u8], opts: &ReadOptions, rows: usize) -> Result<Preview, String> {
    let mut table = read(bytes, opts)?;
    let guesses = detect(&table, 1000);
    let total_rows = table.rows.len();
    table.rows.truncate(rows);
    Ok(Preview {
        table,
        guesses,
        total_rows,
    })
}
