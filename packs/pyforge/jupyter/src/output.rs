//! Cell outputs: the nbformat JSON of `stream`, `execute_result`, `display_data` and `error`
//! outputs, and what the editor shows for them ([`render`]): styled console lines for text
//! and tracebacks (ANSI colors, `\r` progress bars, file links), sanitized HTML for a
//! sandboxed frame, images as data URLs (SVG sanitized), JSON trees and DataFrame tables.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use forge_output::ansi::{AnsiParser, Style};
use forge_output::console::{ConsoleLine, ConsoleModel};
use forge_output::links::LinkContext;

use crate::dataframe::{self, DataFrame};
use crate::html;
use crate::nbformat::{join_text, split_lines};

/// MIME type → value. Texts are joined; JSON types (`application/json`, `*+json`) keep
/// their JSON value.
pub type MimeBundle = Map<String, Value>;

/// A cell output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "outputType",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum Output {
    Stream {
        /// `stdout` or `stderr`.
        name: String,
        text: String,
    },
    ExecuteResult {
        execution_count: Option<u64>,
        data: MimeBundle,
        #[serde(default = "empty_object")]
        metadata: Value,
    },
    DisplayData {
        data: MimeBundle,
        #[serde(default = "empty_object")]
        metadata: Value,
    },
    Error {
        ename: String,
        evalue: String,
        traceback: Vec<String>,
    },
}

fn empty_object() -> Value {
    Value::Object(Map::new())
}

/// JSON MIME types keep their value; others are texts.
pub fn is_json_mime(mime: &str) -> bool {
    mime == "application/json" || mime.ends_with("+json")
}

/// Text MIME types are stored as lists of lines; binary ones (base64) as one string.
fn is_split_mime(mime: &str) -> bool {
    mime.starts_with("text/") || mime == "image/svg+xml" || mime == "application/javascript"
}

fn bundle_from_json(v: Option<&Value>) -> MimeBundle {
    let mut out = MimeBundle::new();
    if let Some(Value::Object(m)) = v {
        for (k, v) in m {
            let value = if is_json_mime(k) {
                v.clone()
            } else {
                Value::String(join_text(v))
            };
            out.insert(k.clone(), value);
        }
    }
    out
}

fn bundle_to_json(bundle: &MimeBundle, original: Option<&Value>) -> Value {
    let mut out = Map::new();
    let mut keys: Vec<&String> = bundle.keys().collect();
    if original.is_none() {
        keys.sort();
    }
    for k in keys {
        let v = &bundle[k];
        let orig = original.and_then(|o| o.get(k));
        let value = match v {
            Value::String(s) if !is_json_mime(k) => match orig {
                Some(Value::String(_)) => v.clone(),
                Some(Value::Array(_)) => Value::Array(split_lines(s)),
                _ if is_split_mime(k) => Value::Array(split_lines(s)),
                _ => v.clone(),
            },
            _ => v.clone(),
        };
        out.insert(k.clone(), value);
    }
    Value::Object(out)
}

/// An output from its nbformat JSON. Unknown output types become an empty stdout stream.
pub fn output_from_json(v: &Value) -> Output {
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match v.get("output_type").and_then(Value::as_str).unwrap_or("") {
        "stream" => Output::Stream {
            name: v
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("stdout")
                .to_string(),
            text: v.get("text").map(join_text).unwrap_or_default(),
        },
        "execute_result" | "pyout" => Output::ExecuteResult {
            execution_count: v.get("execution_count").and_then(Value::as_u64),
            data: bundle_from_json(v.get("data")),
            metadata: v.get("metadata").cloned().unwrap_or_else(empty_object),
        },
        "display_data" => Output::DisplayData {
            data: bundle_from_json(v.get("data")),
            metadata: v.get("metadata").cloned().unwrap_or_else(empty_object),
        },
        "error" | "pyerr" => Output::Error {
            ename: s("ename"),
            evalue: s("evalue"),
            traceback: v
                .get("traceback")
                .and_then(Value::as_array)
                .map(|t| {
                    t.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        },
        _ => Output::Stream {
            name: "stdout".into(),
            text: String::new(),
        },
    }
}

/// nbformat JSON of an output, in the style of `original` (the output it replaces) when
/// given, else as Jupyter writes it.
pub fn output_to_json(o: &Output, original: Option<&Value>) -> Value {
    let orig_text = |k: &str| original.and_then(|v| v.get(k));
    let text = |t: &str, orig: Option<&Value>| match orig {
        Some(Value::String(_)) => Value::String(t.to_string()),
        _ => Value::Array(split_lines(t)),
    };
    let mut m = Map::new();
    match o {
        Output::Stream { name, text: t } => {
            m.insert("name".into(), Value::from(name.as_str()));
            m.insert("output_type".into(), Value::from("stream"));
            m.insert("text".into(), text(t, orig_text("text")));
        }
        Output::ExecuteResult {
            execution_count,
            data,
            metadata,
        } => {
            m.insert("data".into(), bundle_to_json(data, orig_text("data")));
            m.insert(
                "execution_count".into(),
                execution_count.map_or(Value::Null, Value::from),
            );
            m.insert("metadata".into(), metadata.clone());
            m.insert("output_type".into(), Value::from("execute_result"));
        }
        Output::DisplayData { data, metadata } => {
            m.insert("data".into(), bundle_to_json(data, orig_text("data")));
            m.insert("metadata".into(), metadata.clone());
            m.insert("output_type".into(), Value::from("display_data"));
        }
        Output::Error {
            ename,
            evalue,
            traceback,
        } => {
            m.insert("ename".into(), Value::from(ename.as_str()));
            m.insert("evalue".into(), Value::from(evalue.as_str()));
            m.insert("output_type".into(), Value::from("error"));
            m.insert(
                "traceback".into(),
                Value::Array(traceback.iter().map(|t| Value::from(t.as_str())).collect()),
            );
        }
    }
    Value::Object(m)
}

/// What the editor shows for an output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Rendered {
    /// Stream text, `text/plain` results and tracebacks as console lines.
    Text {
        lines: Vec<ConsoleLine>,
        /// `stderr` streams and errors are shown with an error background.
        error: bool,
    },
    /// Sanitized HTML for a sandboxed frame (no scripts, no network).
    Html {
        html: String,
    },
    /// A data URL (`data:image/png;base64,…`); SVG is sanitized first.
    Image {
        src: String,
        mime: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        width: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        height: Option<u64>,
    },
    Json {
        value: Value,
    },
    Markdown {
        text: String,
    },
    /// A pandas DataFrame (from its HTML or `application/vnd.dataresource+json`).
    DataFrame {
        frame: DataFrame,
    },
}

/// Lines of a text with ANSI colors, `\r` overwrites and file links.
pub fn text_lines(text: &str, root: Option<&str>) -> Vec<ConsoleLine> {
    text_lines_styled(text, root, None)
}

fn text_lines_styled(text: &str, root: Option<&str>, base: Option<&Style>) -> Vec<ConsoleLine> {
    let ctx = LinkContext {
        root: root.map(str::to_string),
        ..LinkContext::default()
    };
    let segments = AnsiParser::new().feed(text);
    let mut model = ConsoleModel::new(ctx);
    let delta = model.append(&segments, base);
    let mut lines = Vec::new();
    delta.apply_to(&mut lines);
    // A trailing line break leaves an empty last line.
    if lines.last().is_some_and(|l| l.segments.is_empty()) && text.ends_with('\n') {
        lines.pop();
    }
    lines
}

/// The preferred representation of an output, like JupyterLab's MIME ranking.
pub fn render(o: &Output, root: Option<&str>) -> Rendered {
    match o {
        Output::Stream { name, text } => Rendered::Text {
            lines: text_lines(text, root),
            error: name == "stderr",
        },
        Output::Error {
            ename,
            evalue,
            traceback,
        } => {
            let text = if traceback.is_empty() {
                format!("{ename}: {evalue}")
            } else {
                traceback.join("\n")
            };
            Rendered::Text {
                lines: text_lines(&text, root),
                error: true,
            }
        }
        Output::ExecuteResult { data, metadata, .. } | Output::DisplayData { data, metadata } => {
            render_bundle(data, metadata, root)
        }
    }
}

/// The best representation of a MIME bundle.
pub fn render_bundle(data: &MimeBundle, metadata: &Value, root: Option<&str>) -> Rendered {
    let text = |k: &str| data.get(k).and_then(Value::as_str);
    if let Some(v) = data.get("application/vnd.dataresource+json") {
        if let Some(frame) = dataframe::from_dataresource(v) {
            return Rendered::DataFrame { frame };
        }
    }
    if let Some(h) = text("text/html") {
        if let Some(frame) = dataframe::from_html(h) {
            return Rendered::DataFrame { frame };
        }
    }
    for mime in ["image/png", "image/jpeg", "image/gif", "image/webp"] {
        if let Some(b64) = text(mime) {
            let size = |k: &str| {
                metadata
                    .get(mime)
                    .and_then(|m| m.get(k))
                    .and_then(Value::as_u64)
            };
            return Rendered::Image {
                src: format!(
                    "data:{mime};base64,{}",
                    b64.split_whitespace().collect::<String>()
                ),
                mime: mime.into(),
                width: size("width"),
                height: size("height"),
            };
        }
    }
    if let Some(svg) = text("image/svg+xml") {
        use base64::Engine;
        let clean = html::sanitize_svg(svg);
        return Rendered::Image {
            src: format!(
                "data:image/svg+xml;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(clean)
            ),
            mime: "image/svg+xml".into(),
            width: None,
            height: None,
        };
    }
    if let Some(h) = text("text/html") {
        return Rendered::Html {
            html: html::sanitize_html(h),
        };
    }
    if let Some(v) = data.get("application/json") {
        return Rendered::Json { value: v.clone() };
    }
    if let Some(md) = text("text/markdown") {
        return Rendered::Markdown { text: md.into() };
    }
    if let Some(t) = text("text/plain") {
        return Rendered::Text {
            lines: text_lines(t, root),
            error: false,
        };
    }
    if let Some(t) = text("text/latex") {
        return Rendered::Text {
            lines: text_lines(t, root),
            error: false,
        };
    }
    let kinds: Vec<&str> = data.keys().map(String::as_str).collect();
    Rendered::Text {
        lines: text_lines_styled(
            &format!("<no renderer for {}>", kinds.join(", ")),
            None,
            Some(&Style::fg("var(--fg-muted)")),
        ),
        error: false,
    }
}

/// The text a notebook diff compares for an output: texts as they are, images as a
/// placeholder with a short hash, tracebacks without colors.
pub fn diff_text(o: &Output) -> String {
    let plain = |t: &str| -> String {
        AnsiParser::new()
            .feed(t)
            .into_iter()
            .map(|s| s.text)
            .collect()
    };
    match o {
        Output::Stream { name, text } => format!("[{name}]\n{}", plain(text)),
        Output::Error {
            ename,
            evalue,
            traceback,
        } => {
            let tb = if traceback.is_empty() {
                format!("{ename}: {evalue}")
            } else {
                plain(&traceback.join("\n"))
            };
            format!("[error] {ename}\n{tb}")
        }
        Output::ExecuteResult {
            execution_count,
            data,
            ..
        } => {
            let head = match execution_count {
                Some(n) => format!("[Out {n}]"),
                None => "[Out]".to_string(),
            };
            format!("{head}\n{}", bundle_text(data))
        }
        Output::DisplayData { data, .. } => format!("[display]\n{}", bundle_text(data)),
    }
}

fn bundle_text(data: &MimeBundle) -> String {
    let mut parts = Vec::new();
    for (k, v) in data {
        if k.starts_with("image/") || is_json_mime(k) {
            let body = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            parts.push(format!(
                "<{k} {} bytes, #{}>",
                body.len(),
                short_hash(&body)
            ));
        }
    }
    let text = ["text/plain", "text/markdown", "text/html"]
        .iter()
        .find_map(|k| data.get(*k).and_then(Value::as_str));
    if let Some(t) = text {
        parts.push(t.to_string());
    }
    parts.join("\n")
}

/// FNV-1a, enough to tell changed images apart in a diff.
fn short_hash(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:08x}", h >> 32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_and_writes_every_output_type() {
        for raw in [
            json!({"name": "stderr", "output_type": "stream", "text": ["a\n", "b"]}),
            json!({"data": {"text/plain": ["1"]}, "execution_count": 4, "metadata": {}, "output_type": "execute_result"}),
            json!({"data": {"application/json": {"a": 1}, "image/png": "iVBOR\n", "text/plain": ["<Figure>"]}, "metadata": {"image/png": {"width": 10}}, "output_type": "display_data"}),
            json!({"ename": "ValueError", "evalue": "bad", "output_type": "error", "traceback": ["\u{1b}[0;31mValueError\u{1b}[0m: bad"]}),
        ] {
            let o = output_from_json(&raw);
            assert_eq!(output_to_json(&o, None), raw);
            let s = serde_json::to_value(&o).unwrap();
            assert_eq!(serde_json::from_value::<Output>(s).unwrap(), o);
        }
    }

    #[test]
    fn serializes_for_the_ui_in_camel_case() {
        let o = output_from_json(
            &json!({"data": {}, "execution_count": 1, "output_type": "execute_result"}),
        );
        assert_eq!(
            serde_json::to_value(&o).unwrap(),
            json!({"outputType": "execute_result", "executionCount": 1, "data": {}, "metadata": {}})
        );
    }

    #[test]
    fn renders_by_mime_priority() {
        let o = |data: Value| Output::DisplayData {
            data: data.as_object().unwrap().clone(),
            metadata: json!({}),
        };
        assert!(
            matches!(render(&o(json!({"text/plain": "x", "image/png": "AAA"})), None), Rendered::Image { mime, .. } if mime == "image/png")
        );
        match render(
            &o(json!({"image/svg+xml": "<svg onload=\"x()\"><script>x()</script><rect/></svg>"})),
            None,
        ) {
            Rendered::Image { src, .. } => {
                use base64::Engine;
                let b = src.trim_start_matches("data:image/svg+xml;base64,");
                let svg =
                    String::from_utf8(base64::engine::general_purpose::STANDARD.decode(b).unwrap())
                        .unwrap();
                assert_eq!(svg, "<svg><rect/></svg>");
            }
            other => panic!("{other:?}"),
        }
        assert!(
            matches!(render(&o(json!({"text/html": "<b onclick='x'>hi</b>"})), None), Rendered::Html { html } if html == "<b>hi</b>")
        );
        assert!(matches!(
            render(&o(json!({"application/json": {"a": [1]}})), None),
            Rendered::Json { .. }
        ));
        assert!(matches!(
            render(&o(json!({"text/markdown": "# x"})), None),
            Rendered::Markdown { .. }
        ));
    }

    #[test]
    fn renders_ansi_text_and_errors() {
        let r = render(
            &Output::Error {
                ename: "ZeroDivisionError".into(),
                evalue: "division by zero".into(),
                traceback: vec![
                    "\u{1b}[0;31m---\u{1b}[0m".into(),
                    "\u{1b}[1;31mZeroDivisionError\u{1b}[0m: division by zero".into(),
                ],
            },
            None,
        );
        match r {
            Rendered::Text { lines, error } => {
                assert!(error);
                assert_eq!(lines.len(), 2);
                assert_eq!(lines[1].text(), "ZeroDivisionError: division by zero");
                assert!(lines[1].segments[0].style.bold);
            }
            other => panic!("{other:?}"),
        }
        let lines = text_lines("10%\r50%\r100%\ndone\n", None);
        assert_eq!(
            lines.iter().map(ConsoleLine::text).collect::<Vec<_>>(),
            ["100%", "done"]
        );
    }

    #[test]
    fn diff_texts() {
        let img = Output::DisplayData {
            data: json!({"image/png": "AAAA", "text/plain": "<Figure size 640x480>"})
                .as_object()
                .unwrap()
                .clone(),
            metadata: json!({}),
        };
        let t = diff_text(&img);
        assert!(
            t.starts_with("[display]\n<image/png 4 bytes, #")
                && t.ends_with(">\n<Figure size 640x480>"),
            "{t}"
        );
        assert_eq!(
            diff_text(&Output::Stream {
                name: "stdout".into(),
                text: "\u{1b}[32mok\u{1b}[0m\n".into()
            }),
            "[stdout]\nok\n"
        );
    }
}
