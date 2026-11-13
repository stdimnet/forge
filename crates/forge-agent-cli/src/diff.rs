//! Line diffs of edit tool calls (`Edit`, `MultiEdit`, `Write`) for the chat's edit cards
//! and permission previews.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use similar::{capture_diff_slices, Algorithm, DiffOp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    Ctx,
    Add,
    Del,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    #[serde(rename = "type")]
    pub kind: LineKind,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_no: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_no: Option<usize>,
    /// Distance in lines to the nearest changed line (0 for changes), so the UI can show a
    /// given amount of context around changes.
    pub near: usize,
}

/// The diff of one edit tool call.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditDiff {
    /// One line list per old/new text pair.
    pub hunks: Vec<Vec<DiffLine>>,
    pub added: usize,
    pub removed: usize,
}

fn lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        Vec::new()
    } else {
        text.split('\n').collect()
    }
}

/// Line diff; inside a changed region added lines come before removed ones.
pub fn diff_lines(old: &str, new: &str) -> Vec<DiffLine> {
    let a = lines(old);
    let b = lines(new);
    let line = |kind, text: &str, old_no, new_no| DiffLine {
        kind,
        text: text.to_string(),
        old_no,
        new_no,
        near: 0,
    };
    let del = |k: usize| line(LineKind::Del, a[k], Some(k + 1), None);
    let add = |k: usize| line(LineKind::Add, b[k], None, Some(k + 1));
    let mut out = Vec::new();
    let mut adds = Vec::new();
    let mut dels = Vec::new();
    for op in capture_diff_slices(Algorithm::Myers, &a, &b) {
        match op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                out.append(&mut adds);
                out.append(&mut dels);
                out.extend((0..len).map(|k| {
                    line(
                        LineKind::Ctx,
                        a[old_index + k],
                        Some(old_index + k + 1),
                        Some(new_index + k + 1),
                    )
                }));
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => dels.extend((old_index..old_index + old_len).map(del)),
            DiffOp::Insert {
                new_index, new_len, ..
            } => adds.extend((new_index..new_index + new_len).map(add)),
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                dels.extend((old_index..old_index + old_len).map(del));
                adds.extend((new_index..new_index + new_len).map(add));
            }
        }
    }
    out.append(&mut adds);
    out.append(&mut dels);
    // Distance to the nearest change, forwards and backwards.
    let mut last: Option<usize> = None;
    for (i, l) in out.iter_mut().enumerate() {
        if l.kind != LineKind::Ctx {
            last = Some(i);
        }
        // Without any change every line is "far" (more than the whole diff away).
        l.near = last.map_or(usize::MAX, |c| i - c);
    }
    last = None;
    for (i, l) in out.iter_mut().enumerate().rev() {
        if l.kind != LineKind::Ctx {
            last = Some(i);
        }
        if let Some(c) = last {
            l.near = l.near.min(c - i);
        }
    }
    let far = out.len();
    for l in &mut out {
        l.near = l.near.min(far);
    }
    out
}

fn s(v: Option<&Value>) -> &str {
    v.and_then(Value::as_str).unwrap_or_default()
}

/// The old/new text pairs an edit tool call describes.
pub fn edit_hunks(name: &str, input: &Value) -> Vec<(String, String)> {
    match name {
        "Write" | "write_file" => vec![(String::new(), s(input.get("content")).to_string())],
        "MultiEdit" if input.get("edits").is_some_and(Value::is_array) => input["edits"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| {
                (
                    s(e.get("old_string")).to_string(),
                    s(e.get("new_string")).to_string(),
                )
            })
            .collect(),
        _ => vec![(
            s(input.get("old_string")).to_string(),
            s(input.get("new_string")).to_string(),
        )],
    }
}

/// The diff an edit tool call would make.
pub fn edit_diff(name: &str, input: &Value) -> EditDiff {
    let hunks: Vec<Vec<DiffLine>> = edit_hunks(name, input)
        .iter()
        .map(|(o, n)| diff_lines(o, n))
        .collect();
    with_counts(hunks)
}

/// The diff between two whole texts (one hunk), e.g. an AI proposal's preview.
pub fn text_diff(old: &str, new: &str) -> EditDiff {
    with_counts(vec![diff_lines(old, new)])
}

fn with_counts(hunks: Vec<Vec<DiffLine>>) -> EditDiff {
    let count = |k| hunks.iter().flatten().filter(|l| l.kind == k).count();
    EditDiff {
        added: count(LineKind::Add),
        removed: count(LineKind::Del),
        hunks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn diffs_lines_with_additions_first() {
        let lines = diff_lines("a\nb\nc", "a\nB\nc\nd");
        let got: Vec<String> = lines
            .iter()
            .map(|l| format!("{:?}:{}", l.kind, l.text))
            .collect();
        assert_eq!(got, ["Ctx:a", "Add:B", "Del:b", "Ctx:c", "Add:d"]);
        assert_eq!(
            lines.iter().map(|l| l.near).collect::<Vec<_>>(),
            [1, 0, 0, 1, 0]
        );
        let d = edit_diff(
            "Edit",
            &json!({ "old_string": "a\nb\nc", "new_string": "a\nB\nc\nd" }),
        );
        assert_eq!((d.added, d.removed), (2, 1));
        assert_eq!(text_diff("a\nb\nc", "a\nB\nc\nd"), d);
    }

    #[test]
    fn extracts_hunks_from_edit_tools() {
        assert_eq!(
            edit_hunks("Write", &json!({ "content": "x" })),
            [(String::new(), "x".to_string())]
        );
        assert_eq!(
            edit_hunks(
                "MultiEdit",
                &json!({ "edits": [{ "old_string": "a", "new_string": "b" }] })
            ),
            [("a".to_string(), "b".to_string())]
        );
    }
}
