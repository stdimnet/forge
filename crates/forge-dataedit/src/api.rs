//! Requests of the data editor's pure IPC commands (`db_data_*`, `db_fk_*`, `db_value_*`),
//! shared by the Tauri commands and the WebAssembly bridge so both answer the same way.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use forge_sql::{Dialect, RowChange};

use crate::dml::{choose_key, preview, KeyChoice, UniqueKey};
use crate::query::{TableQuery, TableSql};

use crate::aggregate::{aggregate, aggregate_columns, Aggregate, ColumnAggregate};
use crate::chart::{chart_data, ChartData, ChartSpec};
use crate::compare::{compare, DataDiff, DataSet};
use crate::local::{local_view, LocalView};
use crate::nav::{
    incoming, referenced_row, referencing_rows, FkRef, Incoming, Navigation, TableKeys,
};
use crate::value::{
    binary_info, editor_kind, format_json, format_xml, normalize_temporal, BinaryInfo, EditorKind,
    Formatted,
};

/// `db_data_view`: rows to show of a result set after local sort and filter.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewRequest {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    #[serde(default)]
    pub view: LocalView,
}

pub fn view(r: &ViewRequest) -> Vec<usize> {
    local_view(&r.columns, &r.rows, &r.view)
}

/// `db_data_aggregate`: the selected cells as columns and rows.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateRequest {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Aggregates {
    /// Over all selected cells (the status bar).
    pub total: Aggregate,
    /// Per column (the Aggregate view).
    pub columns: Vec<ColumnAggregate>,
}

pub fn aggregates(r: &AggregateRequest) -> Aggregates {
    let all: Vec<Value> = r.rows.iter().flatten().cloned().collect();
    Aggregates {
        total: aggregate(&all),
        columns: aggregate_columns(&r.columns, &r.rows),
    }
}

/// `db_data_compare`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareRequest {
    pub left: DataSet,
    pub right: DataSet,
    #[serde(default)]
    pub key: Vec<String>,
}

pub fn diff(r: &CompareRequest) -> Result<DataDiff, String> {
    compare(&r.left, &r.right, &r.key).map_err(|e| e.0)
}

/// `db_data_chart`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartRequest {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub spec: ChartSpec,
}

pub fn chart(r: &ChartRequest) -> Result<ChartData, String> {
    chart_data(&r.columns, &r.rows, &r.spec).map_err(|e| e.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Direction {
    /// The row the foreign key references (Ctrl+B).
    Referenced,
    /// The rows of `table` (the referencing table) that reference the given row.
    Referencing,
}

/// `db_fk_navigate`. For `referenced`, `schema.table` is the table of the row; for
/// `referencing`, it is the table that owns `fk`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigateRequest {
    pub dialect: Dialect,
    pub schema: String,
    pub table: String,
    pub fk: FkRef,
    pub columns: Vec<String>,
    pub row: Vec<Value>,
    pub direction: Direction,
}

pub fn navigate(r: &NavigateRequest) -> Result<Navigation, String> {
    match r.direction {
        Direction::Referenced => {
            referenced_row(r.dialect, &r.schema, &r.table, &r.fk, &r.columns, &r.row)
        }
        Direction::Referencing => {
            referencing_rows(r.dialect, &r.schema, &r.table, &r.fk, &r.columns, &r.row)
        }
    }
    .map_err(|e| e.0)
}

/// `db_fk_incoming`: foreign keys of `tables` that reference `schema.table`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingRequest {
    pub schema: String,
    pub table: String,
    pub tables: Vec<TableKeys>,
}

pub fn incoming_keys(r: &IncomingRequest) -> Vec<Incoming> {
    incoming(&r.schema, &r.table, &r.tables)
}

/// `db_value_editor`: which editor a cell gets.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorRequest {
    pub data_type: String,
    #[serde(default)]
    pub value: Value,
}

pub fn editor(r: &EditorRequest) -> EditorKind {
    editor_kind(&r.data_type, &r.value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    Json,
    Xml,
}

/// `db_value_format`: format (indent > 0) or minify JSON/XML, or report why it is invalid.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatRequest {
    pub language: Language,
    pub text: String,
    #[serde(default = "two")]
    pub indent: usize,
}

fn two() -> usize {
    2
}

pub fn format(r: &FormatRequest) -> Formatted {
    match r.language {
        Language::Json => format_json(&r.text, r.indent),
        Language::Xml => format_xml(&r.text, r.indent),
    }
}

/// `db_value_binary`: hex dump and image type of a binary cell.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryRequest {
    pub value: Value,
    #[serde(default = "max_bytes")]
    pub max_bytes: usize,
}

fn max_bytes() -> usize {
    4096
}

pub fn binary(r: &BinaryRequest) -> Option<BinaryInfo> {
    binary_info(&r.value, r.max_bytes)
}

/// `db_value_temporal`: normalize a typed date/time, `None` when invalid.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemporalRequest {
    pub kind: EditorKind,
    pub text: String,
}

pub fn temporal(r: &TemporalRequest) -> Option<String> {
    normalize_temporal(&r.kind, &r.text)
}

/// The parts of a table description (`TableInfo`) that pick its row key.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableShape {
    #[serde(default)]
    pub primary_key: Vec<String>,
    #[serde(default)]
    pub indexes: Vec<IndexShape>,
    #[serde(default)]
    pub columns: Vec<ColumnShape>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexShape {
    #[serde(default)]
    pub name: String,
    pub columns: Vec<String>,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnShape {
    pub name: String,
    #[serde(default)]
    pub nullable: bool,
}

/// The key that identifies rows of a table (see [`crate::dml::choose_key`]).
pub fn table_key(t: &TableShape) -> Option<KeyChoice> {
    let unique: Vec<UniqueKey> = t
        .indexes
        .iter()
        .filter(|i| i.unique && !i.primary)
        .map(|i| UniqueKey {
            name: i.name.clone(),
            columns: i.columns.clone(),
        })
        .collect();
    let nullable = |c: &str| {
        t.columns
            .iter()
            .find(|x| x.name == c)
            .map(|x| x.nullable)
            .unwrap_or(true)
    };
    choose_key(&t.primary_key, &unique, &nullable)
}

/// The browser mock's `db_preview_changes` / `db_apply_changes`: DML for `changes`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    pub dialect: Dialect,
    pub schema: String,
    pub table: String,
    pub info: TableShape,
    pub changes: Vec<RowChange>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResult {
    pub statements: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<KeyChoice>,
}

pub fn preview_changes(r: &PreviewRequest) -> Result<PreviewResult, String> {
    let key = table_key(&r.info);
    preview(r.dialect, &r.schema, &r.table, key.as_ref(), &r.changes)
        .map(|statements| PreviewResult { statements, key })
        .map_err(|e| e.0)
}

/// The browser mock's `db_table_data`: the statements of a table query.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableQueryRequest {
    pub dialect: Dialect,
    pub query: TableQuery,
}

pub fn table_query(r: &TableQueryRequest) -> Result<TableSql, String> {
    r.query.sql(r.dialect).map_err(|e| e.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn requests_deserialize_from_the_ui() {
        let r: NavigateRequest = serde_json::from_value(json!({
            "dialect": "mysql", "schema": "shop", "table": "orders",
            "fk": {"name": "f", "columns": ["user_id"], "refSchema": "shop", "refTable": "users",
                   "refColumns": ["id"], "onDelete": "CASCADE"},
            "columns": ["id", "user_id"], "row": [1, 7], "direction": "referenced"
        }))
        .unwrap();
        assert_eq!(navigate(&r).unwrap().filter, "`id` = 7");
        let a = aggregates(&AggregateRequest {
            columns: vec!["a".into(), "b".into()],
            rows: vec![vec![json!(1), json!(2)]],
        });
        assert_eq!(a.total.sum.as_deref(), Some("3"));
        let f: FormatRequest =
            serde_json::from_value(json!({"language": "json", "text": "[1]"})).unwrap();
        assert_eq!(format(&f).text.unwrap(), "[\n  1\n]");
        let t: TemporalRequest =
            serde_json::from_value(json!({"kind": {"kind": "date"}, "text": "2025-1-2"})).unwrap();
        assert_eq!(temporal(&t).as_deref(), Some("2025-01-02"));
        let p: PreviewRequest = serde_json::from_value(json!({
            "dialect": "mysql", "schema": "shop", "table": "users",
            "info": {"primaryKey": [], "columns": [{"name": "email", "nullable": false}],
                     "indexes": [{"name": "users_email_unique", "columns": ["email"], "unique": true, "primary": false}]},
            "changes": [{"kind": "delete", "key": {"email": "a@x.io"}}]
        }))
        .unwrap();
        let res = preview_changes(&p).unwrap();
        assert_eq!(
            res.statements,
            vec!["DELETE FROM `shop`.`users` WHERE `email` = 'a@x.io'"]
        );
    }
}
