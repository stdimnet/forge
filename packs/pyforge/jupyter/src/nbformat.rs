//! nbformat v4: `.ipynb` files into a [`Notebook`] for the editor and back.
//!
//! Serialization is round-trip stable: [`serialize`] starts from the original file's JSON
//! and only rewrites what changed, so unknown keys, cell ids, metadata, key order, the
//! string-or-list form of multi-line texts, the indent and the final line break stay as they
//! were. Unchanged notebooks serialize to the same bytes as Jupyter writes them
//! (`json.dumps(nb, sort_keys=True, indent=1, ensure_ascii=False)` plus `\n`). New cells and
//! outputs are written the way Jupyter writes them (sorted keys, texts split into lines).

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::ordered::{OMap, OValue};
use crate::output::{output_from_json, output_to_json, Output};

/// Kind of a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CellType {
    #[default]
    Code,
    Markdown,
    Raw,
}

impl CellType {
    pub fn as_str(self) -> &'static str {
        match self {
            CellType::Code => "code",
            CellType::Markdown => "markdown",
            CellType::Raw => "raw",
        }
    }

    fn parse(s: &str) -> CellType {
        match s {
            "markdown" => CellType::Markdown,
            "raw" => CellType::Raw,
            _ => CellType::Code,
        }
    }
}

/// A cell as the editor sees it. Texts are joined; outputs are typed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Cell {
    /// The cell's id (nbformat 4.5+), or one made up by the editor for older notebooks
    /// (written only when the notebook has ids, see [`Notebook::has_ids`]).
    pub id: String,
    pub cell_type: CellType,
    pub source: String,
    #[serde(default = "empty_object")]
    pub metadata: Value,
    #[serde(default)]
    pub execution_count: Option<u64>,
    #[serde(default)]
    pub outputs: Vec<Output>,
    /// Markdown and raw cells: attached files (`attachment:` images).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Value>,
    /// Position of the cell in the file it was read from. The editor keeps it when cells
    /// move and drops it on copies, so [`serialize`] finds the original JSON of the cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<usize>,
}

fn empty_object() -> Value {
    Value::Object(Map::new())
}

/// A notebook for the editor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notebook {
    pub nbformat: u64,
    pub nbformat_minor: u64,
    pub metadata: Value,
    pub cells: Vec<Cell>,
    /// Language of the code cells (`metadata.kernelspec.language`,
    /// `metadata.language_info.name`), `python` when unknown.
    #[serde(default = "python")]
    pub language: String,
    /// Cells carry ids (nbformat 4.5+).
    #[serde(default)]
    pub has_ids: bool,
}

fn python() -> String {
    "python".into()
}

impl Default for Notebook {
    fn default() -> Self {
        new_notebook()
    }
}

/// An empty nbformat 4.5 notebook with a Python 3 kernel spec.
pub fn new_notebook() -> Notebook {
    Notebook {
        nbformat: 4,
        nbformat_minor: 5,
        metadata: serde_json::json!({
            "kernelspec": { "display_name": "Python 3", "language": "python", "name": "python3" },
            "language_info": { "name": "python" }
        }),
        cells: Vec::new(),
        language: python(),
        has_ids: true,
    }
}

/// A multi-line text as nbformat stores it: a string or a list of lines.
pub fn join_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts.iter().filter_map(Value::as_str).collect(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Lines of a text including their line breaks, as Jupyter writes multi-line texts.
pub fn split_lines(text: &str) -> Vec<Value> {
    text.split_inclusive('\n')
        .map(|l| Value::String(l.to_string()))
        .collect()
}

/// Reading errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotebookError(pub String);

impl std::fmt::Display for NotebookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NotebookError {}

/// Read a notebook. Empty text is a new notebook; nbformat 3 and older are refused.
pub fn parse(text: &str) -> Result<Notebook, NotebookError> {
    if text.trim().is_empty() {
        return Ok(new_notebook());
    }
    let root: Value = serde_json::from_str(text)
        .map_err(|e| NotebookError(format!("Not a valid notebook (JSON error: {e})")))?;
    let obj = root.as_object().ok_or_else(|| {
        NotebookError("Not a valid notebook: the file is not a JSON object".into())
    })?;
    let nbformat = obj.get("nbformat").and_then(Value::as_u64).unwrap_or(4);
    if nbformat < 4 {
        return Err(NotebookError(format!(
            "nbformat {nbformat} notebooks are not supported; convert them with `jupyter nbconvert --to notebook`"
        )));
    }
    let nbformat_minor = obj
        .get("nbformat_minor")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let metadata = obj.get("metadata").cloned().unwrap_or_else(empty_object);
    let raw_cells = obj
        .get("cells")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let has_ids =
        nbformat > 4 || nbformat_minor >= 5 || raw_cells.iter().any(|c| c.get("id").is_some());
    let cells = raw_cells
        .iter()
        .enumerate()
        .map(|(i, c)| cell_from_json(c, i))
        .collect();
    Ok(Notebook {
        nbformat,
        nbformat_minor,
        language: language_of(&metadata),
        metadata,
        cells,
        has_ids,
    })
}

fn language_of(metadata: &Value) -> String {
    metadata
        .pointer("/kernelspec/language")
        .or_else(|| metadata.pointer("/language_info/name"))
        .and_then(Value::as_str)
        .map(str::to_lowercase)
        .unwrap_or_else(python)
}

fn cell_from_json(c: &Value, index: usize) -> Cell {
    let cell_type = CellType::parse(c.get("cell_type").and_then(Value::as_str).unwrap_or("code"));
    Cell {
        id: c
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("cell-{index}")),
        cell_type,
        source: c.get("source").map(join_text).unwrap_or_default(),
        metadata: c.get("metadata").cloned().unwrap_or_else(empty_object),
        execution_count: c.get("execution_count").and_then(Value::as_u64),
        outputs: c
            .get("outputs")
            .and_then(Value::as_array)
            .map(|o| o.iter().map(output_from_json).collect())
            .unwrap_or_default(),
        attachments: c.get("attachments").cloned(),
        origin: Some(index),
    }
}

/// How the original file was formatted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Format {
    pub indent: String,
    pub final_newline: bool,
    /// Non-ASCII characters were written as `\uXXXX` escapes.
    pub ascii: bool,
    /// One line without indent; `spaced` with Python's `", "` / `": "` separators.
    pub compact: bool,
    pub spaced: bool,
}

impl Default for Format {
    fn default() -> Self {
        Format {
            indent: " ".into(),
            final_newline: true,
            ascii: false,
            compact: false,
            spaced: false,
        }
    }
}

/// The indent, final line break and escaping of a notebook file.
pub fn detect_format(text: &str) -> Format {
    let mut f = Format::default();
    if text.trim().is_empty() {
        return f;
    }
    if let Some(line) = text.lines().nth(1) {
        let ws: String = line
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        if !ws.is_empty() {
            f.indent = ws;
        }
    }
    f.final_newline = text.ends_with('\n');
    f.compact = !text.trim_end().contains('\n');
    f.spaced = f.compact && (text.contains("\": ") || text.contains(", "));
    f.ascii = text.is_ascii() && has_non_ascii_escape(text);
    f
}

fn has_non_ascii_escape(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = 0;
    while i + 5 < b.len() {
        if b[i] == b'\\' {
            if b[i + 1] == b'u' {
                if let Ok(h) = std::str::from_utf8(&b[i + 2..i + 6]) {
                    if u32::from_str_radix(h, 16).is_ok_and(|n| n >= 0x80) {
                        return true;
                    }
                }
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    false
}

/// Write `nb` as `.ipynb` text, keeping everything of `original` (the file's current text)
/// that did not change.
pub fn serialize(nb: &Notebook, original: Option<&str>) -> String {
    let original_root = original
        .and_then(|t| serde_json::from_str::<OValue>(t).ok())
        .filter(|v| v.as_object().is_some());
    let format = original
        .filter(|_| original_root.is_some())
        .map(detect_format)
        .unwrap_or_default();
    let root = notebook_json(nb, original_root.as_ref());
    let mut text = write_json(&root, &format);
    if format.final_newline {
        text.push('\n');
    }
    text
}

/// The notebook's JSON, merged into the original's.
pub fn notebook_json(nb: &Notebook, original: Option<&OValue>) -> OValue {
    let orig = original.and_then(OValue::as_object);
    let orig_cells: &[OValue] = orig
        .and_then(|o| o.get("cells"))
        .and_then(OValue::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let cells: Vec<OValue> = nb
        .cells
        .iter()
        .map(|c| {
            let o = c
                .origin
                .and_then(|i| orig_cells.get(i))
                .and_then(OValue::as_object);
            OValue::Object(cell_json(c, o, nb.has_ids))
        })
        .collect();
    let mut out = orig.cloned().unwrap_or_default();
    let sorted = orig.is_none_or(OMap::keys_sorted);
    out.insert("cells", OValue::Array(cells));
    if out.get("metadata").map(OValue::to_value).as_ref() != Some(&nb.metadata) {
        out.insert("metadata", OValue::from(&nb.metadata));
    }
    set_if_changed(&mut out, "nbformat", nb.nbformat);
    set_if_changed(&mut out, "nbformat_minor", nb.nbformat_minor);
    if sorted {
        out.sort();
    }
    OValue::Object(out)
}

fn set_if_changed(map: &mut OMap, key: &str, value: u64) {
    if map.get(key).and_then(OValue::as_u64) != Some(value) {
        map.insert(key, OValue::Number(value.into()));
    }
}

fn text_like(text: &str, original: Option<&OValue>) -> OValue {
    match original {
        Some(OValue::String(_)) => OValue::String(text.to_string()),
        _ => OValue::Array(
            text.split_inclusive('\n')
                .map(|l| OValue::String(l.to_string()))
                .collect(),
        ),
    }
}

/// A cell's JSON: the original's with the changed parts replaced.
fn cell_json(cell: &Cell, original: Option<&OMap>, write_id: bool) -> OMap {
    let mut out = original.cloned().unwrap_or_default();
    let sorted = original.is_none_or(OMap::keys_sorted);
    let str_of = |m: &OMap, k: &str| m.get(k).and_then(OValue::as_str).map(str::to_string);
    if str_of(&out, "cell_type").as_deref() != Some(cell.cell_type.as_str()) {
        out.insert("cell_type", OValue::String(cell.cell_type.as_str().into()));
    }
    if write_id && !cell.id.is_empty() && str_of(&out, "id").as_deref() != Some(cell.id.as_str()) {
        out.insert("id", OValue::String(cell.id.clone()));
    }
    if out.get("metadata").map(OValue::to_value).as_ref() != Some(&cell.metadata) {
        out.insert("metadata", OValue::from(&cell.metadata));
    }
    let source = out.get("source").map(|v| join_text(&v.to_value()));
    if source.as_deref() != Some(cell.source.as_str()) {
        let v = text_like(&cell.source, out.get("source"));
        out.insert("source", v);
    }
    match &cell.attachments {
        Some(a) if out.get("attachments").map(OValue::to_value).as_ref() != Some(a) => {
            out.insert("attachments", OValue::from(a));
        }
        None if cell.cell_type == CellType::Code => out.remove("attachments"),
        _ => {}
    }
    if cell.cell_type == CellType::Code {
        let count = cell
            .execution_count
            .map_or(OValue::Null, |n| OValue::Number(n.into()));
        if out.get("execution_count") != Some(&count) {
            out.insert("execution_count", count);
        }
        let orig_outputs: Vec<OValue> = out
            .get("outputs")
            .and_then(OValue::as_array)
            .cloned()
            .unwrap_or_default();
        let outputs: Vec<OValue> = cell
            .outputs
            .iter()
            .enumerate()
            .map(|(i, o)| {
                let raw = orig_outputs.get(i);
                let value = raw.map(OValue::to_value);
                match (raw, &value) {
                    (Some(raw), Some(v)) if &output_from_json(v) == o => raw.clone(),
                    _ => OValue::from(&output_to_json(o, value.as_ref())),
                }
            })
            .collect();
        if out.get("outputs").and_then(OValue::as_array) != Some(&outputs) {
            out.insert("outputs", OValue::Array(outputs));
        }
    } else {
        out.remove("outputs");
        out.remove("execution_count");
    }
    if sorted {
        out.sort();
    }
    out
}

/// JSON text with the given indent (`indent=1` style), escaping non-ASCII when `ascii`.
pub fn write_json(value: &impl Serialize, format: &Format) -> String {
    let mut buf = Vec::new();
    let pretty = serde_json::ser::PrettyFormatter::with_indent(format.indent.as_bytes());
    let mut ser = serde_json::Serializer::with_formatter(
        &mut buf,
        NbFormatter {
            inner: pretty,
            ascii: format.ascii,
            compact: format.compact,
            spaced: format.spaced,
        },
    );
    value
        .serialize(&mut ser)
        .expect("serializing a JSON value cannot fail");
    String::from_utf8(buf).expect("serde_json writes UTF-8")
}

/// serde_json's pretty formatter, optionally escaping non-ASCII like Python's
/// `ensure_ascii=True`.
struct NbFormatter<'a> {
    inner: serde_json::ser::PrettyFormatter<'a>,
    ascii: bool,
    compact: bool,
    spaced: bool,
}

macro_rules! structure {
    ($($name:ident($($arg:ident : $ty:ty),*) => $compact:expr),* $(,)?) => {
        $(
            fn $name<W: ?Sized + std::io::Write>(&mut self, w: &mut W $(, $arg: $ty)*) -> std::io::Result<()> {
                if self.compact {
                    let text: &str = $compact(self.spaced $(, $arg)*);
                    w.write_all(text.as_bytes())
                } else {
                    self.inner.$name(w $(, $arg)*)
                }
            }
        )*
    };
}

fn sep(spaced: bool, first: bool) -> &'static str {
    match (first, spaced) {
        (true, _) => "",
        (false, true) => ", ",
        (false, false) => ",",
    }
}

impl serde_json::ser::Formatter for NbFormatter<'_> {
    structure!(
        begin_array() => |_| "[",
        end_array() => |_| "]",
        begin_array_value(first: bool) => sep,
        end_array_value() => |_| "",
        begin_object() => |_| "{",
        end_object() => |_| "}",
        begin_object_key(first: bool) => sep,
        end_object_key() => |_| "",
        begin_object_value() => |spaced| if spaced { ": " } else { ":" },
        end_object_value() => |_| "",
    );

    fn write_string_fragment<W: ?Sized + std::io::Write>(
        &mut self,
        w: &mut W,
        fragment: &str,
    ) -> std::io::Result<()> {
        if !self.ascii || fragment.is_ascii() {
            return w.write_all(fragment.as_bytes());
        }
        for ch in fragment.chars() {
            if ch.is_ascii() {
                w.write_all(&[ch as u8])?;
            } else {
                let mut units = [0u16; 2];
                for u in ch.encode_utf16(&mut units) {
                    write!(w, "\\u{u:04x}")?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::MimeBundle;

    const MINIMAL: &str = r##"{
 "cells": [
  {
   "cell_type": "markdown",
   "id": "a1",
   "metadata": {},
   "source": [
    "# Title\n",
    "Text"
   ]
  },
  {
   "cell_type": "code",
   "execution_count": 2,
   "id": "b2",
   "metadata": {
    "tags": []
   },
   "outputs": [
    {
     "name": "stdout",
     "output_type": "stream",
     "text": [
      "hi\n"
     ]
    }
   ],
   "source": [
    "print('hi')"
   ]
  }
 ],
 "metadata": {
  "kernelspec": {
   "display_name": "Python 3",
   "language": "python",
   "name": "python3"
  }
 },
 "nbformat": 4,
 "nbformat_minor": 5
}
"##;

    #[test]
    fn reads_cells_and_outputs() {
        let nb = parse(MINIMAL).unwrap();
        assert_eq!(nb.cells.len(), 2);
        assert!(nb.has_ids);
        assert_eq!(nb.language, "python");
        assert_eq!(nb.cells[0].cell_type, CellType::Markdown);
        assert_eq!(nb.cells[0].source, "# Title\nText");
        assert_eq!(nb.cells[1].execution_count, Some(2));
        assert_eq!(
            nb.cells[1].outputs,
            vec![Output::Stream {
                name: "stdout".into(),
                text: "hi\n".into()
            }]
        );
    }

    #[test]
    fn unchanged_notebooks_round_trip_byte_for_byte() {
        let nb = parse(MINIMAL).unwrap();
        assert_eq!(serialize(&nb, Some(MINIMAL)), MINIMAL);
    }

    #[test]
    fn edits_keep_the_rest() {
        let mut nb = parse(MINIMAL).unwrap();
        nb.cells[1].source = "print('hi')\nprint('there')".into();
        nb.cells[1].outputs.clear();
        nb.cells[1].execution_count = None;
        let out = serialize(&nb, Some(MINIMAL));
        assert!(
            out.contains("   \"source\": [\n    \"print('hi')\\n\",\n    \"print('there')\"\n   ]"),
            "{out}"
        );
        assert!(out.contains("\"execution_count\": null"), "{out}");
        assert!(out.contains("\"outputs\": []"), "{out}");
        assert!(out.contains("\"tags\": []"), "{out}");
        assert_eq!(parse(&out).unwrap().cells[1].source, nb.cells[1].source);
    }

    #[test]
    fn new_cells_moves_and_type_changes() {
        let mut nb = parse(MINIMAL).unwrap();
        nb.cells.swap(0, 1);
        nb.cells.push(Cell {
            id: "c3".into(),
            cell_type: CellType::Code,
            source: "1 + 1".into(),
            metadata: empty_object(),
            execution_count: Some(3),
            outputs: vec![Output::ExecuteResult {
                execution_count: Some(3),
                data: MimeBundle::from_iter([("text/plain".to_string(), Value::from("2"))]),
                metadata: empty_object(),
            }],
            attachments: None,
            origin: None,
        });
        nb.cells[1].cell_type = CellType::Raw;
        let out = serialize(&nb, Some(MINIMAL));
        let back = parse(&out).unwrap();
        assert_eq!(back.cells[0].id, "b2");
        assert_eq!(back.cells[1].cell_type, CellType::Raw);
        assert_eq!(back.cells[2].id, "c3");
        assert_eq!(back.cells[2].outputs, nb.cells[2].outputs);
        // New cells are written like Jupyter writes them: sorted keys, split lines.
        assert!(out.contains(
            "  {\n   \"cell_type\": \"code\",\n   \"execution_count\": 3,\n   \"id\": \"c3\",\n   \"metadata\": {},\n   \"outputs\": ["
        ), "{out}");
        let raw = &serde_json::from_str::<Value>(&out).unwrap()["cells"][1];
        assert!(raw.get("outputs").is_none() && raw.get("execution_count").is_none());
    }

    #[test]
    fn keeps_indent_key_order_string_sources_and_escapes() {
        let text = "{\n    \"nbformat\": 4,\n    \"nbformat_minor\": 2,\n    \"metadata\": {\"z\": 1, \"a\": 2},\n    \"cells\": [\n        {\n            \"source\": \"x = '\\u00e9'\",\n            \"cell_type\": \"code\",\n            \"metadata\": {},\n            \"outputs\": [],\n            \"execution_count\": null\n        }\n    ]\n}";
        let mut nb = parse(text).unwrap();
        assert!(!nb.has_ids);
        assert_eq!(nb.cells[0].source, "x = 'é'");
        assert_eq!(nb.cells[0].id, "cell-0");
        nb.cells[0].source = "x = 'ü'".into();
        let out = serialize(&nb, Some(text));
        assert!(!out.ends_with('\n'));
        assert!(out.starts_with("{\n    \"nbformat\": 4,\n    \"nbformat_minor\": 2,\n    \"metadata\": {\n        \"z\": 1,"), "{out}");
        assert!(out.contains("\"source\": \"x = '\\u00fc'\""), "{out}");
        assert!(!out.contains("\"id\""), "{out}");
    }

    #[test]
    fn empty_and_old_notebooks() {
        let nb = parse("").unwrap();
        assert!(nb.cells.is_empty() && nb.has_ids);
        let text = serialize(&nb, None);
        assert!(
            text.starts_with("{\n \"cells\": [],\n \"metadata\": {\n  \"kernelspec\""),
            "{text}"
        );
        assert!(parse(r#"{"nbformat": 3, "worksheets": []}"#).is_err());
        assert!(parse("[1]").is_err());
        assert!(parse("{").is_err());
    }
}
