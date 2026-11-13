//! Data editor IPC (`forge_dataedit`): local sort and filter of result sets, aggregates,
//! data compare, chart series, foreign-key navigation and the value editor. All pure; the
//! browser mock runs the same code through WebAssembly (`crates/forge-wasm/src/dataedit.rs`).
//! The table-backed parts (server-side sort/filter, DML preview, submit) are in `db.rs`.

use forge_core::{ForgeError, Result};
use forge_db::dataedit::api::{self, Aggregates};
use forge_db::dataedit::chart::ChartData;
use forge_db::dataedit::compare::DataDiff;
use forge_db::dataedit::nav::{Incoming, Navigation};
use forge_db::dataedit::value::{BinaryInfo, EditorKind, Formatted};

fn refused(e: String) -> ForgeError {
    ForgeError::Other(e)
}

/// Rows of a result set to show, in order, after local sort and filter.
#[tauri::command]
pub fn db_data_view(request: api::ViewRequest) -> Vec<usize> {
    api::view(&request)
}

/// Count, sum, average, min, max and distinct count of the selected cells.
#[tauri::command]
pub fn db_data_aggregate(request: api::AggregateRequest) -> Aggregates {
    api::aggregates(&request)
}

/// Diff two result sets by key.
#[tauri::command]
pub fn db_data_compare(request: api::CompareRequest) -> Result<DataDiff> {
    api::diff(&request).map_err(refused)
}

/// Series of a chart over a result set.
#[tauri::command]
pub fn db_data_chart(request: api::ChartRequest) -> Result<ChartData> {
    api::chart(&request).map_err(refused)
}

/// Where foreign-key navigation from a row goes (table and filter).
#[tauri::command]
pub fn db_fk_navigate(request: api::NavigateRequest) -> Result<Navigation> {
    api::navigate(&request).map_err(refused)
}

/// Foreign keys (real and virtual) that reference a table.
#[tauri::command]
pub fn db_fk_incoming(request: api::IncomingRequest) -> Vec<Incoming> {
    api::incoming_keys(&request)
}

/// The value editor for a cell.
#[tauri::command]
pub fn db_value_editor(request: api::EditorRequest) -> EditorKind {
    api::editor(&request)
}

/// Format or minify JSON/XML, or say why it is invalid.
#[tauri::command]
pub fn db_value_format(request: api::FormatRequest) -> Formatted {
    api::format(&request)
}

/// Hex dump and image type of a binary cell.
#[tauri::command]
pub fn db_value_binary(request: api::BinaryRequest) -> Option<BinaryInfo> {
    api::binary(&request)
}

/// A typed date/time, normalized; `None` when invalid.
#[tauri::command]
pub fn db_value_temporal(request: api::TemporalRequest) -> Option<String> {
    api::temporal(&request)
}
