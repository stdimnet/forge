//! The data editor (`forge-dataedit`) for the browser-mode database mock, mirroring the
//! `db_data_*`, `db_fk_*` and `db_value_*` commands, plus the table query and DML preview
//! that the mock's `db_table_data`, `db_preview_changes` and `db_apply_changes` run.

use forge_dataedit::api;
use serde::de::DeserializeOwned;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn run<R: DeserializeOwned, T: serde::Serialize>(
    request_json: &str,
    f: impl FnOnce(&R) -> Result<T, String>,
) -> String {
    match serde_json::from_str::<R>(request_json) {
        Ok(r) => match f(&r) {
            Ok(v) => to_json(&v),
            Err(e) => to_json(&json!({ "error": e })),
        },
        Err(e) => to_json(&json!({ "error": e.to_string() })),
    }
}

/// `db_data_view`: `{ columns, rows, view }` → row indexes.
#[wasm_bindgen]
pub fn dataedit_view(request_json: &str) -> String {
    run(request_json, |r: &api::ViewRequest| Ok(api::view(r)))
}

/// `db_data_aggregate`: `{ columns, rows }` → `{ total, columns }`.
#[wasm_bindgen]
pub fn dataedit_aggregate(request_json: &str) -> String {
    run(request_json, |r: &api::AggregateRequest| Ok(api::aggregates(r)))
}

/// `db_data_compare`: `{ left, right, key }` → `DataDiff`.
#[wasm_bindgen]
pub fn dataedit_compare(request_json: &str) -> String {
    run(request_json, api::diff)
}

/// `db_data_chart`: `{ columns, rows, spec }` → `ChartData`.
#[wasm_bindgen]
pub fn dataedit_chart(request_json: &str) -> String {
    run(request_json, api::chart)
}

/// `db_fk_navigate` → `Navigation`.
#[wasm_bindgen]
pub fn dataedit_navigate(request_json: &str) -> String {
    run(request_json, api::navigate)
}

/// `db_fk_incoming` → `Incoming[]`.
#[wasm_bindgen]
pub fn dataedit_incoming(request_json: &str) -> String {
    run(request_json, |r: &api::IncomingRequest| Ok(api::incoming_keys(r)))
}

/// `db_value_editor` → `EditorKind`.
#[wasm_bindgen]
pub fn dataedit_value_editor(request_json: &str) -> String {
    run(request_json, |r: &api::EditorRequest| Ok(api::editor(r)))
}

/// `db_value_format` → `{ text?, error? }`.
#[wasm_bindgen]
pub fn dataedit_value_format(request_json: &str) -> String {
    run(request_json, |r: &api::FormatRequest| Ok(api::format(r)))
}

/// `db_value_binary` → `BinaryInfo | null`.
#[wasm_bindgen]
pub fn dataedit_value_binary(request_json: &str) -> String {
    run(request_json, |r: &api::BinaryRequest| Ok(api::binary(r)))
}

/// `db_value_temporal` → `string | null`.
#[wasm_bindgen]
pub fn dataedit_value_temporal(request_json: &str) -> String {
    run(request_json, |r: &api::TemporalRequest| Ok(api::temporal(r)))
}

/// `{ dialect, query }` → `TableSql` (the mock's `db_table_data`).
#[wasm_bindgen]
pub fn dataedit_table_query(request_json: &str) -> String {
    run(request_json, api::table_query)
}

/// `{ dialect, schema, table, info, changes }` → `{ statements, key }` (the mock's
/// `db_preview_changes` and `db_apply_changes`).
#[wasm_bindgen]
pub fn dataedit_preview(request_json: &str) -> String {
    run(request_json, api::preview_changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_the_commands() {
        let q = dataedit_table_query(
            r#"{"dialect":"mysql","query":{"schema":"shop","table":"orders","filter":"total > 1","order":[{"column":"id","desc":true}]}}"#,
        );
        assert!(q.contains("ORDER BY `id` DESC"), "{q}");
        let bad = dataedit_table_query(
            r#"{"dialect":"mysql","query":{"schema":"s","table":"t","filter":"1; drop table t"}}"#,
        );
        assert!(bad.contains("error"), "{bad}");
        let v = dataedit_view(
            r#"{"columns":["a"],"rows":[[2],[1]],"view":{"order":[{"column":"a"}]}}"#,
        );
        assert_eq!(v, "[1,0]");
        let p = dataedit_preview(
            r#"{"dialect":"mysql","schema":"s","table":"t","info":{"primaryKey":[]},"changes":[{"kind":"update","key":{},"values":{"a":1}}]}"#,
        );
        assert!(p.contains("no primary key"), "{p}");
    }
}
