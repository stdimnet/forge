//! The data editor of the database tools (DataGrip's data editor), as pure functions over
//! dialects and cell values, so the desktop backend and the browser-mode UI (WebAssembly)
//! share them:
//!
//! - [`query`]: header sort and the filter row become `WHERE` / `ORDER BY` per dialect; the
//!   typed filter is checked so it stays a single read-only condition.
//! - [`local`]: sort and filter rows that came from an arbitrary query, in memory.
//! - [`dml`]: which key identifies a row (primary or unique key) and the DML preview of
//!   pending changes.
//! - [`nav`]: foreign-key navigation (real and virtual keys) in both directions.
//! - [`aggregate`]: count, sum, average, min, max and distinct count of cells.
//! - [`compare`]: the diff of two result sets by key.
//! - [`chart`]: series for line, bar, area, pie and scatter charts.
//! - [`value`]: the value editor: JSON and XML formatting with validation, hex dumps,
//!   image detection, enum values and date parsing.
//!
//! [`api`] holds the requests of the IPC commands, shared with the WebAssembly bridge.
//!
//! Cell values use the JSON conventions of `forge_sql::value`.

pub mod aggregate;
pub mod api;
pub mod chart;
pub mod compare;
pub mod dml;
pub mod local;
pub mod nav;
pub mod query;
pub mod value;

pub use aggregate::{aggregate, aggregate_columns, Aggregate, ColumnAggregate};
pub use chart::{chart_data, ChartData, ChartSpec};
pub use compare::{compare, DataDiff, DataSet, DiffRow, RowStatus};
pub use dml::{choose_key, preview, KeyChoice, UniqueKey};
pub use local::{local_view, LocalView};
pub use nav::{referenced_row, referencing_rows, FkRef, Navigation};
pub use query::{check_where, ColumnFilter, FilterOp, SortKey, TableQuery, TableSql};

use std::cmp::Ordering;

use serde_json::Value;

/// A cell as a number, when it is one (numbers, numeric strings such as decimals, booleans
/// are not numbers).
pub(crate) fn as_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() || !t.bytes().all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b)) {
                return None;
            }
            t.parse::<f64>().ok().filter(|f| f.is_finite())
        }
        _ => None,
    }
}

/// Text of a cell for comparisons and labels (`forge_sql::value::display`).
pub(crate) fn text(v: &Value) -> String {
    forge_sql::value::display(v)
}

/// Total order of cells: NULL first, then booleans, numbers (numerically, also numeric
/// strings), then text.
pub(crate) fn cmp_cells(a: &Value, b: &Value) -> Ordering {
    fn rank(v: &Value) -> u8 {
        match v {
            Value::Null => 0,
            Value::Bool(_) => 1,
            _ if as_number(v).is_some() => 2,
            _ => 3,
        }
    }
    match (rank(a), rank(b)) {
        (0, 0) => Ordering::Equal,
        (1, 1) => a.as_bool().cmp(&b.as_bool()),
        (2, 2) => as_number(a)
            .partial_cmp(&as_number(b))
            .unwrap_or(Ordering::Equal),
        (3, 3) => text(a).cmp(&text(b)),
        (x, y) => x.cmp(&y),
    }
}

/// Whether two cells hold the same value (`19.9` equals `"19.90"`).
pub(crate) fn same_cell(a: &Value, b: &Value) -> bool {
    if a == b {
        return true;
    }
    match (as_number(a), as_number(b)) {
        (Some(x), Some(y)) => x == y,
        _ => !a.is_null() && !b.is_null() && text(a) == text(b) && a.is_string() == b.is_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn orders_cells() {
        assert_eq!(cmp_cells(&Value::Null, &json!(1)), Ordering::Less);
        assert_eq!(cmp_cells(&json!("10.5"), &json!(9)), Ordering::Greater);
        assert_eq!(cmp_cells(&json!("b"), &json!("a")), Ordering::Greater);
        assert!(same_cell(&json!(19.9), &json!("19.90")));
        assert!(!same_cell(&json!("1"), &Value::Null));
    }
}
