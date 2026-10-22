//! Auto Import for PHP (Settings | Editor | General | Auto Import), like PHPStorm:
//!
//! - [`unresolved_classes`]: class names a file uses that resolve to no known class, with
//!   the classes of the index (project and vendor) a `use` statement could import. The app
//!   adds PHP's own classes from the stubs ([`UnresolvedClass::add_global`]).
//! - [`reference_edits`]: how a reference to a chosen class is written: imported, kept
//!   short (same namespace, imported already) or fully qualified (Prefer FQN, PHPDoc,
//!   global classes, auto-import scopes, name clashes). Completion and Import class use it.
//! - [`import_class_fix`]: the "Import class" quick fix (Alt+Enter), with the "Class to
//!   Import" chooser when several classes have the name.
//! - [`import_hint`]: the class the "? App\Models\User Alt+Enter" popup offers.
//! - [`on_the_fly`]: "Add unambiguous imports on the fly" and "Optimize imports on the
//!   fly", which only touch what the user is not editing right now.
//!
//! The pure functions take UTF-16 offsets (JS strings) and answer UTF-16 edits.

use serde::{Deserialize, Serialize};

use crate::generate::TextEdit;
use crate::index::ProjectIndex;
use crate::intentions::{Intention, IntentionKind};
use crate::model::{short_name, FileIndex, UseKind};
use crate::offsets::{byte_to_utf16, utf16_to_byte};
use crate::parser::{self, new_parser};
use crate::refactor::classes::{all_doc_class_names, code_class_names, is_special, namespace_of};

use super::insert::{insert_imports, NewImport};
use super::style::ImportStyle;
use super::{block_spans, import_edits, Mode};

/// The UI action of the ambiguous "Import class" fix: it shows the "Class to Import"
/// chooser and imports the chosen class with `php_import_reference`.
pub const IMPORT_CLASS_ACTION: &str = "PhpImportClass";

/// A class name that resolves to no known class.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnresolvedClass {
    /// The name's range (UTF-16 offsets).
    pub start: usize,
    pub end: usize,
    /// As written (a bare name).
    pub name: String,
    /// What the name resolves to in its namespace.
    pub resolved: String,
    /// In a PHPDoc comment.
    #[serde(default)]
    pub doc: bool,
    /// Classes named like it (fully qualified), project classes first.
    #[serde(default)]
    pub candidates: Vec<String>,
}

impl UnresolvedClass {
    /// PHP's own class `name` (from the stubs): the name is no longer unresolved in a file
    /// without namespace, and another candidate in a namespaced one.
    pub fn add_global(
        mut list: Vec<UnresolvedClass>,
        is_php_class: impl Fn(&str) -> bool,
    ) -> Vec<UnresolvedClass> {
        list.retain_mut(|u| {
            if !is_php_class(&u.name) {
                return true;
            }
            if !u.resolved.contains('\\') {
                return false;
            }
            if !u.candidates.iter().any(|c| c.eq_ignore_ascii_case(&u.name)) {
                u.candidates.push(u.name.clone());
            }
            true
        });
        list
    }
}

/// The unresolved class names of `src` (bytes in, UTF-16 out), with import candidates from
/// the index. Names the file imports, declares or that resolve to an indexed class are left
/// out, and so are qualified names.
pub fn unresolved_classes(index: &mut ProjectIndex, path: &str, src: &str) -> Vec<UnresolvedClass> {
    let file = parser::index_source(path, src);
    let Some(tree) = new_parser().parse(src, None) else {
        return vec![];
    };
    let root = tree.root_node();
    let mut names = code_class_names(root, src);
    names.extend(all_doc_class_names(root, src));
    let mut out: Vec<UnresolvedClass> = Vec::new();
    let mut candidates_of: std::collections::HashMap<String, Vec<String>> = Default::default();
    for n in names {
        if !n.is_bare() || is_special(&n.text) || imported_name(&file, &n.text) {
            continue;
        }
        let resolved = file.resolve_class(&n.text, n.span.start);
        let declared = file
            .classes
            .iter()
            .any(|c| c.fqn.eq_ignore_ascii_case(&resolved));
        if declared || index.resolve_class(&resolved).is_some() {
            continue;
        }
        let key = n.text.to_ascii_lowercase();
        let candidates = candidates_of
            .entry(key)
            .or_insert_with(|| index.class_candidates(&n.text))
            .clone();
        out.push(UnresolvedClass {
            start: byte_to_utf16(src, n.span.start),
            end: byte_to_utf16(src, n.span.end),
            name: n.text.clone(),
            resolved,
            doc: n.doc,
            candidates,
        });
    }
    out.sort_by_key(|u| u.start);
    out.dedup_by_key(|u| u.start);
    out
}

fn imported_name(file: &FileIndex, name: &str) -> bool {
    file.uses
        .iter()
        .any(|u| u.kind == UseKind::Class && u.local_name().eq_ignore_ascii_case(name))
}

/// How a reference to a class is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Reference {
    /// The short name plus a `use` statement.
    Import,
    /// The short name only (same namespace, or imported already).
    Short,
    /// `\App\Models\User`.
    Qualified,
}

/// How to write a reference to `fqn` at byte `at` of `src`. `auto` is true for the
/// automatic imports (completion, on the fly), which the auto-import scopes switch off.
pub fn decide(
    file: &FileIndex,
    at: usize,
    fqn: &str,
    doc: bool,
    style: &ImportStyle,
    auto: bool,
) -> Reference {
    let fqn = fqn.trim_start_matches('\\');
    let ns = file.namespace_at(at);
    let short = short_name(fqn);
    if namespace_of(fqn).eq_ignore_ascii_case(ns) {
        return Reference::Short;
    }
    let same_name = file
        .uses
        .iter()
        .find(|u| u.kind == UseKind::Class && u.local_name().eq_ignore_ascii_case(short));
    match same_name {
        Some(u) if u.fqn.eq_ignore_ascii_case(fqn) && u.alias.is_none() => return Reference::Short,
        Some(_) => return Reference::Qualified,
        None => {}
    }
    let clashes = file
        .classes
        .iter()
        .any(|c| c.name.eq_ignore_ascii_case(short));
    let global = !fqn.contains('\\');
    if clashes
        || (doc && style.fqn_in_phpdoc)
        || style.prefer_fqn
        || (global && !style.import_global_classes)
        || (auto && !ns.is_empty() && !style.namespace_scope)
        || (auto && ns.is_empty() && !style.file_scope)
    {
        return Reference::Qualified;
    }
    Reference::Import
}

/// The edits writing the reference `start..end` (UTF-16) of `src` as a reference to `fqn`:
/// the short name plus an import, or the fully qualified name (see [`decide`]).
pub fn reference_edits(
    src: &str,
    start: usize,
    end: usize,
    fqn: &str,
    doc: bool,
    style: &ImportStyle,
    auto: bool,
) -> Vec<TextEdit> {
    let fqn = fqn.trim_start_matches('\\');
    let (bs, be) = (
        utf16_to_byte(src, start),
        utf16_to_byte(src, end).min(src.len()),
    );
    if bs > be {
        return vec![];
    }
    let file = parser::index_source("", src);
    let current = &src[bs..be];
    let mut edits = Vec::new();
    let mut name = |text: String| {
        if text != current {
            edits.push(TextEdit {
                start: bs,
                end: be,
                new_text: text,
            });
        }
    };
    match decide(&file, bs, fqn, doc, style, auto) {
        Reference::Qualified => name(format!("\\{fqn}")),
        Reference::Short => {
            let local = file
                .uses
                .iter()
                .find(|u| u.kind == UseKind::Class && u.fqn.eq_ignore_ascii_case(fqn))
                .map_or(short_name(fqn), |u| u.local_name());
            name(local.to_string());
        }
        Reference::Import => {
            name(short_name(fqn).to_string());
            edits.extend(insert_imports(&file, src, &[NewImport::class(fqn)], style));
        }
    }
    to_utf16(src, edits)
}

fn to_utf16(src: &str, edits: Vec<TextEdit>) -> Vec<TextEdit> {
    edits
        .into_iter()
        .map(|e| TextEdit {
            start: byte_to_utf16(src, e.start),
            end: byte_to_utf16(src, e.end),
            new_text: e.new_text,
        })
        .collect()
}

/// Data of the ambiguous "Import class" fix ([`IMPORT_CLASS_ACTION`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportChoice {
    pub start: usize,
    pub end: usize,
    pub name: String,
    pub doc: bool,
    pub candidates: Vec<String>,
}

/// "Import class" for the unresolved name at UTF-16 `offset`: the edits for a single
/// candidate, the chooser for several, nothing without candidates.
pub fn import_class_fix(
    src: &str,
    unresolved: &[UnresolvedClass],
    offset: usize,
    style: &ImportStyle,
) -> Option<Intention> {
    let u = unresolved
        .iter()
        .find(|u| u.start <= offset && offset <= u.end && !u.candidates.is_empty())?;
    if let [fqn] = u.candidates.as_slice() {
        return Some(Intention {
            title: "Import class".into(),
            kind: IntentionKind::QuickFix,
            edits: reference_edits(src, u.start, u.end, fqn, u.doc, style, false),
            ..Default::default()
        });
    }
    Some(Intention {
        title: "Import class".into(),
        kind: IntentionKind::QuickFix,
        action: Some(IMPORT_CLASS_ACTION.into()),
        data: serde_json::to_value(ImportChoice {
            start: u.start,
            end: u.end,
            name: u.name.clone(),
            doc: u.doc,
            candidates: u.candidates.clone(),
        })
        .ok(),
        ..Default::default()
    })
}

/// The class the import popup offers at UTF-16 `caret`: the unresolved name under the
/// caret, else the nearest one on the caret's line (only names with candidates).
pub fn import_hint(
    src: &str,
    unresolved: &[UnresolvedClass],
    caret: usize,
) -> Option<UnresolvedClass> {
    let with = || unresolved.iter().filter(|u| !u.candidates.is_empty());
    if let Some(u) = with().find(|u| u.start <= caret && caret <= u.end) {
        return Some(u.clone());
    }
    let caret_b = utf16_to_byte(src, caret).min(src.len());
    let line = line_of(src, caret_b);
    with()
        .filter(|u| line_of(src, utf16_to_byte(src, u.start)) == line)
        .min_by_key(|u| u.start.abs_diff(caret))
        .cloned()
}

fn line_of(src: &str, byte: usize) -> usize {
    src[..byte.min(src.len())].matches('\n').count()
}

/// What happens on the fly after the user paused typing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OnTheFlyRequest {
    /// The caret (UTF-16).
    pub caret: usize,
    /// The text changed since the last check (UTF-16 range in the current text).
    pub edited_from: Option<usize>,
    pub edited_to: Option<usize>,
    /// "Add unambiguous imports on the fly".
    pub add: bool,
    /// "Optimize imports on the fly".
    pub optimize: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnTheFlyAnswer {
    /// UTF-16 edits of the current text.
    pub edits: Vec<TextEdit>,
    /// Imported classes.
    pub added: Vec<String>,
    /// Removed imports.
    pub removed: Vec<String>,
}

/// The on-the-fly import changes for `src`:
///
/// - unused imports are removed unless the user is editing the `use` block (caret or the
///   last edit on its lines) or typing a name the import provides (the word at the caret
///   is the start of its name);
/// - unresolved names with exactly one candidate are imported, except the one at the caret
///   (still being typed) and ones the style would write fully qualified.
///
/// Removing comes first; imports are added on a later pass, so the edits never overlap.
pub fn on_the_fly(
    src: &str,
    unresolved: &[UnresolvedClass],
    req: &OnTheFlyRequest,
    style: &ImportStyle,
) -> OnTheFlyAnswer {
    let mut answer = OnTheFlyAnswer::default();
    let caret = utf16_to_byte(src, req.caret).min(src.len());
    if req.optimize {
        let edited = match (req.edited_from, req.edited_to) {
            (Some(f), Some(t)) => Some((utf16_to_byte(src, f), utf16_to_byte(src, t))),
            _ => None,
        };
        let line_from = |b: usize| line_of(src, b);
        let busy: Vec<usize> = block_spans(src)
            .into_iter()
            .filter(|b| {
                let (first, last) = (line_from(b.start), line_from(b.end));
                let touches = |at: usize| (first..=last).contains(&line_from(at));
                touches(caret)
                    || edited.is_some_and(|(f, t)| {
                        touches(f) || touches(t) || (f <= b.start && b.end <= t)
                    })
            })
            .map(|b| b.start)
            .collect();
        let typed = crate::hover::word_at(src, caret)
            .or_else(|| {
                caret
                    .checked_sub(1)
                    .and_then(|c| crate::hover::word_at(src, c))
            })
            .map(|(s, _)| {
                src[s..caret.max(s)]
                    .trim_start_matches('\\')
                    .to_ascii_lowercase()
            })
            .unwrap_or_default();
        let keep = |name: &str, block: usize| {
            busy.contains(&block)
                || (!typed.is_empty() && name.to_ascii_lowercase().starts_with(&typed))
        };
        if let Some(out) = import_edits(src, Mode::RemoveUnused, style, &keep) {
            if !out.edits.is_empty() {
                answer.removed = out.removed;
                answer.edits = to_utf16(
                    src,
                    out.edits
                        .into_iter()
                        .map(|(s, e, t)| TextEdit {
                            start: s,
                            end: e,
                            new_text: t,
                        })
                        .collect(),
                );
                return answer;
            }
        }
    }
    if req.add {
        let file = parser::index_source("", src);
        let mut imports: Vec<NewImport> = Vec::new();
        for u in unresolved {
            let [fqn] = u.candidates.as_slice() else {
                continue;
            };
            if u.start <= req.caret && req.caret <= u.end {
                continue;
            }
            let at = utf16_to_byte(src, u.start);
            if decide(&file, at, fqn, u.doc, style, true) != Reference::Import {
                continue;
            }
            // Two names with the same short name but different classes: not unambiguous.
            if imports.iter().any(|i| {
                short_name(&i.fqn).eq_ignore_ascii_case(short_name(fqn))
                    && !i.fqn.eq_ignore_ascii_case(fqn)
            }) {
                continue;
            }
            if !imports.iter().any(|i| i.fqn.eq_ignore_ascii_case(fqn)) {
                imports.push(NewImport::class(fqn));
            }
        }
        let edits = insert_imports(&file, src, &imports, style);
        if !edits.is_empty() {
            answer.added = imports.into_iter().map(|i| i.fqn).collect();
            answer.edits = to_utf16(src, edits);
        }
    }
    answer
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate::apply_edits;
    use crate::IndexConfig;

    fn unresolved(src: &str, files: &[(&str, &str)]) -> Vec<UnresolvedClass> {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in files {
            let p = dir.path().join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        let mut idx = ProjectIndex::new(dir.path(), IndexConfig::default());
        idx.index_workspace(|_, _| {});
        unresolved_classes(&mut idx, "app/Http/C.php", src)
    }

    const MODELS: &[(&str, &str)] = &[
        (
            "app/Models/User.php",
            "<?php\nnamespace App\\Models;\nclass User {}\n",
        ),
        (
            "app/Models/Order.php",
            "<?php\nnamespace App\\Models;\nclass Order {}\n",
        ),
        (
            "legacy/Order.php",
            "<?php\nnamespace Legacy;\nclass Order {}\n",
        ),
        (
            "app/Http/Request.php",
            "<?php\nnamespace App\\Http;\nclass Request {}\n",
        ),
    ];

    #[test]
    fn finds_unresolved_names_with_candidates() {
        let src = "<?php\nnamespace App\\Http;\n\nuse App\\Models\\User;\n\nclass C {\n    /** @param Order $o */\n    public function f(User $u, Request $r, Order $o, Missing $m, \\Fqn\\X $x, int $i): self {}\n}\n";
        let list = unresolved(src, MODELS);
        let names: Vec<(&str, bool, Vec<&str>)> = list
            .iter()
            .map(|u| {
                (
                    u.name.as_str(),
                    u.doc,
                    u.candidates.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            names,
            vec![
                ("Order", true, vec!["App\\Models\\Order", "Legacy\\Order"]),
                ("Order", false, vec!["App\\Models\\Order", "Legacy\\Order"]),
                ("Missing", false, vec![]),
            ]
        );
        assert_eq!(list[1].resolved, "App\\Http\\Order");
        let at = src.find("Order $o,").unwrap();
        assert_eq!(list[1].start, at);
    }

    #[test]
    fn php_classes_from_the_stubs() {
        let mk = |resolved: &str| UnresolvedClass {
            start: 0,
            end: 9,
            name: "Exception".into(),
            resolved: resolved.into(),
            doc: false,
            candidates: vec!["App\\Exception".into()],
        };
        let is_php = |n: &str| n == "Exception";
        assert!(UnresolvedClass::add_global(vec![mk("Exception")], is_php).is_empty());
        let list = UnresolvedClass::add_global(vec![mk("App\\Http\\Exception")], is_php);
        assert_eq!(list[0].candidates, vec!["App\\Exception", "Exception"]);
    }

    fn apply(src: &str, edits: &[TextEdit]) -> String {
        // UTF-16 equals bytes for these ASCII samples.
        apply_edits(src, edits)
    }

    #[test]
    fn reference_is_imported_short_or_qualified() {
        let src = "<?php\nnamespace App\\Http;\n\nuse Other\\User;\n\nclass C {}\nnew Order();\n";
        let at = src.find("Order").unwrap();
        let style = ImportStyle::default();
        let edit = |fqn: &str, style: &ImportStyle, auto: bool| {
            apply(
                src,
                &reference_edits(src, at, at + 5, fqn, false, style, auto),
            )
        };
        assert!(edit("App\\Models\\Order", &style, false)
            .contains("use App\\Models\\Order;\nuse Other\\User;"));
        // Same namespace: nothing to import.
        assert_eq!(edit("App\\Http\\Order", &style, false), src);
        // Prefer FQN, or auto-import switched off for namespaces.
        let fqn = ImportStyle {
            prefer_fqn: true,
            ..Default::default()
        };
        assert!(edit("App\\Models\\Order", &fqn, false).contains("new \\App\\Models\\Order();"));
        let no_ns_scope = ImportStyle {
            namespace_scope: false,
            ..Default::default()
        };
        assert!(
            edit("App\\Models\\Order", &no_ns_scope, true).contains("new \\App\\Models\\Order();")
        );
        assert!(edit("App\\Models\\Order", &no_ns_scope, false).contains("use App\\Models\\Order;"));
        // Global classes.
        let no_global = ImportStyle {
            import_global_classes: false,
            ..Default::default()
        };
        assert!(edit("Order", &no_global, false).contains("new \\Order();"));
        assert!(edit("Order", &style, false).contains("use Order;"));
    }

    #[test]
    fn clashing_names_and_phpdoc_are_qualified() {
        let src = "<?php\nnamespace App;\n\nuse Other\\Order;\n\n/** @var Order */\n";
        let at = src.find("Order */").unwrap();
        let style = ImportStyle::default();
        let out = apply(
            src,
            &reference_edits(src, at, at + 5, "App\\Models\\Order", true, &style, false),
        );
        assert!(out.contains("@var \\App\\Models\\Order */"));
        let src2 = "<?php\nnamespace App;\n\n/** @var User */\n";
        let at2 = src2.find("User").unwrap();
        let doc = ImportStyle {
            fqn_in_phpdoc: true,
            ..Default::default()
        };
        assert!(apply(
            src2,
            &reference_edits(src2, at2, at2 + 4, "M\\User", true, &doc, false)
        )
        .contains("@var \\M\\User */"));
        assert!(apply(
            src2,
            &reference_edits(src2, at2, at2 + 4, "M\\User", true, &style, false)
        )
        .contains("use M\\User;"));
    }

    #[test]
    fn file_scope_controls_files_without_namespace() {
        let src = "<?php\n\nnew User();\n";
        let at = src.find("User").unwrap();
        let style = ImportStyle::default();
        assert!(apply(
            src,
            &reference_edits(src, at, at + 4, "M\\User", false, &style, true)
        )
        .contains("new \\M\\User();"));
        let file_scope = ImportStyle {
            file_scope: true,
            ..Default::default()
        };
        assert_eq!(
            apply(
                src,
                &reference_edits(src, at, at + 4, "M\\User", false, &file_scope, true)
            ),
            "<?php\n\nuse M\\User;\n\nnew User();\n"
        );
    }

    #[test]
    fn import_class_fix_offers_a_chooser_when_ambiguous() {
        let src = "<?php\nnamespace App\\Http;\n\nnew Order(); new User();\n";
        let list = unresolved(src, MODELS);
        let order = src.find("Order").unwrap();
        let fix = import_class_fix(src, &list, order + 2, &ImportStyle::default()).unwrap();
        assert_eq!(fix.action.as_deref(), Some(IMPORT_CLASS_ACTION));
        let choice: ImportChoice = serde_json::from_value(fix.data.unwrap()).unwrap();
        assert_eq!(
            choice.candidates,
            vec!["App\\Models\\Order", "Legacy\\Order"]
        );
        let user = src.find("User").unwrap();
        let fix = import_class_fix(src, &list, user, &ImportStyle::default()).unwrap();
        assert_eq!(fix.title, "Import class");
        assert_eq!(
            apply(src, &fix.edits),
            "<?php\nnamespace App\\Http;\n\nuse App\\Models\\User;\n\nnew Order(); new User();\n"
        );
        assert!(import_class_fix(src, &list, 3, &ImportStyle::default()).is_none());
    }

    #[test]
    fn hint_prefers_the_name_under_the_caret_then_the_line() {
        let src = "<?php\nnamespace App\\Http;\n\nnew Order(); new User();\n$x = 1;\n";
        let list = unresolved(src, MODELS);
        let user = src.find("User").unwrap();
        assert_eq!(import_hint(src, &list, user + 1).unwrap().name, "User");
        let line_end = src.find("\n$x").unwrap();
        assert_eq!(import_hint(src, &list, line_end).unwrap().name, "User");
        assert!(import_hint(src, &list, src.len() - 2).is_none());
    }

    #[test]
    fn on_the_fly_adds_unambiguous_imports_except_at_the_caret() {
        let src = "<?php\nnamespace App\\Http;\n\nnew Order(); new User();\n$x = 1;\n";
        let list = unresolved(src, MODELS);
        let req = OnTheFlyRequest {
            caret: src.len() - 1,
            add: true,
            ..Default::default()
        };
        let out = on_the_fly(src, &list, &req, &ImportStyle::default());
        assert_eq!(out.added, vec!["App\\Models\\User"]);
        assert_eq!(
            apply(src, &out.edits),
            "<?php\nnamespace App\\Http;\n\nuse App\\Models\\User;\n\nnew Order(); new User();\n$x = 1;\n"
        );
        // Still typing "User".
        let typing = OnTheFlyRequest {
            caret: src.find("User").unwrap() + 4,
            ..req.clone()
        };
        assert!(on_the_fly(src, &list, &typing, &ImportStyle::default())
            .edits
            .is_empty());
        // Prefer FQN: nothing is imported on the fly.
        let fqn = ImportStyle {
            prefer_fqn: true,
            ..Default::default()
        };
        assert!(on_the_fly(src, &list, &req, &fqn).edits.is_empty());
    }

    #[test]
    fn on_the_fly_removes_unused_imports_only_away_from_the_edit() {
        let src = "<?php\nnamespace App;\n\nuse App\\Models\\User;\nuse App\\Models\\Order;\n\nnew Order();\n$u = Us\n";
        let end = src.len() - 1;
        let req = OnTheFlyRequest {
            caret: end,
            optimize: true,
            ..Default::default()
        };
        // "Us" at the caret may become "User": kept.
        assert!(on_the_fly(src, &[], &req, &ImportStyle::default())
            .edits
            .is_empty());
        let away = "<?php\nnamespace App;\n\nuse App\\Models\\User;\nuse App\\Models\\Order;\n\nnew Order();\n$u = 1;\n";
        let req = OnTheFlyRequest {
            caret: away.len() - 1,
            optimize: true,
            ..Default::default()
        };
        let out = on_the_fly(away, &[], &req, &ImportStyle::default());
        assert_eq!(out.removed, vec!["App\\Models\\User"]);
        assert_eq!(
            apply(away, &out.edits),
            "<?php\nnamespace App;\n\nuse App\\Models\\Order;\n\nnew Order();\n$u = 1;\n"
        );
        // The caret or the last edit in the imports: the user is editing them.
        let in_block = OnTheFlyRequest {
            caret: away.find("User;").unwrap(),
            ..req.clone()
        };
        assert!(on_the_fly(away, &[], &in_block, &ImportStyle::default())
            .edits
            .is_empty());
        let edited = OnTheFlyRequest {
            edited_from: Some(away.find("use App\\Models\\Order").unwrap()),
            edited_to: Some(away.find("use App\\Models\\Order").unwrap() + 3),
            ..req
        };
        assert!(on_the_fly(away, &[], &edited, &ImportStyle::default())
            .edits
            .is_empty());
    }
}
