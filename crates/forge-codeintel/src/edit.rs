//! Text edits and workspace edits with LSP positions.
//!
//! A [`WorkspaceEdit`] is an ordered list of [`DocumentChange`]s (text edits of one file,
//! file creation, rename or deletion), the shape of LSP `documentChanges`. Applying one is
//! atomic: every new file content is computed first and nothing is written when any edit
//! does not fit. The returned [`AppliedFile`]s are what "Undo" needs to restore the files.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// 0-based line and UTF-16 character, like LSP and CodeMirror.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl Position {
    pub const fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// A range on one line.
    pub const fn on_line(line: u32, start: u32, end: u32) -> Self {
        Self::new(Position::new(line, start), Position::new(line, end))
    }

    pub fn contains(&self, p: Position) -> bool {
        self.start <= p && p <= self.end
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}

/// One step of a workspace edit. Paths are workspace-relative (`/`-separated) once they
/// reach the app; the language server client produces absolute ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DocumentChange {
    Edit {
        path: String,
        edits: Vec<TextEdit>,
    },
    Create {
        path: String,
        #[serde(default)]
        overwrite: bool,
    },
    Rename {
        from: String,
        to: String,
    },
    Delete {
        path: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEdit {
    #[serde(default)]
    pub changes: Vec<DocumentChange>,
}

impl WorkspaceEdit {
    pub fn is_empty(&self) -> bool {
        self.changes.iter().all(|c| match c {
            DocumentChange::Edit { edits, .. } => edits.is_empty(),
            _ => false,
        })
    }

    /// Add text edits of `path`, merged into an earlier step for the same file when no file
    /// operation came in between (servers sometimes list a document twice).
    pub fn add_edits(&mut self, path: impl Into<String>, edits: Vec<TextEdit>) {
        let path = path.into();
        if let Some(DocumentChange::Edit { path: p, edits: e }) = self.changes.last_mut() {
            if *p == path {
                e.extend(edits);
                return;
            }
        }
        if let Some(DocumentChange::Edit { edits: e, .. }) = self
            .changes
            .iter_mut()
            .rev()
            .take_while(|c| matches!(c, DocumentChange::Edit { .. }))
            .find(|c| matches!(c, DocumentChange::Edit { path: p, .. } if *p == path))
        {
            e.extend(edits);
            return;
        }
        self.changes.push(DocumentChange::Edit { path, edits });
    }

    /// Every path the edit touches, in order of first appearance.
    pub fn paths(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut push = |p: &String| {
            if !out.contains(p) {
                out.push(p.clone());
            }
        };
        for c in &self.changes {
            match c {
                DocumentChange::Edit { path, .. }
                | DocumentChange::Create { path, .. }
                | DocumentChange::Delete { path } => push(path),
                DocumentChange::Rename { from, to } => {
                    push(from);
                    push(to);
                }
            }
        }
        out
    }

    /// Number of text edits.
    pub fn edit_count(&self) -> usize {
        self.changes
            .iter()
            .map(|c| match c {
                DocumentChange::Edit { edits, .. } => edits.len(),
                _ => 0,
            })
            .sum()
    }

    /// Rewrite every path (absolute → workspace-relative and back).
    pub fn map_paths(&mut self, mut f: impl FnMut(&str) -> String) {
        for c in &mut self.changes {
            match c {
                DocumentChange::Edit { path, .. }
                | DocumentChange::Create { path, .. }
                | DocumentChange::Delete { path } => *path = f(path),
                DocumentChange::Rename { from, to } => {
                    *from = f(from);
                    *to = f(to);
                }
            }
        }
    }

    /// The text edits of `path` only (the open editor applies these itself).
    pub fn edits_of(&self, path: &str) -> Vec<TextEdit> {
        self.changes
            .iter()
            .filter_map(|c| match c {
                DocumentChange::Edit { path: p, edits } if p == path => Some(edits.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// The edit without the text edits of `paths` (files the UI edits in open editors).
    pub fn without_text_edits_of(&self, paths: &[String]) -> WorkspaceEdit {
        WorkspaceEdit {
            changes: self
                .changes
                .iter()
                .filter(|c| !matches!(c, DocumentChange::Edit { path, .. } if paths.contains(path)))
                .cloned()
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    /// Two edits of one file overlap.
    Overlap { path: String },
    /// The edit names a file that does not exist.
    Missing { path: String },
    /// The file changed since the edit was computed or applied.
    Conflict { path: String },
    /// Reading or writing failed.
    Io { path: String, message: String },
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::Overlap { path } => write!(f, "overlapping edits in {path}"),
            EditError::Missing { path } => write!(f, "{path} does not exist"),
            EditError::Conflict { path } => write!(f, "{path} was changed in the meantime"),
            EditError::Io { path, message } => write!(f, "{path}: {message}"),
        }
    }
}

impl std::error::Error for EditError {}

/// Byte offset of every line start. Lines end at `\n`; a `\r` before it is part of the
/// line break, so positions never land between `\r` and `\n`.
fn line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    starts
}

/// Byte offset of an LSP position, clamped to the line (and the document).
fn offset_of(text: &str, starts: &[usize], p: Position) -> usize {
    let Some(&start) = starts.get(p.line as usize) else {
        return text.len();
    };
    let end = starts
        .get(p.line as usize + 1)
        .map(|&next| {
            let mut e = next - 1; // the `\n`
            if e > start && text.as_bytes()[e - 1] == b'\r' {
                e -= 1;
            }
            e
        })
        .unwrap_or(text.len());
    let mut units = 0u32;
    for (i, c) in text[start..end].char_indices() {
        if units >= p.character {
            return start + i;
        }
        units += c.len_utf16() as u32;
    }
    end
}

/// Apply LSP text edits to `text`. Edits refer to the original text; inserts at the same
/// position keep their order. `None` when edits overlap.
pub fn apply_text_edits(text: &str, edits: &[TextEdit]) -> Option<String> {
    let starts = line_starts(text);
    let mut spans: Vec<(usize, usize, usize)> = edits
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let a = offset_of(text, &starts, e.range.start);
            let b = offset_of(text, &starts, e.range.end);
            (a.min(b), a.max(b), i)
        })
        .collect();
    spans.sort_by_key(|&(a, b, i)| (a, b, i));
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for (a, b, i) in spans {
        if a < at {
            return None;
        }
        out.push_str(&text[at..a]);
        out.push_str(&edits[i].new_text);
        at = b;
    }
    out.push_str(&text[at..]);
    Some(out)
}

/// Where workspace edits are read from and written to: the project's backend (local folder
/// or remote host) in the app, a map in tests and browser mode.
pub trait FileStore {
    /// `Ok(None)` when the file does not exist.
    fn read(&mut self, path: &str) -> Result<Option<String>, String>;
    fn write(&mut self, path: &str, text: &str) -> Result<(), String>;
    fn delete(&mut self, path: &str) -> Result<(), String>;
}

/// An in-memory [`FileStore`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryStore(pub BTreeMap<String, String>);

impl FileStore for MemoryStore {
    fn read(&mut self, path: &str) -> Result<Option<String>, String> {
        Ok(self.0.get(path).cloned())
    }
    fn write(&mut self, path: &str, text: &str) -> Result<(), String> {
        self.0.insert(path.to_string(), text.to_string());
        Ok(())
    }
    fn delete(&mut self, path: &str) -> Result<(), String> {
        self.0.remove(path);
        Ok(())
    }
}

/// A file changed by [`apply_workspace_edit`]: its content before and after (`None` = the
/// file did not exist / was deleted).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedFile {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

fn io(path: &str, message: String) -> EditError {
    EditError::Io {
        path: path.to_string(),
        message,
    }
}

/// Apply `edit` to the files in `store`, all or nothing. Returns the changed files in order
/// of first appearance.
pub fn apply_workspace_edit(
    edit: &WorkspaceEdit,
    store: &mut dyn FileStore,
) -> Result<Vec<AppliedFile>, EditError> {
    // path -> (original, planned) content
    let mut files: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    fn slot<'a>(
        files: &'a mut Vec<(String, Option<String>, Option<String>)>,
        store: &mut dyn FileStore,
        path: &str,
    ) -> Result<&'a mut (String, Option<String>, Option<String>), EditError> {
        if let Some(i) = files.iter().position(|f| f.0 == path) {
            return Ok(&mut files[i]);
        }
        let original = store.read(path).map_err(|e| io(path, e))?;
        files.push((path.to_string(), original.clone(), original));
        Ok(files.last_mut().expect("just pushed"))
    }

    for change in &edit.changes {
        match change {
            DocumentChange::Edit { path, edits } => {
                let f = slot(&mut files, store, path)?;
                let current =
                    f.2.as_deref()
                        .ok_or_else(|| EditError::Missing { path: path.clone() })?;
                let next = apply_text_edits(current, edits)
                    .ok_or_else(|| EditError::Overlap { path: path.clone() })?;
                f.2 = Some(next);
            }
            DocumentChange::Create { path, overwrite } => {
                let f = slot(&mut files, store, path)?;
                if f.2.is_none() || *overwrite {
                    f.2 = Some(String::new());
                }
            }
            DocumentChange::Rename { from, to } => {
                let content = slot(&mut files, store, from)?
                    .2
                    .take()
                    .ok_or_else(|| EditError::Missing { path: from.clone() })?;
                let target = slot(&mut files, store, to)?;
                if target.2.is_some() {
                    return Err(EditError::Conflict { path: to.clone() });
                }
                target.2 = Some(content);
            }
            DocumentChange::Delete { path } => {
                slot(&mut files, store, path)?.2 = None;
            }
        }
    }

    let changed: Vec<AppliedFile> = files
        .into_iter()
        .filter(|(_, before, after)| before != after)
        .map(|(path, before, after)| AppliedFile {
            path,
            before,
            after,
        })
        .collect();
    write_all(store, changed.iter().map(|f| (&f.path, &f.after)))?;
    Ok(changed)
}

fn write_all<'a>(
    store: &mut dyn FileStore,
    files: impl Iterator<Item = (&'a String, &'a Option<String>)>,
) -> Result<(), EditError> {
    for (path, content) in files {
        match content {
            Some(text) => store.write(path, text),
            None => store.delete(path),
        }
        .map_err(|e| io(path, e))?;
    }
    Ok(())
}

/// Undo [`apply_workspace_edit`]: restore every file, unless one of them was changed since
/// (then nothing is touched).
pub fn revert_applied(applied: &[AppliedFile], store: &mut dyn FileStore) -> Result<(), EditError> {
    for f in applied {
        if store.read(&f.path).map_err(|e| io(&f.path, e))? != f.after {
            return Err(EditError::Conflict {
                path: f.path.clone(),
            });
        }
    }
    write_all(store, applied.iter().map(|f| (&f.path, &f.before)))
}

/// One affected line in a refactoring preview: the line as it is now, with the parts that
/// change marked (UTF-16 offsets into `text`), and the line afterwards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRow {
    pub line: u32,
    pub column: u32,
    /// The current line, leading whitespace removed.
    pub text: String,
    pub ranges: Vec<[u32; 2]>,
    /// The line after the change, leading whitespace removed.
    pub after: String,
}

/// The affected lines of one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePreview {
    pub path: String,
    /// `"edit"`, `"create"`, `"rename"` or `"delete"`.
    pub operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_path: Option<String>,
    pub rows: Vec<PreviewRow>,
}

fn utf16_len(s: &str) -> u32 {
    s.encode_utf16().count() as u32
}

/// The line `n` of `text` without its line break.
fn line_of(text: &str, n: u32) -> &str {
    text.split('\n')
        .nth(n as usize)
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .unwrap_or("")
}

/// Rows for a refactoring preview (Find tool window): every line an edit touches, read
/// through `read` (`None` for files that do not exist yet).
pub fn preview_workspace_edit(
    edit: &WorkspaceEdit,
    mut read: impl FnMut(&str) -> Option<String>,
) -> Vec<FilePreview> {
    let mut out: Vec<FilePreview> = Vec::new();
    for change in &edit.changes {
        match change {
            DocumentChange::Edit { path, edits } => {
                let Some(text) = read(path) else { continue };
                let after_text = apply_text_edits(&text, edits).unwrap_or_else(|| text.clone());
                let mut by_line: BTreeMap<u32, Vec<&TextEdit>> = BTreeMap::new();
                for e in edits {
                    by_line.entry(e.range.start.line).or_default().push(e);
                }
                // Line shift so the "after" line is found in the edited text.
                let mut shift: i64 = 0;
                let mut rows = Vec::new();
                for (line, mut line_edits) in by_line {
                    line_edits.sort_by_key(|e| e.range.start.character);
                    let current = line_of(&text, line);
                    let lead = utf16_len(current) - utf16_len(current.trim_start());
                    let ranges = line_edits
                        .iter()
                        .map(|e| {
                            let end = if e.range.end.line == line {
                                e.range.end.character
                            } else {
                                utf16_len(current)
                            };
                            [
                                e.range.start.character.saturating_sub(lead),
                                end.saturating_sub(lead),
                            ]
                        })
                        .collect();
                    let after_line = (line as i64 + shift).max(0) as u32;
                    rows.push(PreviewRow {
                        line,
                        column: line_edits[0].range.start.character,
                        text: current.trim_start().to_string(),
                        ranges,
                        after: line_of(&after_text, after_line).trim_start().to_string(),
                    });
                    for e in line_edits {
                        let removed = (e.range.end.line - e.range.start.line) as i64;
                        let added = e.new_text.matches('\n').count() as i64;
                        shift += added - removed;
                    }
                }
                match out
                    .iter_mut()
                    .find(|f| f.path == *path && f.operation == "edit")
                {
                    Some(f) => f.rows.extend(rows),
                    None => out.push(FilePreview {
                        path: path.clone(),
                        operation: "edit".into(),
                        new_path: None,
                        rows,
                    }),
                }
            }
            DocumentChange::Create { path, .. } => out.push(FilePreview {
                path: path.clone(),
                operation: "create".into(),
                new_path: None,
                rows: vec![],
            }),
            DocumentChange::Rename { from, to } => out.push(FilePreview {
                path: from.clone(),
                operation: "rename".into(),
                new_path: Some(to.clone()),
                rows: vec![],
            }),
            DocumentChange::Delete { path } => out.push(FilePreview {
                path: path.clone(),
                operation: "delete".into(),
                new_path: None,
                rows: vec![],
            }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(l1: u32, c1: u32, l2: u32, c2: u32, text: &str) -> TextEdit {
        TextEdit {
            range: Range::new(Position::new(l1, c1), Position::new(l2, c2)),
            new_text: text.into(),
        }
    }

    #[test]
    fn applies_edits_with_utf16_positions() {
        let text = "<?php\n$ü = new User();\nUser::find(1);\n";
        let out = apply_text_edits(
            text,
            &[edit(1, 9, 1, 13, "Customer"), edit(2, 0, 2, 4, "Customer")],
        )
        .unwrap();
        assert_eq!(out, "<?php\n$ü = new Customer();\nCustomer::find(1);\n");

        // Surrogate pairs count twice.
        let out = apply_text_edits("a😀b", &[edit(0, 3, 0, 4, "c")]).unwrap();
        assert_eq!(out, "a😀c");
    }

    #[test]
    fn keeps_insert_order_and_refuses_overlaps() {
        let out = apply_text_edits("ab", &[edit(0, 1, 0, 1, "1"), edit(0, 1, 0, 1, "2")]).unwrap();
        assert_eq!(out, "a12b");
        assert!(
            apply_text_edits("abcdef", &[edit(0, 0, 0, 3, "x"), edit(0, 2, 0, 4, "y")]).is_none()
        );
    }

    #[test]
    fn crlf_and_out_of_range_positions_are_clamped() {
        let text = "one\r\ntwo\r\n";
        // Past the end of line 0 stays before "\r\n".
        let out = apply_text_edits(text, &[edit(0, 99, 0, 99, "!")]).unwrap();
        assert_eq!(out, "one!\r\ntwo\r\n");
        // Past the last line appends.
        let out = apply_text_edits(text, &[edit(9, 0, 9, 0, "end")]).unwrap();
        assert_eq!(out, "one\r\ntwo\r\nend");
        // Multi-line replacement.
        let out = apply_text_edits(text, &[edit(0, 1, 1, 2, "-")]).unwrap();
        assert_eq!(out, "o-o\r\n");
    }

    fn store(files: &[(&str, &str)]) -> MemoryStore {
        MemoryStore(
            files
                .iter()
                .map(|(p, t)| (p.to_string(), t.to_string()))
                .collect(),
        )
    }

    #[test]
    fn workspace_edits_apply_across_files_and_revert() {
        let mut s = store(&[
            ("app/User.php", "class User {}\n"),
            ("app/Order.php", "new User();\nUser::class;\n"),
        ]);
        let mut e = WorkspaceEdit::default();
        e.add_edits("app/User.php", vec![edit(0, 6, 0, 10, "Customer")]);
        e.add_edits("app/Order.php", vec![edit(0, 4, 0, 8, "Customer")]);
        e.add_edits("app/Order.php", vec![edit(1, 0, 1, 4, "Customer")]);
        e.changes.push(DocumentChange::Rename {
            from: "app/User.php".into(),
            to: "app/Customer.php".into(),
        });
        assert_eq!(e.changes.len(), 3, "edits of one file are merged");
        assert_eq!(e.edit_count(), 3);

        let applied = apply_workspace_edit(&e, &mut s).unwrap();
        assert_eq!(s.0.get("app/Customer.php").unwrap(), "class Customer {}\n");
        assert_eq!(
            s.0.get("app/Order.php").unwrap(),
            "new Customer();\nCustomer::class;\n"
        );
        assert!(!s.0.contains_key("app/User.php"));
        assert_eq!(applied.len(), 3);

        revert_applied(&applied, &mut s).unwrap();
        assert_eq!(
            s,
            store(&[
                ("app/User.php", "class User {}\n"),
                ("app/Order.php", "new User();\nUser::class;\n"),
            ])
        );
    }

    #[test]
    fn failing_edits_write_nothing() {
        let mut s = store(&[("a.php", "abc"), ("b.php", "abcdef")]);
        let mut e = WorkspaceEdit::default();
        e.add_edits("a.php", vec![edit(0, 0, 0, 1, "x")]);
        e.add_edits("b.php", vec![edit(0, 0, 0, 3, "x"), edit(0, 2, 0, 4, "y")]);
        assert_eq!(
            apply_workspace_edit(&e, &mut s),
            Err(EditError::Overlap {
                path: "b.php".into()
            })
        );
        assert_eq!(s.0.get("a.php").unwrap(), "abc");

        let mut e = WorkspaceEdit::default();
        e.add_edits("missing.php", vec![edit(0, 0, 0, 0, "x")]);
        assert!(matches!(
            apply_workspace_edit(&e, &mut s),
            Err(EditError::Missing { .. })
        ));
    }

    #[test]
    fn revert_refuses_files_changed_since() {
        let mut s = store(&[("a.php", "old")]);
        let mut e = WorkspaceEdit::default();
        e.add_edits("a.php", vec![edit(0, 0, 0, 3, "new")]);
        let applied = apply_workspace_edit(&e, &mut s).unwrap();
        s.0.insert("a.php".into(), "edited by hand".into());
        assert_eq!(
            revert_applied(&applied, &mut s),
            Err(EditError::Conflict {
                path: "a.php".into()
            })
        );
        assert_eq!(s.0.get("a.php").unwrap(), "edited by hand");
    }

    #[test]
    fn edits_can_be_split_between_editors_and_the_backend() {
        let mut e = WorkspaceEdit::default();
        e.add_edits("open.php", vec![edit(0, 0, 0, 1, "x")]);
        e.add_edits("closed.php", vec![edit(0, 0, 0, 1, "y")]);
        assert_eq!(e.edits_of("open.php").len(), 1);
        let rest = e.without_text_edits_of(&["open.php".into()]);
        assert_eq!(rest.paths(), vec!["closed.php".to_string()]);
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["changes"][0]["kind"], "edit");
        assert_eq!(json["changes"][0]["edits"][0]["newText"], "x");
    }

    #[test]
    fn preview_lists_changed_lines() {
        let text =
            "<?php\nclass User\n{\n    public function x(User $u) { return new User(); }\n}\n";
        let mut e = WorkspaceEdit::default();
        e.add_edits(
            "User.php",
            vec![
                edit(1, 6, 1, 10, "Customer"),
                edit(3, 44, 3, 48, "Customer"),
                edit(3, 22, 3, 26, "Customer"),
            ],
        );
        e.changes.push(DocumentChange::Rename {
            from: "User.php".into(),
            to: "Customer.php".into(),
        });
        let p = preview_workspace_edit(&e, |_| Some(text.to_string()));
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].rows.len(), 2);
        assert_eq!(p[0].rows[0].text, "class User");
        assert_eq!(p[0].rows[0].after, "class Customer");
        let row = &p[0].rows[1];
        assert_eq!(row.line, 3);
        assert_eq!(row.ranges, vec![[18, 22], [40, 44]]);
        assert_eq!(&row.text[18..22], "User");
        assert_eq!(
            row.after,
            "public function x(Customer $u) { return new Customer(); }"
        );
        assert_eq!(p[1].operation, "rename");
        assert_eq!(p[1].new_path.as_deref(), Some("Customer.php"));
    }
}
