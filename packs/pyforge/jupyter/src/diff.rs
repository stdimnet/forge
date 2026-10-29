//! Notebook diffs: two `.ipynb` texts compared cell by cell instead of as JSON. Cells are
//! paired by id (nbformat 4.5+) or, without ids, by an alignment of their sources where
//! neighbouring removed and added cells count as one changed cell. Sources and outputs (as
//! text, see [`crate::output::diff_text`]) get separate line diffs; metadata, cell type and
//! execution count changes are flagged.

use serde::{Deserialize, Serialize};
use similar::{capture_diff_slices, Algorithm, ChangeTag, DiffOp, TextDiff};

use crate::nbformat::{parse, Cell, CellType, Notebook, NotebookError};
use crate::output::diff_text;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineKind {
    Equal,
    Insert,
    Delete,
}

/// A line of a unified diff with its line numbers (1-based) on each side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub kind: LineKind,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<usize>,
}

/// A line diff of two texts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TextChange {
    pub changed: bool,
    pub inserted: usize,
    pub deleted: usize,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CellStatus {
    Unchanged,
    Modified,
    Added,
    Removed,
}

/// One side of a cell pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellSide {
    /// Position in its notebook (0-based).
    pub index: usize,
    pub id: String,
    pub cell_type: CellType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellDiff {
    pub status: CellStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left: Option<CellSide>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right: Option<CellSide>,
    pub source: TextChange,
    pub outputs: TextChange,
    pub type_changed: bool,
    pub metadata_changed: bool,
    pub execution_count_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiffStats {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
    pub unchanged: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotebookDiff {
    pub cells: Vec<CellDiff>,
    pub metadata_changed: bool,
    pub stats: DiffStats,
    /// Language of the code cells, for highlighting.
    pub language: String,
}

/// Diff two texts line by line.
pub fn text_change(a: &str, b: &str) -> TextChange {
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Patience)
        .diff_lines(a, b);
    let mut out = TextChange::default();
    for c in diff.iter_all_changes() {
        let kind = match c.tag() {
            ChangeTag::Equal => LineKind::Equal,
            ChangeTag::Insert => {
                out.inserted += 1;
                LineKind::Insert
            }
            ChangeTag::Delete => {
                out.deleted += 1;
                LineKind::Delete
            }
        };
        out.lines.push(DiffLine {
            kind,
            text: c.value().trim_end_matches(['\n', '\r']).to_string(),
            left: c.old_index().map(|i| i + 1),
            right: c.new_index().map(|i| i + 1),
        });
    }
    out.changed = out.inserted + out.deleted > 0;
    out
}

fn outputs_text(c: Option<&Cell>) -> String {
    let Some(c) = c else { return String::new() };
    let mut s = String::new();
    for o in &c.outputs {
        let t = diff_text(o);
        s.push_str(&t);
        if !t.ends_with('\n') {
            s.push('\n');
        }
    }
    s
}

fn side(c: &Cell, index: usize) -> CellSide {
    CellSide {
        index,
        id: c.id.clone(),
        cell_type: c.cell_type,
        execution_count: c.execution_count,
    }
}

fn pair(left: Option<(usize, &Cell)>, right: Option<(usize, &Cell)>) -> CellDiff {
    let l = left.map(|(_, c)| c);
    let r = right.map(|(_, c)| c);
    let source = text_change(
        l.map_or("", |c| c.source.as_str()),
        r.map_or("", |c| c.source.as_str()),
    );
    let outputs = text_change(&outputs_text(l), &outputs_text(r));
    let type_changed = matches!((l, r), (Some(a), Some(b)) if a.cell_type != b.cell_type);
    let metadata_changed = matches!((l, r), (Some(a), Some(b)) if a.metadata != b.metadata);
    let execution_count_changed =
        matches!((l, r), (Some(a), Some(b)) if a.execution_count != b.execution_count);
    let status = match (l, r) {
        (Some(_), None) => CellStatus::Removed,
        (None, Some(_)) => CellStatus::Added,
        _ if source.changed || outputs.changed || type_changed || metadata_changed => {
            CellStatus::Modified
        }
        _ => CellStatus::Unchanged,
    };
    CellDiff {
        status,
        left: left.map(|(i, c)| side(c, i)),
        right: right.map(|(i, c)| side(c, i)),
        source,
        outputs,
        type_changed,
        metadata_changed,
        execution_count_changed,
    }
}

/// Compare two notebooks.
pub fn diff_notebooks(a: &Notebook, b: &Notebook) -> NotebookDiff {
    let by_id = a.has_ids && b.has_ids;
    let keys = |nb: &Notebook| -> Vec<String> {
        nb.cells
            .iter()
            .map(|c| {
                if by_id {
                    c.id.clone()
                } else {
                    format!("{}\u{0}{}", c.cell_type.as_str(), c.source)
                }
            })
            .collect()
    };
    let (ka, kb) = (keys(a), keys(b));
    let ops = capture_diff_slices(Algorithm::Patience, &ka, &kb);
    let mut cells = Vec::new();
    for op in ops {
        match op {
            DiffOp::Equal {
                old_index,
                new_index,
                len,
            } => {
                for k in 0..len {
                    let (i, j) = (old_index + k, new_index + k);
                    cells.push(pair(Some((i, &a.cells[i])), Some((j, &b.cells[j]))));
                }
            }
            DiffOp::Delete {
                old_index, old_len, ..
            } => {
                for i in old_index..old_index + old_len {
                    cells.push(pair(Some((i, &a.cells[i])), None));
                }
            }
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                for j in new_index..new_index + new_len {
                    cells.push(pair(None, Some((j, &b.cells[j]))));
                }
            }
            DiffOp::Replace {
                old_index,
                old_len,
                new_index,
                new_len,
            } => {
                // Without ids, removed and added cells side by side are changed cells.
                let paired = if by_id { 0 } else { old_len.min(new_len) };
                for k in 0..paired {
                    let (i, j) = (old_index + k, new_index + k);
                    cells.push(pair(Some((i, &a.cells[i])), Some((j, &b.cells[j]))));
                }
                for i in old_index + paired..old_index + old_len {
                    cells.push(pair(Some((i, &a.cells[i])), None));
                }
                for j in new_index + paired..new_index + new_len {
                    cells.push(pair(None, Some((j, &b.cells[j]))));
                }
            }
        }
    }
    let mut stats = DiffStats::default();
    for c in &cells {
        match c.status {
            CellStatus::Added => stats.added += 1,
            CellStatus::Removed => stats.removed += 1,
            CellStatus::Modified => stats.modified += 1,
            CellStatus::Unchanged => stats.unchanged += 1,
        }
    }
    NotebookDiff {
        cells,
        metadata_changed: a.metadata != b.metadata,
        stats,
        language: b.language.clone(),
    }
}

/// Compare two `.ipynb` texts (an empty or missing side is an empty notebook).
pub fn diff_texts(left: &str, right: &str) -> Result<NotebookDiff, NotebookError> {
    Ok(diff_notebooks(&parse(left)?, &parse(right)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nb(cells: &[(&str, &str, &str)], ids: bool) -> String {
        let cells: Vec<String> = cells
            .iter()
            .map(|(id, kind, src)| {
                let id = if ids {
                    format!("\"id\": \"{id}\", ")
                } else {
                    String::new()
                };
                let extra = if *kind == "code" {
                    ", \"outputs\": [], \"execution_count\": null"
                } else {
                    ""
                };
                format!(
                    "{{{id}\"cell_type\": \"{kind}\", \"metadata\": {{}}, \"source\": {}{extra}}}",
                    serde_json::to_string(src).unwrap()
                )
            })
            .collect();
        format!(
            "{{\"cells\": [{}], \"metadata\": {{}}, \"nbformat\": 4, \"nbformat_minor\": {}}}",
            cells.join(","),
            if ids { 5 } else { 4 }
        )
    }

    #[test]
    fn pairs_cells_by_id() {
        let a = nb(
            &[
                ("a", "markdown", "# T"),
                ("b", "code", "x = 1\ny = 2"),
                ("c", "code", "gone"),
            ],
            true,
        );
        let b = nb(
            &[
                ("a", "markdown", "# T"),
                ("b", "code", "x = 1\ny = 3"),
                ("d", "code", "new"),
            ],
            true,
        );
        let d = diff_texts(&a, &b).unwrap();
        let st: Vec<CellStatus> = d.cells.iter().map(|c| c.status).collect();
        assert_eq!(
            st,
            [
                CellStatus::Unchanged,
                CellStatus::Modified,
                CellStatus::Removed,
                CellStatus::Added
            ]
        );
        let src = &d.cells[1].source;
        assert_eq!((src.inserted, src.deleted), (1, 1));
        assert_eq!(
            src.lines[0],
            DiffLine {
                kind: LineKind::Equal,
                text: "x = 1".into(),
                left: Some(1),
                right: Some(1)
            }
        );
        assert_eq!(
            d.stats,
            DiffStats {
                added: 1,
                removed: 1,
                modified: 1,
                unchanged: 1
            }
        );
        assert!(!d.metadata_changed);
    }

    #[test]
    fn pairs_cells_without_ids_by_position_in_a_change() {
        let a = nb(
            &[
                ("", "code", "import pandas"),
                ("", "code", "df.head()"),
                ("", "markdown", "end"),
            ],
            false,
        );
        let b = nb(
            &[
                ("", "code", "import pandas"),
                ("", "code", "df.head(10)"),
                ("", "markdown", "end"),
            ],
            false,
        );
        let d = diff_texts(&a, &b).unwrap();
        let st: Vec<CellStatus> = d.cells.iter().map(|c| c.status).collect();
        assert_eq!(
            st,
            [
                CellStatus::Unchanged,
                CellStatus::Modified,
                CellStatus::Unchanged
            ]
        );
    }

    #[test]
    fn compares_outputs_separately() {
        let with_out = |text: &str| {
            format!(
                "{{\"cells\": [{{\"id\": \"a\", \"cell_type\": \"code\", \"metadata\": {{}}, \"source\": \"print(1)\", \"execution_count\": 1, \"outputs\": [{{\"output_type\": \"stream\", \"name\": \"stdout\", \"text\": \"{text}\"}}]}}], \"metadata\": {{}}, \"nbformat\": 4, \"nbformat_minor\": 5}}"
            )
        };
        let d = diff_texts(&with_out("1\\n"), &with_out("2\\n")).unwrap();
        let c = &d.cells[0];
        assert_eq!(c.status, CellStatus::Modified);
        assert!(!c.source.changed && c.outputs.changed);
        assert_eq!(c.outputs.lines[0].text, "[stdout]");
        let d = diff_texts("", &with_out("1\\n")).unwrap();
        assert_eq!(d.cells[0].status, CellStatus::Added);
    }
}
