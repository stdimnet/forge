//! Rename refactoring (Shift+F6): the workspace edit from a list of references (the PHP
//! index when the language server cannot rename), name validation, PHPStorm's "rename the
//! file too" suggestion for classes, conflicts to show before refactoring, and how an edit
//! is split between open editors (undoable there) and the project backend.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::edit::{apply_text_edits, DocumentChange, Range, TextEdit, WorkspaceEdit};
use crate::usages::{UsageKind, UsageLocation};

/// A problem PHPStorm lists in "Problems Detected" before the refactoring runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameConflict {
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// 0-based line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

impl RenameConflict {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            path: None,
            line: None,
        }
    }

    pub fn at(message: impl Into<String>, path: impl Into<String>, line: Option<u32>) -> Self {
        Self {
            message: message.into(),
            path: Some(path.into()),
            line,
        }
    }
}

/// A file renamed along with the symbol (`app/Models/User.php` → `Customer.php`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRename {
    pub from: String,
    pub to: String,
}

/// Why `name` cannot be a new symbol name, PHPStorm's wording; `None` when it is fine. A
/// leading `$` (PHP variables and properties) is allowed.
pub fn validate_identifier(name: &str) -> Option<String> {
    let bare = name.strip_prefix('$').unwrap_or(name);
    let mut chars = bare.chars();
    let valid = match chars.next() {
        Some(c) if c == '_' || c.is_alphabetic() => chars.all(|c| c == '_' || c.is_alphanumeric()),
        _ => false,
    };
    (!valid).then(|| format!("'{name}' is not a valid identifier"))
}

/// The UTF-16 range `r` of a single line of `text`.
fn slice_of(text: &str, r: &Range) -> Option<String> {
    let line = text
        .split('\n')
        .nth(r.start.line as usize)?
        .trim_end_matches('\r');
    let units: Vec<u16> = line.encode_utf16().collect();
    let end = if r.end.line == r.start.line {
        r.end.character as usize
    } else {
        units.len()
    };
    String::from_utf16(units.get(r.start.character as usize..end.min(units.len()))?).ok()
}

/// The rename edit built from references.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationEdit {
    pub edit: WorkspaceEdit,
    /// Dynamic usages (members on untyped receivers) that are left alone, like PHPStorm
    /// does unless asked.
    pub skipped: usize,
}

/// Replace every reference (and the declaration, which should be among `locations`) with
/// `new_name`. A `$` is kept where the old text had one (`$email` declaration, `->email`
/// usage); library files and dynamic usages are left out; duplicates are dropped.
pub fn edit_from_locations(
    locations: &[UsageLocation],
    new_name: &str,
    mut read: impl FnMut(&str) -> Option<String>,
) -> LocationEdit {
    let bare = new_name.strip_prefix('$').unwrap_or(new_name);
    let mut texts: HashMap<String, Option<String>> = HashMap::new();
    let mut by_path: Vec<(String, Vec<TextEdit>)> = Vec::new();
    let mut skipped = 0;
    for loc in locations {
        if loc.path.contains("://") {
            continue;
        }
        if loc.kind == Some(UsageKind::Dynamic) {
            skipped += 1;
            continue;
        }
        let text = texts
            .entry(loc.path.clone())
            .or_insert_with(|| read(&loc.path));
        let sigil = text
            .as_deref()
            .and_then(|t| slice_of(t, &loc.range))
            .is_some_and(|old| old.starts_with('$'));
        let new_text = if sigil {
            format!("${bare}")
        } else {
            bare.to_string()
        };
        let slot = match by_path.iter().position(|(p, _)| *p == loc.path) {
            Some(i) => &mut by_path[i].1,
            None => {
                by_path.push((loc.path.clone(), Vec::new()));
                &mut by_path.last_mut().expect("just pushed").1
            }
        };
        if !slot.iter().any(|e| e.range == loc.range) {
            slot.push(TextEdit {
                range: loc.range,
                new_text,
            });
        }
    }
    let mut edit = WorkspaceEdit::default();
    for (path, mut edits) in by_path {
        edits.sort_by_key(|e| e.range);
        edit.add_edits(path, edits);
    }
    LocationEdit { edit, skipped }
}

/// The ranges `edit` replaces in `path`, in document order (the occurrences an in-place
/// rename edits live).
pub fn ranges_in(edit: &WorkspaceEdit, path: &str) -> Vec<Range> {
    let mut out: Vec<Range> = edit.edits_of(path).into_iter().map(|e| e.range).collect();
    out.sort();
    out.dedup();
    out
}

fn file_stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.split('.').next().unwrap_or(name)
}

/// Whether `text` declares a class, interface, trait or enum called `name`.
pub fn declares_class(text: &str, name: &str) -> bool {
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    ["class", "interface", "trait", "enum"].iter().any(|kw| {
        text.match_indices(kw).any(|(i, _)| {
            let before_ok = !text[..i].chars().next_back().is_some_and(ident);
            let rest = &text[i + kw.len()..];
            let trimmed = rest.trim_start();
            before_ok
                && trimmed.len() < rest.len()
                && trimmed.starts_with(name)
                && !trimmed[name.len()..].chars().next().is_some_and(ident)
        })
    })
}

/// PHPStorm renames a class's file with it when the file is named after the class
/// (`User.php` declaring `class User`).
pub fn is_class_file(path: &str, text: &str, name: &str) -> bool {
    !name.is_empty() && file_stem(path) == name && declares_class(text, name)
}

/// `app/Models/User.php` renamed for class `Customer` → `app/Models/Customer.php`.
pub fn renamed_path(path: &str, new_name: &str) -> String {
    let (dir, file) = match path.rfind('/') {
        Some(i) => (&path[..=i], &path[i + 1..]),
        None => ("", path),
    };
    let ext = file.find('.').map_or("", |i| &file[i..]);
    format!("{dir}{new_name}{ext}")
}

/// The file to rename along with class `old`: the first of `paths` named after it that
/// declares it.
pub fn suggest_file_rename(
    paths: &[String],
    old: &str,
    new_name: &str,
    mut read: impl FnMut(&str) -> Option<String>,
) -> Option<FileRename> {
    paths.iter().find_map(|p| {
        if p.contains("://") || file_stem(p) != old {
            return None;
        }
        let text = read(p)?;
        is_class_file(p, &text, old).then(|| FileRename {
            from: p.clone(),
            to: renamed_path(p, new_name),
        })
    })
}

/// Problems of applying `edit` itself: overlapping edits in a file and files that would be
/// created or renamed over existing ones (`exists` asks the backend).
pub fn edit_conflicts(
    edit: &WorkspaceEdit,
    mut exists: impl FnMut(&str) -> bool,
) -> Vec<RenameConflict> {
    let mut out = Vec::new();
    for change in &edit.changes {
        match change {
            DocumentChange::Edit { path, edits } => {
                // The text does not matter for overlaps, only the positions; a blank text
                // clamps every position to 0, so use one long enough for all edits.
                let lines = edits.iter().map(|e| e.range.end.line).max().unwrap_or(0) as usize;
                let width = edits
                    .iter()
                    .map(|e| e.range.end.character.max(e.range.start.character))
                    .max()
                    .unwrap_or(0) as usize;
                let text = format!("{}\n", " ".repeat(width)).repeat(lines + 1);
                if apply_text_edits(&text, edits).is_none() {
                    out.push(RenameConflict::at(
                        "Overlapping changes cannot be applied",
                        path.clone(),
                        None,
                    ));
                }
            }
            DocumentChange::Rename { to, .. } if exists(to) => {
                out.push(RenameConflict::at(
                    format!("File {to} already exists"),
                    to.clone(),
                    None,
                ));
            }
            DocumentChange::Create {
                path,
                overwrite: false,
            } if exists(path) => {
                out.push(RenameConflict::at(
                    format!("File {path} already exists"),
                    path.clone(),
                    None,
                ));
            }
            _ => {}
        }
    }
    out
}

/// Text edits of one open editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorEdits {
    pub path: String,
    pub edits: Vec<TextEdit>,
}

/// How the UI applies a workspace edit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditPlan {
    /// Open files that are only edited: the editors apply these as one undoable step.
    pub editors: Vec<EditorEdits>,
    /// Everything else, applied through the backend (closed files, file operations, and
    /// the text edits of open files that are renamed or deleted).
    pub backend: WorkspaceEdit,
    /// Open files the backend writes: save their unsaved edits first.
    pub save_first: Vec<String>,
    /// Open files that move: reopen their tabs at the new path.
    pub reopen: Vec<FileRename>,
    /// Open files that are deleted: close their tabs.
    pub close: Vec<String>,
    /// Number of files the edit touches.
    pub file_count: usize,
}

/// Split `edit` for the UI: `open` are the files open in an editor.
pub fn plan_edit(edit: &WorkspaceEdit, open: &[String]) -> EditPlan {
    let is_open = |p: &str| open.iter().any(|o| o == p);
    let mut plan = EditPlan {
        file_count: edit.paths().len()
            - edit
                .changes
                .iter()
                .filter(|c| matches!(c, DocumentChange::Rename { .. }))
                .count(),
        ..Default::default()
    };
    // Open files whose path changes are written by the backend as a whole.
    let mut moved: Vec<String> = Vec::new();
    for c in &edit.changes {
        match c {
            DocumentChange::Rename { from, to } if is_open(from) => {
                moved.push(from.clone());
                plan.reopen.push(FileRename {
                    from: from.clone(),
                    to: to.clone(),
                });
            }
            DocumentChange::Delete { path } if is_open(path) => {
                moved.push(path.clone());
                plan.close.push(path.clone());
            }
            _ => {}
        }
    }
    let mut in_editors: Vec<String> = Vec::new();
    for c in &edit.changes {
        if let DocumentChange::Edit { path, .. } = c {
            if !is_open(path) || in_editors.contains(path) {
                continue;
            }
            if moved.contains(path) {
                if !plan.save_first.contains(path) {
                    plan.save_first.push(path.clone());
                }
                continue;
            }
            in_editors.push(path.clone());
            plan.editors.push(EditorEdits {
                path: path.clone(),
                edits: edit.edits_of(path),
            });
        }
    }
    for p in &moved {
        if !plan.save_first.contains(p) {
            plan.save_first.push(p.clone());
        }
    }
    plan.backend = edit.without_text_edits_of(&in_editors);
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Position;

    fn loc(path: &str, line: u32, start: u32, end: u32) -> UsageLocation {
        UsageLocation {
            path: path.into(),
            range: Range::on_line(line, start, end),
            kind: None,
        }
    }

    #[test]
    fn validates_identifiers() {
        assert_eq!(validate_identifier("Customer"), None);
        assert_eq!(validate_identifier("$total_cents"), None);
        assert_eq!(validate_identifier("_ünits2"), None);
        assert_eq!(
            validate_identifier("2fa"),
            Some("'2fa' is not a valid identifier".into())
        );
        assert!(validate_identifier("").is_some());
        assert!(validate_identifier("$").is_some());
        assert!(validate_identifier("first name").is_some());
        assert!(validate_identifier("a-b").is_some());
    }

    #[test]
    fn builds_edits_from_references_keeping_the_sigil() {
        let files: HashMap<&str, &str> = HashMap::from([
            (
                "app/Models/User.php",
                "<?php\nclass User {\n    public string $email;\n}\n",
            ),
            (
                "app/Mail.php",
                "<?php\n$u->email = 1;\n$x->email;\necho $u->email;\n",
            ),
        ]);
        let mut dynamic = loc("app/Mail.php", 2, 4, 9);
        dynamic.kind = Some(UsageKind::Dynamic);
        let locations = vec![
            loc("app/Models/User.php", 2, 18, 24),
            loc("app/Mail.php", 1, 4, 9),
            dynamic,
            loc("app/Mail.php", 3, 9, 14),
            loc("app/Mail.php", 1, 4, 9),
            loc("library://php-stubs/standard.php", 1, 0, 5),
        ];
        let out = edit_from_locations(&locations, "address", |p| {
            files.get(p).map(|s| s.to_string())
        });
        assert_eq!(out.skipped, 1);
        assert_eq!(
            out.edit.paths(),
            vec!["app/Models/User.php".to_string(), "app/Mail.php".into()]
        );
        let user = out.edit.edits_of("app/Models/User.php");
        assert_eq!(user[0].new_text, "$address");
        let mail = out.edit.edits_of("app/Mail.php");
        assert_eq!(mail.len(), 2, "duplicates are dropped");
        assert!(mail.iter().all(|e| e.new_text == "address"));
        assert_eq!(
            apply_text_edits(files["app/Mail.php"], &mail).unwrap(),
            "<?php\n$u->address = 1;\n$x->email;\necho $u->address;\n"
        );
        assert_eq!(
            ranges_in(&out.edit, "app/Mail.php"),
            vec![Range::on_line(1, 4, 9), Range::on_line(3, 9, 14)]
        );

        // Variables: a new name typed with `$` does not get a second one.
        let out = edit_from_locations(&[loc("a.php", 0, 0, 2)], "$y", |_| Some("$x = 1;".into()));
        assert_eq!(out.edit.edits_of("a.php")[0].new_text, "$y");
    }

    #[test]
    fn suggests_renaming_the_class_file() {
        let user = "<?php\nnamespace App\\Models;\n\nfinal class User extends Model {}\n";
        assert!(declares_class(user, "User"));
        assert!(!declares_class(user, "Use"));
        assert!(!declares_class("<?php\n$classUser = 1;", "User"));
        assert!(declares_class("<?php\nenum Status: string {}", "Status"));
        assert!(is_class_file("app/Models/User.php", user, "User"));
        assert!(!is_class_file("app/Models/Users.php", user, "User"));
        assert_eq!(
            renamed_path("app/Models/User.php", "Customer"),
            "app/Models/Customer.php"
        );
        assert_eq!(renamed_path("User.php", "Customer"), "Customer.php");
        assert_eq!(
            renamed_path("views/User.blade.php", "Customer"),
            "views/Customer.blade.php"
        );

        let paths = vec![
            "app/Http/Controllers/UserController.php".to_string(),
            "app/Models/User.php".to_string(),
        ];
        let found = suggest_file_rename(&paths, "User", "Customer", |p| {
            (p == "app/Models/User.php").then(|| user.to_string())
        });
        assert_eq!(
            found,
            Some(FileRename {
                from: "app/Models/User.php".into(),
                to: "app/Models/Customer.php".into()
            })
        );
        // A file named like the class that declares something else stays.
        assert_eq!(
            suggest_file_rename(&paths, "User", "Customer", |_| Some(
                "<?php\nreturn [];".into()
            )),
            None
        );
    }

    #[test]
    fn detects_conflicts() {
        let mut e = WorkspaceEdit::default();
        e.add_edits(
            "a.php",
            vec![
                TextEdit {
                    range: Range::on_line(0, 0, 4),
                    new_text: "x".into(),
                },
                TextEdit {
                    range: Range::on_line(0, 2, 6),
                    new_text: "y".into(),
                },
            ],
        );
        e.add_edits(
            "b.php",
            vec![TextEdit {
                range: Range::new(Position::new(3, 1), Position::new(3, 5)),
                new_text: "z".into(),
            }],
        );
        e.changes.push(DocumentChange::Rename {
            from: "app/User.php".into(),
            to: "app/Customer.php".into(),
        });
        let conflicts = edit_conflicts(&e, |p| p == "app/Customer.php");
        assert_eq!(
            conflicts,
            vec![
                RenameConflict::at("Overlapping changes cannot be applied", "a.php", None),
                RenameConflict::at(
                    "File app/Customer.php already exists",
                    "app/Customer.php",
                    None
                ),
            ]
        );
        assert!(edit_conflicts(&e, |_| false).len() == 1);
    }

    #[test]
    fn plans_edits_for_open_editors_and_the_backend() {
        let te = |line| TextEdit {
            range: Range::on_line(line, 0, 4),
            new_text: "Customer".into(),
        };
        let mut e = WorkspaceEdit::default();
        e.add_edits("app/Models/User.php", vec![te(3)]);
        e.add_edits(
            "app/Http/Controllers/UserController.php",
            vec![te(5), te(9)],
        );
        e.add_edits("routes/web.php", vec![te(2)]);
        e.changes.push(DocumentChange::Rename {
            from: "app/Models/User.php".into(),
            to: "app/Models/Customer.php".into(),
        });
        let open = vec![
            "app/Models/User.php".to_string(),
            "app/Http/Controllers/UserController.php".to_string(),
        ];
        let plan = plan_edit(&e, &open);
        assert_eq!(plan.file_count, 3);
        assert_eq!(plan.editors.len(), 1);
        assert_eq!(
            plan.editors[0].path,
            "app/Http/Controllers/UserController.php"
        );
        assert_eq!(plan.editors[0].edits.len(), 2);
        // The renamed open file is written by the backend with its edits, then reopened.
        assert_eq!(
            plan.backend.paths(),
            vec![
                "app/Models/User.php".to_string(),
                "routes/web.php".into(),
                "app/Models/Customer.php".into()
            ]
        );
        assert_eq!(plan.save_first, vec!["app/Models/User.php".to_string()]);
        assert_eq!(
            plan.reopen,
            vec![FileRename {
                from: "app/Models/User.php".into(),
                to: "app/Models/Customer.php".into()
            }]
        );
        assert!(plan.close.is_empty());

        // Nothing open: everything goes to the backend.
        let plan = plan_edit(&e, &[]);
        assert!(plan.editors.is_empty());
        assert_eq!(plan.backend, e);
        assert!(plan.save_first.is_empty());
    }
}
