//! Jupyter notebooks (`pyforge-jupyter`) for the browser-mode mock, mirroring
//! `jupyter_open`, `jupyter_serialize`, `jupyter_render`, `jupyter_diff` and
//! `jupyter_dataframe_query`. Errors come back as `{ "error": … }`.

use pyforge_jupyter::dataframe::{self, DataFrame, Query};
use pyforge_jupyter::{Notebook, Output};
use serde::Deserialize;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::to_json;

fn error(e: impl ToString) -> String {
    to_json(&json!({ "error": e.to_string() }))
}

fn with<T: for<'de> Deserialize<'de>>(request_json: &str, f: impl FnOnce(T) -> String) -> String {
    match serde_json::from_str::<T>(request_json) {
        Ok(r) => f(r),
        Err(e) => error(e),
    }
}

#[derive(Deserialize)]
struct OpenRequest {
    text: String,
    #[serde(default)]
    root: Option<String>,
}

/// `{ text, root? }` → `NotebookView`.
#[wasm_bindgen]
pub fn jupyter_open(request_json: &str) -> String {
    with(request_json, |r: OpenRequest| {
        match pyforge_jupyter::open(&r.text, r.root.as_deref()) {
            Ok(v) => to_json(&v),
            Err(e) => error(e),
        }
    })
}

#[derive(Deserialize)]
struct SerializeRequest {
    notebook: Notebook,
    #[serde(default)]
    original: Option<String>,
}

/// `{ notebook, original? }` → the `.ipynb` text.
#[wasm_bindgen]
pub fn jupyter_serialize(request_json: &str) -> String {
    with(request_json, |r: SerializeRequest| {
        to_json(&pyforge_jupyter::serialize(
            &r.notebook,
            r.original.as_deref(),
        ))
    })
}

#[derive(Deserialize)]
struct RenderRequest {
    outputs: Vec<Output>,
    #[serde(default)]
    root: Option<String>,
}

/// `{ outputs, root? }` → `Rendered[]`.
#[wasm_bindgen]
pub fn jupyter_render(request_json: &str) -> String {
    with(request_json, |r: RenderRequest| {
        to_json(&pyforge_jupyter::render_outputs(
            &r.outputs,
            r.root.as_deref(),
        ))
    })
}

#[derive(Deserialize)]
struct DiffRequest {
    left: String,
    right: String,
}

/// `{ left, right }` → `NotebookDiff`.
#[wasm_bindgen]
pub fn jupyter_diff(request_json: &str) -> String {
    with(
        request_json,
        |r: DiffRequest| match pyforge_jupyter::diff_texts(&r.left, &r.right) {
            Ok(d) => to_json(&d),
            Err(e) => error(e),
        },
    )
}

#[derive(Deserialize)]
struct QueryRequest {
    frame: DataFrame,
    #[serde(default)]
    query: Query,
}

/// `{ frame, query }` → `Page`.
#[wasm_bindgen]
pub fn jupyter_dataframe_query(request_json: &str) -> String {
    with(request_json, |r: QueryRequest| {
        to_json(&dataframe::query(&r.frame, &r.query))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_the_commands() {
        let nb = r#"{"cells": [{"cell_type": "code", "id": "a", "metadata": {}, "source": "1", "execution_count": 1, "outputs": [{"output_type": "stream", "name": "stdout", "text": "x\n"}]}], "metadata": {}, "nbformat": 4, "nbformat_minor": 5}"#;
        let view = jupyter_open(&json!({ "text": nb }).to_string());
        assert!(view.contains(r#""kind":"text""#), "{view}");
        let v: serde_json::Value = serde_json::from_str(&view).unwrap();
        let text =
            jupyter_serialize(&json!({ "notebook": v["notebook"], "original": nb }).to_string());
        assert_eq!(
            serde_json::from_str::<String>(&text).unwrap(),
            nb.to_string() + ""
        );
        assert!(jupyter_open(r#"{"text": "{"}"#).contains("error"));
        let d = jupyter_diff(&json!({ "left": nb, "right": "" }).to_string());
        assert!(d.contains(r#""status":"removed""#), "{d}");
        let page = jupyter_dataframe_query(
            r#"{"frame": {"columns": ["a"], "rows": [["2"], ["10"]]}, "query": {"sort": {"column": 0, "descending": true}}}"#,
        );
        assert!(page.contains(r#""rowNumbers":[1,0]"#), "{page}");
        let r = jupyter_render(
            r#"{"outputs": [{"outputType": "stream", "name": "stderr", "text": "e"}]}"#,
        );
        assert!(r.contains(r#""error":true"#), "{r}");
    }
}
