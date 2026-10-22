//! The project-wide refactorings of the Refactor menu that need Find Usages: Move Class,
//! Copy Class, moving PHP files, Safe Delete and Change Signature. They read the project
//! through a [`Project`] (the app's backend, so remote projects work; the PHP index in
//! tests) and answer with a [`ProjectPlan`]: text edits (bytes) per file, file operations
//! and the problems for PHPStorm's "Problems Detected" dialog.

use std::collections::HashMap;

use forge_codeintel::{
    utf16_offset, Position, Range, RenameConflict, UsageKind, UsageLocation, UsageOptions,
};
use serde::{Deserialize, Serialize};

use super::classes::qualify;
use super::move_class::{
    check_namespace, class_file_path, class_in_header, classes_in, extract_class_file, file_edits,
    fq_references, moved_text, remove_class, ClassMove,
};
use super::safe_delete::{self, DeleteTarget};
use super::signature::{self, ChangeSignatureOptions, SignatureInfo};
use super::{cannot, conflict_at, Span};
use crate::generate::TextEdit;
use crate::index::{SearchKind, SymbolHit};
use crate::navigate::{is_vendor, PhpUsages};
use crate::offsets::utf16_to_byte;

/// What the refactorings need from the project.
pub trait Project {
    /// Find Usages of the symbol at `offset` (bytes) of `path`, whose content is `text`.
    fn usages(
        &mut self,
        path: &str,
        text: &str,
        offset: usize,
        options: &UsageOptions,
    ) -> Option<PhpUsages>;
    /// A project file's text (editor buffers first); `None` when it does not exist.
    fn read(&mut self, path: &str) -> Option<String>;
    /// Go to Declaration: the file and byte offset of the declaration's name.
    fn definition(&mut self, path: &str, text: &str, offset: usize) -> Option<(String, usize)>;
    /// Symbols named exactly `name`.
    fn search(&mut self, name: &str, kind: SearchKind) -> Vec<SymbolHit>;
}

/// One step of a plan, bytes into the file's current text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FileOp {
    Edit { path: String, edits: Vec<TextEdit> },
    Create { path: String, text: String },
    Rename { from: String, to: String },
    Delete { path: String },
}

/// The result of a project-wide refactoring.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPlan {
    pub ops: Vec<FileOp>,
    pub conflicts: Vec<RenameConflict>,
}

impl ProjectPlan {
    fn edit(&mut self, path: &str, edits: Vec<TextEdit>) {
        if edits.is_empty() {
            return;
        }
        for op in &mut self.ops {
            if let FileOp::Edit { path: p, edits: e } = op {
                if p == path {
                    e.extend(edits);
                    *e = super::classes::tidy(std::mem::take(e));
                    return;
                }
            }
        }
        self.ops.push(FileOp::Edit {
            path: path.to_string(),
            edits,
        });
    }

    fn conflict(&mut self, message: String, path: Option<&str>, line: Option<u32>) {
        self.conflicts.push(match path {
            Some(p) => RenameConflict::at(message, p, line),
            None => RenameConflict::new(message),
        });
    }

    /// The edits of `path` (for tests and previews).
    pub fn edits_of(&self, path: &str) -> Vec<TextEdit> {
        self.ops
            .iter()
            .filter_map(|op| match op {
                FileOp::Edit { path: p, edits } if p == path => Some(edits.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }
}

fn byte_of(text: &str, p: Position) -> usize {
    utf16_to_byte(text, utf16_offset(text, p))
}

/// The byte span of an LSP range in `text`.
pub fn span_of(text: &str, r: &Range) -> Span {
    Span::new(byte_of(text, r.start), byte_of(text, r.end))
}

fn line_of(text: &str, byte: usize) -> u32 {
    super::line_of(text, byte)
}

fn usage_options(comments: bool) -> UsageOptions {
    UsageOptions {
        search_in_comments_and_strings: comments,
        ..UsageOptions::default()
    }
}

/// Usages as byte spans per file (vendor files and the declaration left out).
fn usage_spans(p: &mut dyn Project, found: &PhpUsages) -> Vec<(String, Span, Option<UsageKind>)> {
    let mut out = Vec::new();
    for u in &found.usages {
        if is_vendor(&u.path) || u.path.contains("://") {
            continue;
        }
        if found
            .declaration
            .as_ref()
            .is_some_and(|d| d.path == u.path && d.range == u.range)
        {
            continue;
        }
        let Some(text) = p.read(&u.path) else {
            continue;
        };
        out.push((u.path.clone(), span_of(&text, &u.range), u.kind));
    }
    out
}

fn group(spans: Vec<(String, Span, usize)>) -> Vec<(String, Vec<(Span, usize)>)> {
    super::move_class::group_by_file(&spans)
}

/// The declaration at the caret (via `find`), or the one the reference at the caret
/// points to: its file, text and what `find` made of it.
pub fn at_declaration<T>(
    p: &mut dyn Project,
    path: &str,
    text: &str,
    offset: usize,
    find: impl Fn(&str, usize) -> Result<T, String>,
) -> Result<(String, String, T), String> {
    match find(text, offset) {
        Ok(t) => Ok((path.to_string(), text.to_string(), t)),
        Err(e) => {
            let Some((decl_path, at)) = p.definition(path, text, offset) else {
                return Err(e);
            };
            if is_vendor(&decl_path) || decl_path.contains("://") {
                return Err(cannot("The element is in a read-only library"));
            }
            let decl_text = p.read(&decl_path).ok_or_else(|| e.clone())?;
            find(&decl_text, at)
                .map(|t| (decl_path, decl_text, t))
                .map_err(|_| e)
        }
    }
}

// ---- Move Class / Copy Class ----

/// Move Class (F6): the class at `offset` into `target_ns`, its file into `target_dir`
/// (the PSR-4 folder of the namespace). A class alone in its file moves with the file;
/// otherwise it goes into a new file and leaves the old one.
#[allow(clippy::too_many_arguments)]
pub fn move_class(
    p: &mut dyn Project,
    path: &str,
    text: &str,
    offset: usize,
    target_ns: &str,
    target_dir: &str,
    comments: bool,
) -> Result<ProjectPlan, String> {
    let (path, text, info) = at_declaration(p, path, text, offset, |t, o| {
        class_in_header(t, o)
            .ok_or_else(|| cannot("Caret should be positioned at the class to be moved"))
    })?;
    let (path, text) = (path.as_str(), text.as_str());
    check_namespace(target_ns).map_err(cannot)?;
    let ns = target_ns.trim().trim_matches('\\');
    let new_fqn = qualify(ns, &info.name);
    let new_path = class_file_path(target_dir, &info.name);
    let same_ns = new_fqn.eq_ignore_ascii_case(&info.fqn);
    if same_ns && (new_path == path || !info.sole) {
        let where_ = if ns.is_empty() {
            "the global namespace".to_string()
        } else {
            format!("namespace {ns}")
        };
        return Err(cannot(format!(
            "Class {} is already in {where_}",
            info.name
        )));
    }
    let moves = [ClassMove::new(info.fqn.clone(), new_fqn.clone())];
    let mut plan = ProjectPlan::default();
    if !same_ns {
        for h in p.search(&info.name, SearchKind::Class) {
            if h.fqn.eq_ignore_ascii_case(&new_fqn) {
                plan.conflict(
                    format!("Class {new_fqn} already exists"),
                    Some(&h.path),
                    Some(h.line),
                );
            }
        }
    }
    let found = p.usages(path, text, info.name_span.start, &usage_options(comments));
    let spans: Vec<(String, Span, usize)> = match &found {
        Some(f) => usage_spans(p, f)
            .into_iter()
            .map(|(a, b, _)| (a, b, 0))
            .collect(),
        None => vec![],
    };
    let mut by_file = group(spans);
    let own: Vec<(Span, usize)> = match by_file.iter().position(|(f, _)| f == path) {
        Some(i) => by_file.remove(i).1,
        None => vec![],
    };
    if new_path != path && p.read(&new_path).is_some() {
        plan.conflict(
            format!("File {new_path} already exists"),
            Some(&new_path),
            None,
        );
    }
    if info.sole {
        plan.edit(path, file_edits(text, path, &moves, &own, Some(ns))?);
        if new_path != path {
            plan.ops.push(FileOp::Rename {
                from: path.to_string(),
                to: new_path.clone(),
            });
        }
    } else {
        let extracted = extract_class_file(text, &info);
        let refs: Vec<(Span, usize)> = fq_references(&extracted, &info.fqn)
            .into_iter()
            .map(|s| (s, 0))
            .collect();
        let new_text = moved_text(&extracted, &new_path, &moves, &refs, Some(ns))?;
        let outside: Vec<(Span, usize)> = own
            .into_iter()
            .filter(|(s, _)| !info.span.contains(*s))
            .collect();
        let mut edits = file_edits(text, path, &moves, &outside, None)?;
        edits.push(remove_class(text, &info));
        plan.edit(path, super::classes::tidy(edits));
        plan.ops.push(FileOp::Create {
            path: new_path,
            text: new_text,
        });
    }
    for (file, spans) in by_file {
        let Some(t) = p.read(&file) else { continue };
        match file_edits(&t, &file, &moves, &spans, None) {
            Ok(edits) => plan.edit(&file, edits),
            Err(e) => plan.conflict(format!("{file} is not updated: {e}"), Some(&file), None),
        }
    }
    Ok(plan)
}

/// Copy Class (F5): a copy of the class at `offset` named `new_name` in `target_ns`, in
/// a new file in `target_dir`.
pub fn copy_class(
    p: &mut dyn Project,
    path: &str,
    text: &str,
    offset: usize,
    new_name: &str,
    target_ns: &str,
    target_dir: &str,
) -> Result<ProjectPlan, String> {
    let (_, text, info) = at_declaration(p, path, text, offset, |t, o| {
        class_in_header(t, o)
            .ok_or_else(|| cannot("Caret should be positioned at the class to be copied"))
    })?;
    let ns = target_ns.trim().trim_matches('\\');
    let new_name = new_name.trim();
    let copy = super::move_class::copy_class_text(&text, &info, new_name, ns).map_err(|e| {
        if e.starts_with(super::CANNOT) {
            e
        } else {
            cannot(e)
        }
    })?;
    let new_path = class_file_path(target_dir, new_name);
    let mut plan = ProjectPlan::default();
    if p.read(&new_path).is_some() {
        return Err(cannot(format!("File {new_path} already exists")));
    }
    let new_fqn = qualify(ns, new_name);
    for h in p.search(new_name, SearchKind::Class) {
        if h.fqn.eq_ignore_ascii_case(&new_fqn) {
            plan.conflict(
                format!("Class {new_fqn} already exists"),
                Some(&h.path),
                Some(h.line),
            );
        }
    }
    plan.ops.push(FileOp::Create {
        path: new_path,
        text: copy,
    });
    Ok(plan)
}

// ---- Moving files (Project view) ----

/// A file that moves (every file inside a moved folder is listed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileMove {
    pub from: String,
    pub to: String,
}

fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

fn is_php(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    (p.ends_with(".php") || p.ends_with(".phtml") || p.ends_with(".inc"))
        && !p.ends_with(".blade.php")
}

/// Moving files and folders in the Project view (F6, drag and drop): PHP files that follow
/// PSR-4 (`namespace_for_dir` of their folder is their namespace) get the namespace of
/// their new folder, and the references to their classes are updated. The edits are for
/// the paths after the move (the files themselves are moved by the caller).
pub fn move_files(
    p: &mut dyn Project,
    files: &[FileMove],
    namespace_for_dir: &dyn Fn(&str) -> Option<String>,
    comments: bool,
) -> Result<ProjectPlan, String> {
    let mut plan = ProjectPlan::default();
    let new_path = |old: &str| -> String {
        files
            .iter()
            .find(|f| f.from == old)
            .map_or_else(|| old.to_string(), |f| f.to.clone())
    };
    // The files whose namespace changes, and the classes that move with them.
    let mut new_ns: HashMap<String, String> = HashMap::new();
    let mut moves: Vec<ClassMove> = Vec::new();
    let mut decls: Vec<(String, String, Span)> = Vec::new();
    for f in files.iter().filter(|f| is_php(&f.from)) {
        let Some(text) = p.read(&f.from) else {
            continue;
        };
        let classes = classes_in(&text);
        let Some(tree) = super::parse(&text) else {
            continue;
        };
        let ns = super::move_class::file_namespace(tree.root_node(), &text);
        let (Some(old_dir_ns), Some(target)) = (
            namespace_for_dir(dir_of(&f.from)),
            namespace_for_dir(dir_of(&f.to)),
        ) else {
            continue;
        };
        if !old_dir_ns.eq_ignore_ascii_case(&ns) || target.eq_ignore_ascii_case(&ns) {
            continue;
        }
        new_ns.insert(f.from.clone(), target.clone());
        for c in classes {
            decls.push((f.from.clone(), text.clone(), c.name_span));
            moves.push(ClassMove::new(c.fqn.clone(), qualify(&target, &c.name)));
        }
    }
    let mut spans: Vec<(String, Span, usize)> = Vec::new();
    for (i, (path, text, name)) in decls.iter().enumerate() {
        if let Some(found) = p.usages(path, text, name.start, &usage_options(comments)) {
            for (file, span, _) in usage_spans(p, &found) {
                // Its own declaration file renames itself through the namespace.
                spans.push((file, span, i));
            }
        }
    }
    let mut by_file = group(spans);
    for path in new_ns.keys() {
        if !by_file.iter().any(|(f, _)| f == path) {
            by_file.push((path.clone(), vec![]));
        }
    }
    by_file.sort_by(|a, b| a.0.cmp(&b.0));
    for (file, spans) in by_file {
        let Some(text) = p.read(&file) else { continue };
        let ns = new_ns.get(&file).map(String::as_str);
        let target = new_path(&file);
        match file_edits(&text, &file, &moves, &spans, ns) {
            Ok(edits) => plan.edit(&target, edits),
            Err(e) => plan.conflict(format!("{file} is not updated: {e}"), Some(&file), None),
        }
    }
    for f in files {
        if f.from != f.to && p.read(&f.to).is_some() && !files.iter().any(|g| g.from == f.to) {
            plan.conflict(format!("File {} already exists", f.to), Some(&f.to), None);
        }
    }
    Ok(plan)
}

// ---- Safe Delete ----

/// Safe Delete: the element, the usages that keep it alive and the plan that deletes it
/// ("Delete Anyway" applies the same plan).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafeDeletePlan {
    pub target: DeleteTarget,
    /// The file that declares it.
    pub path: String,
    /// Usages that are not safe to delete.
    pub usages: Vec<UsageLocation>,
    /// How many of them are in comments and strings.
    pub in_comments: usize,
    /// "class User has 2 usages that are not safe to delete. ..."
    pub message: String,
    pub plan: ProjectPlan,
}

/// Safe Delete (Alt+Delete) of the element at `offset`.
pub fn safe_delete(
    p: &mut dyn Project,
    path: &str,
    text: &str,
    offset: usize,
    comments: bool,
) -> Result<SafeDeletePlan, String> {
    let (decl_path, decl_text, target) =
        at_declaration(p, path, text, offset, safe_delete::target_at)?;
    let mut plan = ProjectPlan::default();
    let mut blocking: Vec<UsageLocation> = Vec::new();
    let lines = crate::navigate::Lines::new(&decl_text);
    if let Some(index) = target.parameter {
        let info = safe_delete::parameter_signature(&decl_text, &target)
            .ok_or_else(|| cannot("The function cannot be found"))?;
        let options = signature::without_parameter(&info, index);
        for s in &target.body_usages {
            blocking.push(UsageLocation {
                path: decl_path.clone(),
                range: lines.range(s.start, s.end),
                kind: None,
            });
        }
        signature_plan(p, &mut plan, &decl_path, &decl_text, &info, &options, false)?;
        // The problem is the usage list here, not a conflict.
        plan.conflicts
            .retain(|c| !c.message.starts_with("Parameter $"));
    } else {
        let options = usage_options(comments);
        if let Some(found) = p.usages(&decl_path, &decl_text, target.name_span.start, &options) {
            for u in found.usages {
                if is_vendor(&u.path) || u.path.contains("://") {
                    continue;
                }
                if found
                    .declaration
                    .as_ref()
                    .is_some_and(|d| d.path == u.path && d.range == u.range)
                {
                    continue;
                }
                if u.path == decl_path
                    && safe_delete::is_inside(&target, span_of(&decl_text, &u.range))
                {
                    continue;
                }
                blocking.push(u);
            }
        }
        if target.delete_file {
            plan.ops.push(FileOp::Delete {
                path: decl_path.clone(),
            });
        } else {
            plan.edit(&decl_path, safe_delete::delete_edits(&decl_text, &target));
        }
    }
    let in_comments = blocking
        .iter()
        .filter(|u| matches!(u.kind, Some(UsageKind::Comment | UsageKind::StringLiteral)))
        .count();
    Ok(SafeDeletePlan {
        message: safe_delete::usages_message(&target, blocking.len(), in_comments),
        target,
        path: decl_path,
        usages: blocking,
        in_comments,
        plan,
    })
}

/// Safe Delete of files and folders in the Project view: the usages (outside the deleted
/// files) of the classes they declare. `files` lists every file (folders expanded).
pub fn safe_delete_files(
    p: &mut dyn Project,
    files: &[String],
    comments: bool,
) -> Vec<UsageLocation> {
    let mut out = Vec::new();
    for f in files.iter().filter(|f| is_php(f)) {
        let Some(text) = p.read(f) else { continue };
        for c in classes_in(&text) {
            let Some(found) = p.usages(f, &text, c.name_span.start, &usage_options(comments))
            else {
                continue;
            };
            for u in found.usages {
                if files.contains(&u.path) || is_vendor(&u.path) || u.path.contains("://") {
                    continue;
                }
                out.push(u);
            }
        }
    }
    out
}

// ---- Change Signature ----

/// The function to change: the declaration whose header the caret is in, the function a
/// call at the caret calls, else the function around the caret.
fn signature_target(
    p: &mut dyn Project,
    path: &str,
    text: &str,
    offset: usize,
) -> Result<(String, String, SignatureInfo), String> {
    match at_declaration(p, path, text, offset, signature::signature_in_header) {
        Ok(found) => Ok(found),
        Err(e) => signature::signature_at(text, offset)
            .map(|i| (path.to_string(), text.to_string(), i))
            .map_err(|_| e),
    }
}

/// The signature at the caret (or of the function the call at the caret calls).
pub fn signature_at(
    p: &mut dyn Project,
    path: &str,
    text: &str,
    offset: usize,
) -> Result<(String, SignatureInfo), String> {
    let (decl_path, _, info) = signature_target(p, path, text, offset)?;
    Ok((decl_path, info))
}

/// Change Signature (Ctrl+F6) of the method or function at `offset`.
pub fn change_signature(
    p: &mut dyn Project,
    path: &str,
    text: &str,
    offset: usize,
    options: &ChangeSignatureOptions,
) -> Result<ProjectPlan, String> {
    let (decl_path, decl_text, info) = signature_target(p, path, text, offset)?;
    signature::check(&info, options)?;
    let mut plan = ProjectPlan::default();
    signature_plan(p, &mut plan, &decl_path, &decl_text, &info, options, true)?;
    Ok(plan)
}

/// The declaration, the overriding methods and the call sites of a signature change.
fn signature_plan(
    p: &mut dyn Project,
    plan: &mut ProjectPlan,
    decl_path: &str,
    decl_text: &str,
    info: &SignatureInfo,
    options: &ChangeSignatureOptions,
    own_visibility: bool,
) -> Result<(), String> {
    let (edits, problems) = signature::declaration_edits(decl_text, info, options, own_visibility)?;
    for m in problems {
        plan.conflict(
            m,
            Some(decl_path),
            Some(line_of(decl_text, info.name_span.start)),
        );
    }
    plan.edit(decl_path, edits);
    let usage_opts = UsageOptions {
        method_overriding_methods: true,
        ..UsageOptions::default()
    };
    let found = p.usages(decl_path, decl_text, info.name_span.start, &usage_opts);
    let mut sites: Vec<(String, Span, Option<UsageKind>)> = match &found {
        Some(f) => usage_spans(p, f),
        None => vec![],
    };
    if info.constructor {
        // `new Foo(...)`: the class's usages.
        if let Some(class) = super::move_class::class_at(decl_text, info.name_span.start) {
            if let Some(f) = p.usages(
                decl_path,
                decl_text,
                class.name_span.start,
                &UsageOptions::default(),
            ) {
                sites.extend(usage_spans(p, &f));
            }
        }
    }
    // A new name that is taken.
    if options.name.trim() != info.name {
        if let Some(found) = &found {
            let hits = p.search(options.name.trim(), SearchKind::Symbol);
            for c in crate::rename::clashes(&found.symbol, options.name.trim(), &hits) {
                plan.conflicts.push(c);
            }
        }
    }
    let mut texts: HashMap<String, String> = HashMap::new();
    for (file, span, kind) in sites {
        let text = match texts.get(&file) {
            Some(t) => t.clone(),
            None => {
                let Some(t) = p.read(&file) else { continue };
                texts.insert(file.clone(), t.clone());
                t
            }
        };
        if matches!(
            kind,
            Some(UsageKind::OverridingMethod | UsageKind::ImplementingMethod)
        ) {
            let Some(sub) = signature::signature_of(&text, span) else {
                continue;
            };
            if sub.parameters.len() != info.parameters.len() {
                plan.conflict(
                    format!(
                        "Method {}::{}() has a different parameter list and is not updated",
                        sub.class_name.as_deref().unwrap_or("?"),
                        sub.name
                    ),
                    Some(&file),
                    Some(line_of(&text, span.start)),
                );
                continue;
            }
            let (edits, problems) = signature::declaration_edits(&text, &sub, options, false)?;
            for m in problems {
                plan.conflict(m, Some(&file), Some(line_of(&text, span.start)));
            }
            plan.edit(&file, edits);
            continue;
        }
        match signature::call_edits(&text, span, info, options) {
            Ok(edits) => plan.edit(&file, edits),
            Err(e) => {
                let c = conflict_at(e, &file, &text, span.start);
                plan.conflicts.push(c);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::generate::apply_edits;
    use crate::{IndexConfig, ProjectIndex};

    /// A project on disk with its PHP index.
    pub(crate) struct TestProject {
        pub dir: tempfile::TempDir,
        pub index: ProjectIndex,
    }

    impl TestProject {
        pub(crate) fn new(files: &[(&str, &str)]) -> Self {
            let dir = tempfile::tempdir().unwrap();
            for (path, text) in files {
                let p = dir.path().join(path);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, text).unwrap();
            }
            let config = IndexConfig {
                cache_file: Some(dir.path().join(".cache.json")),
                ..IndexConfig::default()
            };
            let mut index = ProjectIndex::new(dir.path(), config);
            index.index_workspace(|_, _| {});
            Self { dir, index }
        }

        pub(crate) fn text(&self, path: &str) -> String {
            std::fs::read_to_string(self.dir.path().join(path)).unwrap()
        }

        /// The texts after applying `plan` (path -> text, `None` = deleted).
        pub(crate) fn apply(&self, plan: &ProjectPlan) -> HashMap<String, Option<String>> {
            let mut out: HashMap<String, Option<String>> = HashMap::new();
            for op in &plan.ops {
                match op {
                    FileOp::Edit { path, edits } => {
                        let t = out
                            .get(path)
                            .cloned()
                            .flatten()
                            .unwrap_or_else(|| self.text(path));
                        out.insert(path.clone(), Some(apply_edits(&t, edits)));
                    }
                    FileOp::Create { path, text } => {
                        out.insert(path.clone(), Some(text.clone()));
                    }
                    FileOp::Rename { from, to } => {
                        let t = out
                            .get(from)
                            .cloned()
                            .flatten()
                            .unwrap_or_else(|| self.text(from));
                        out.insert(from.clone(), None);
                        out.insert(to.clone(), Some(t));
                    }
                    FileOp::Delete { path } => {
                        out.insert(path.clone(), None);
                    }
                }
            }
            out
        }
    }

    impl Project for TestProject {
        fn usages(
            &mut self,
            path: &str,
            text: &str,
            offset: usize,
            options: &UsageOptions,
        ) -> Option<PhpUsages> {
            crate::navigate::find_usages_with(&mut self.index, path, text, offset, options)
        }
        fn read(&mut self, path: &str) -> Option<String> {
            std::fs::read_to_string(self.dir.path().join(path)).ok()
        }
        fn definition(&mut self, path: &str, text: &str, offset: usize) -> Option<(String, usize)> {
            let d = crate::navigate::definition(&mut self.index, path, text, offset);
            let t = d.targets.first()?;
            let target = self.read(&t.path)?;
            let at = byte_of(&target, Position::new(t.line, t.column));
            Some((t.path.clone(), at))
        }
        fn search(&mut self, name: &str, kind: SearchKind) -> Vec<SymbolHit> {
            self.index.search_with(name, kind, 50, true)
        }
    }

    const COMPOSER: &str = r#"{"autoload": {"psr-4": {"App\\": "app/"}}}"#;

    const USER: &str = "<?php\n\nnamespace App\\Models;\n\nclass User\n{\n    public function orders(): array\n    {\n        return [new Order()];\n    }\n\n    public function name(string $prefix, bool $upper = false): string\n    {\n        return $prefix;\n    }\n}\n";
    const ORDER: &str = "<?php\n\nnamespace App\\Models;\n\nclass Order\n{\n    public function owner(): User\n    {\n        return new User();\n    }\n}\n";
    const CONTROLLER: &str = "<?php\n\nnamespace App\\Http;\n\nuse App\\Models\\User;\n\nclass UserController\n{\n    public function show(User $user): string\n    {\n        return $user->name('Dr. ') . \\App\\Models\\User::class;\n    }\n}\n";

    fn project() -> TestProject {
        TestProject::new(&[
            ("composer.json", COMPOSER),
            ("app/Models/User.php", USER),
            ("app/Models/Order.php", ORDER),
            ("app/Http/UserController.php", CONTROLLER),
        ])
    }

    #[test]
    fn moves_a_class_to_another_namespace() {
        let mut p = project();
        let at = USER.find("User").unwrap();
        let plan = move_class(
            &mut p,
            "app/Models/User.php",
            USER,
            at,
            "App\\Domain",
            "app/Domain",
            false,
        )
        .unwrap();
        assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
        let out = p.apply(&plan);
        assert_eq!(out["app/Models/User.php"], None);
        let moved = out["app/Domain/User.php"].clone().unwrap();
        assert!(moved.contains("namespace App\\Domain;\n\nuse App\\Models\\Order;\n\nclass User"));
        let order = out["app/Models/Order.php"].clone().unwrap();
        assert!(order.contains("namespace App\\Models;\n\nuse App\\Domain\\User;\n\nclass Order"));
        let controller = out["app/Http/UserController.php"].clone().unwrap();
        assert!(controller.contains("use App\\Domain\\User;"));
        assert!(controller.contains("\\App\\Domain\\User::class"));
        // Already there.
        assert!(move_class(
            &mut p,
            "app/Models/User.php",
            USER,
            at,
            "App\\Models",
            "app/Models",
            false
        )
        .is_err());
        // From a reference in another file.
        let at = CONTROLLER.find("User $user").unwrap();
        let plan = move_class(
            &mut p,
            "app/Http/UserController.php",
            CONTROLLER,
            at,
            "App\\Domain",
            "app/Domain",
            false,
        )
        .unwrap();
        assert!(p.apply(&plan).contains_key("app/Domain/User.php"));
    }

    #[test]
    fn reports_existing_classes_and_files() {
        let mut p = TestProject::new(&[
            ("app/Models/User.php", USER),
            (
                "app/Domain/User.php",
                "<?php\nnamespace App\\Domain;\nclass User {}\n",
            ),
        ]);
        let plan = move_class(
            &mut p,
            "app/Models/User.php",
            USER,
            USER.find("User").unwrap(),
            "App\\Domain",
            "app/Domain",
            false,
        )
        .unwrap();
        let messages: Vec<&str> = plan.conflicts.iter().map(|c| c.message.as_str()).collect();
        assert!(messages.contains(&"Class App\\Domain\\User already exists"));
        assert!(messages.contains(&"File app/Domain/User.php already exists"));
    }

    #[test]
    fn copies_a_class() {
        let mut p = project();
        let plan = copy_class(
            &mut p,
            "app/Models/User.php",
            USER,
            USER.find("User").unwrap(),
            "Customer",
            "App\\Billing",
            "app/Billing",
        )
        .unwrap();
        let out = p.apply(&plan);
        let copy = out["app/Billing/Customer.php"].clone().unwrap();
        assert!(
            copy.contains("namespace App\\Billing;\n\nuse App\\Models\\Order;\n\nclass Customer")
        );
        assert!(copy_class(
            &mut p,
            "app/Models/User.php",
            USER,
            USER.find("User").unwrap(),
            "Order",
            "App\\Models",
            "app/Models"
        )
        .is_err());
    }

    #[test]
    fn moves_files_with_their_namespace() {
        let mut p = project();
        let roots = forge_file_templates::psr4::roots(COMPOSER, "");
        let ns = |d: &str| forge_file_templates::psr4::namespace_for_dir(&roots, d);
        let files = vec![
            FileMove {
                from: "app/Models/User.php".into(),
                to: "app/Domain/Models/User.php".into(),
            },
            FileMove {
                from: "app/Models/Order.php".into(),
                to: "app/Domain/Models/Order.php".into(),
            },
        ];
        let plan = move_files(&mut p, &files, &ns, false).unwrap();
        // Edits are for the new paths; the classes moving together need no imports.
        let user = apply_edits(USER, &plan.edits_of("app/Domain/Models/User.php"));
        assert!(user.starts_with("<?php\n\nnamespace App\\Domain\\Models;\n\nclass User"));
        let order = apply_edits(ORDER, &plan.edits_of("app/Domain/Models/Order.php"));
        assert!(order.starts_with("<?php\n\nnamespace App\\Domain\\Models;\n\nclass Order"));
        let controller = apply_edits(CONTROLLER, &plan.edits_of("app/Http/UserController.php"));
        assert!(controller.contains("use App\\Domain\\Models\\User;"));
        assert!(controller.contains("\\App\\Domain\\Models\\User::class"));
    }

    #[test]
    fn safe_deletes_unused_and_reports_used_elements() {
        let mut p = project();
        // `orders()` is not called anywhere.
        let r = safe_delete(
            &mut p,
            "app/Models/User.php",
            USER,
            USER.find("orders").unwrap(),
            false,
        )
        .unwrap();
        assert!(r.usages.is_empty());
        let out = p.apply(&r.plan);
        assert!(!out["app/Models/User.php"]
            .clone()
            .unwrap()
            .contains("orders"));
        // `name()` is called by the controller.
        let r = safe_delete(
            &mut p,
            "app/Models/User.php",
            USER,
            USER.find("name(").unwrap(),
            false,
        )
        .unwrap();
        assert_eq!(r.usages.len(), 1);
        assert_eq!(r.usages[0].path, "app/Http/UserController.php");
        assert!(r
            .message
            .starts_with("method name() has 1 usage that is not safe to delete."));
        // The class: its file goes.
        let r = safe_delete(
            &mut p,
            "app/Models/Order.php",
            ORDER,
            ORDER.find("Order").unwrap(),
            false,
        )
        .unwrap();
        assert_eq!(r.usages.len(), 1);
        assert_eq!(
            r.plan.ops,
            vec![FileOp::Delete {
                path: "app/Models/Order.php".into()
            }]
        );
        // Files from the Project view.
        let u = safe_delete_files(
            &mut p,
            &["app/Models/User.php".into(), "app/Models/Order.php".into()],
            false,
        );
        assert_eq!(
            u.iter()
                .filter(|u| u.path == "app/Http/UserController.php")
                .count(),
            3
        );
    }

    #[test]
    fn safe_deletes_a_parameter_with_its_arguments() {
        let mut p = project();
        let r = safe_delete(
            &mut p,
            "app/Models/User.php",
            USER,
            USER.find("$upper").unwrap(),
            false,
        )
        .unwrap();
        assert!(r.usages.is_empty());
        let out = p.apply(&r.plan);
        assert!(out["app/Models/User.php"]
            .clone()
            .unwrap()
            .contains("name(string $prefix): string"));
        // The used one: its body usage is listed.
        let r = safe_delete(
            &mut p,
            "app/Models/User.php",
            USER,
            USER.find("$prefix").unwrap(),
            false,
        )
        .unwrap();
        assert_eq!(r.usages.len(), 1);
        let out = p.apply(&r.plan);
        assert!(out["app/Http/UserController.php"]
            .clone()
            .unwrap()
            .contains("$user->name()"));
    }

    #[test]
    fn changes_a_signature_across_files() {
        let mut p = project();
        let (_, info) = signature_at(
            &mut p,
            "app/Models/User.php",
            USER,
            USER.find("name(").unwrap(),
        )
        .unwrap();
        let mut o = signature::unchanged(&info);
        o.name = "title".into();
        o.parameters.reverse();
        o.parameters[0].default_value = None;
        o.parameters[0].call_value = None;
        let plan = change_signature(
            &mut p,
            "app/Models/User.php",
            USER,
            USER.find("name(").unwrap(),
            &o,
        );
        // `upper` has no value at the call site once it is required.
        let plan = plan.unwrap();
        assert_eq!(plan.conflicts.len(), 1);
        let mut o = signature::unchanged(&info);
        o.name = "title".into();
        o.parameters.reverse();
        let plan = change_signature(
            &mut p,
            "app/Http/UserController.php",
            CONTROLLER,
            CONTROLLER.find("name(").unwrap(),
            &o,
        )
        .unwrap();
        assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
        let out = p.apply(&plan);
        assert!(out["app/Models/User.php"]
            .clone()
            .unwrap()
            .contains("public function title(bool $upper = false, string $prefix): string"));
        assert!(out["app/Http/UserController.php"]
            .clone()
            .unwrap()
            .contains("$user->title(prefix: 'Dr. ')"));
    }
}
