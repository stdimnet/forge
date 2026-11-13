//! Row editing from the data grid; the statements are built by `forge_sql::edit`.

pub use forge_sql::edit::{ChangeKind, RowChange};

use crate::config::Dialect;
use crate::error::{DbError, DbResult};

/// SQL for one change, or an error when the row cannot be identified.
pub fn change_sql(
    d: Dialect,
    schema: &str,
    table: &str,
    pk: &[String],
    change: &RowChange,
) -> DbResult<String> {
    forge_sql::change_sql(d, schema, table, pk, change).map_err(|e| DbError::Refused(e.0))
}
