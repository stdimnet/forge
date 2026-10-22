//! Move Class (F6) and Copy Class (F5) like PHPStorm, and the class part of moving PHP
//! files in the Project view:
//!
//! - [`class_at`]: the class the caret is on (its declaration).
//! - [`file_edits`]: the edits of one file when classes move to other namespaces: in a
//!   moved file the namespace declaration and imports for the classes of its old namespace
//!   (they no longer resolve by their short name), in every file the `use` imports, fully
//!   qualified names, PHPDoc types and (optionally) comments and strings naming a moved
//!   class, and the imports bare names now need.
//! - [`extract_class_file`] / [`copy_class_text`]: the text of a new file for a class moved
//!   out of a file with other declarations, or copied under a new name.
//!
//! The usages come from Find Usages (the PHP index); everything here is pure and works on
//! byte offsets.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use super::classes::{
    add_imports, all_doc_class_names, code_class_names, imported, namespace_of, qualify, tidy,
    ClassName,
};
use super::{cannot, parse, text, Span};
use crate::generate::{apply_edits, TextEdit};
use crate::model::{short_name, FileIndex, UseKind};
use crate::parser::index_source;

/// A class that moves from `old_fqn` to `new_fqn`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassMove {
    pub old_fqn: String,
    pub new_fqn: String,
}

impl ClassMove {
    pub fn new(old_fqn: impl Into<String>, new_fqn: impl Into<String>) -> Self {
        Self {
            old_fqn: old_fqn.into().trim_start_matches('\\').to_string(),
            new_fqn: new_fqn.into().trim_start_matches('\\').to_string(),
        }
    }
}

/// Where moved classes go, by their old name (case-insensitive like PHP).
struct Moves<'a>(&'a [ClassMove]);

impl Moves<'_> {
    fn get(&self, old: &str) -> Option<&ClassMove> {
        let old = old.trim_start_matches('\\');
        self.0.iter().find(|m| m.old_fqn.eq_ignore_ascii_case(old))
    }
}

/// The class declaration the caret is on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassInfo {
    pub name: String,
    pub fqn: String,
    pub namespace: String,
    /// `class`, `interface`, `trait` or `enum`.
    pub kind: String,
    pub name_span: Span,
    /// The declaration with its PHPDoc and attributes.
    pub span: Span,
    /// The only declaration in its file (the file moves with it).
    pub sole: bool,
}

fn class_kind(n: Node) -> Option<&'static str> {
    Some(match n.kind() {
        "class_declaration" => "class",
        "interface_declaration" => "interface",
        "trait_declaration" => "trait",
        "enum_declaration" => "enum",
        _ => return None,
    })
}

/// The namespace of the file (the first `namespace` statement).
pub fn file_namespace(root: Node, src: &str) -> String {
    let mut c = root.walk();
    let ns = root
        .named_children(&mut c)
        .find(|n| n.kind() == "namespace_definition")
        .and_then(|n| n.child_by_field_name("name"))
        .map(|n| text(n, src).trim_matches('\\').to_string());
    ns.unwrap_or_default()
}

/// Top-level declarations (classes, functions, constants, statements) of the file.
fn top_level<'t>(root: Node<'t>) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    let mut c = root.walk();
    for n in root.named_children(&mut c) {
        match n.kind() {
            "php_tag"
            | "comment"
            | "namespace_use_declaration"
            | "declare_statement"
            | "text_interpolation" => {}
            "namespace_definition" => {
                if let Some(body) = n.child_by_field_name("body") {
                    out.extend(top_level(body));
                }
            }
            _ => out.push(n),
        }
    }
    out
}

/// The declaration with the PHPDoc comment right before it.
pub(crate) fn with_doc(n: Node, src: &str) -> Span {
    let mut start = n.start_byte();
    let mut prev = n.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "comment"
            && text(p, src).starts_with("/**")
            && src[p.end_byte()..start].trim().is_empty()
        {
            start = p.start_byte();
            prev = p.prev_sibling();
        } else {
            break;
        }
    }
    Span::new(start, n.end_byte())
}

/// The class declaration at `pos`: on its name, its header or between its members.
pub fn class_at(src: &str, pos: usize) -> Option<ClassInfo> {
    let tree = parse(src)?;
    let root = tree.root_node();
    let mut n = root.descendant_for_byte_range(pos, pos)?;
    let class = loop {
        if class_kind(n).is_some() {
            break n;
        }
        n = n.parent()?;
    };
    info_of(class, root, src)
}

/// The class whose header (name, `extends`, `implements`) the caret is in.
pub fn class_in_header(src: &str, pos: usize) -> Option<ClassInfo> {
    let tree = parse(src)?;
    let root = tree.root_node();
    let mut n = root.descendant_for_byte_range(pos, pos)?;
    let class = loop {
        if class_kind(n).is_some() {
            break n;
        }
        n = n.parent()?;
    };
    if class
        .child_by_field_name("body")
        .is_some_and(|b| pos > b.start_byte())
    {
        return None;
    }
    info_of(class, root, src)
}

/// The class declarations of a file.
pub fn classes_in(src: &str) -> Vec<ClassInfo> {
    let Some(tree) = parse(src) else {
        return vec![];
    };
    let root = tree.root_node();
    top_level(root)
        .into_iter()
        .filter(|n| class_kind(*n).is_some())
        .filter_map(|n| info_of(n, root, src))
        .collect()
}

fn info_of(class: Node, root: Node, src: &str) -> Option<ClassInfo> {
    let name_node = class.child_by_field_name("name")?;
    let name = text(name_node, src).to_string();
    // A class inside a braced namespace: that namespace.
    let mut namespace = file_namespace(root, src);
    let mut p = class.parent();
    while let Some(x) = p {
        if x.kind() == "namespace_definition" {
            namespace = x
                .child_by_field_name("name")
                .map(|n| text(n, src).trim_matches('\\').to_string())
                .unwrap_or_default();
        }
        p = x.parent();
    }
    let sole = top_level(root).len() == 1;
    Some(ClassInfo {
        fqn: qualify(&namespace, &name),
        namespace,
        kind: class_kind(class)?.to_string(),
        name_span: Span::of(name_node),
        span: with_doc(class, src),
        name,
        sole,
    })
}

/// A valid namespace (`App\Billing`, or empty for the global one).
pub fn check_namespace(ns: &str) -> Result<(), String> {
    let ns = ns.trim().trim_matches('\\');
    if ns.is_empty() {
        return Ok(());
    }
    for seg in ns.split('\\') {
        if !super::is_identifier(seg) {
            return Err(format!("'{ns}' is not a valid namespace name"));
        }
    }
    Ok(())
}

/// The edits putting the file into `new_ns`: the namespace statement, imports for the
/// classes of the old namespace it names by their short name (or their new names when they
/// move too) and fully qualified names for relative ones. Returns the imports to add.
fn retarget(
    root: Node,
    src: &str,
    file: &FileIndex,
    new_ns: &str,
    moves: &Moves,
) -> Result<(Vec<TextEdit>, Vec<String>), String> {
    let mut edits = Vec::new();
    let mut imports = Vec::new();
    let mut c = root.walk();
    let defs: Vec<Node> = root
        .named_children(&mut c)
        .filter(|n| n.kind() == "namespace_definition")
        .collect();
    if defs.len() > 1 {
        return Err(cannot("The file declares several namespaces"));
    }
    let old_ns = file_namespace(root, src);
    if old_ns.eq_ignore_ascii_case(new_ns) {
        return Ok((edits, imports));
    }
    match defs.first() {
        Some(def) => {
            let braced = def.child_by_field_name("body").is_some();
            match def.child_by_field_name("name") {
                Some(name) if !new_ns.is_empty() => edits.push(TextEdit {
                    start: name.start_byte(),
                    end: name.end_byte(),
                    new_text: new_ns.to_string(),
                }),
                Some(name) if braced => edits.push(TextEdit {
                    // `namespace App { ... }` -> `namespace { ... }`
                    start: def.start_byte() + "namespace".len(),
                    end: name.end_byte(),
                    new_text: String::new(),
                }),
                Some(_) => {
                    let (s, e, t) =
                        crate::imports::remove_lines(src, def.start_byte(), def.end_byte());
                    edits.push(TextEdit {
                        start: s,
                        end: e,
                        new_text: t,
                    });
                }
                None => edits.push(TextEdit {
                    start: def.start_byte() + "namespace".len(),
                    end: def.start_byte() + "namespace".len(),
                    new_text: format!(" {new_ns}"),
                }),
            }
        }
        None => {
            // After `declare(strict_types=1);`, else after the opening tag's line.
            let mut c = root.walk();
            let declare = root
                .named_children(&mut c)
                .find(|n| n.kind() == "declare_statement");
            let at = match declare {
                Some(d) => d.end_byte(),
                None => src.find('\n').unwrap_or(src.len()),
            };
            edits.push(TextEdit {
                start: at,
                end: at,
                new_text: format!("\n\nnamespace {new_ns};"),
            });
        }
    }
    let declared: Vec<String> = file
        .classes
        .iter()
        .map(|c| c.name.to_ascii_lowercase())
        .collect();
    let mut names: Vec<ClassName> = code_class_names(root, src);
    names.extend(all_doc_class_names(root, src));
    for n in names {
        if n.is_fully_qualified() || n.text.starts_with("namespace\\") || imported(file, &n.text) {
            continue;
        }
        let old = qualify(&old_ns, &n.text);
        let target = moves.get(&old).map_or(old.clone(), |m| m.new_fqn.clone());
        if n.is_bare() {
            if declared.contains(&n.text.to_ascii_lowercase()) {
                continue;
            }
            if !namespace_of(&target).eq_ignore_ascii_case(new_ns) {
                // A class of the same name in the new namespace would win otherwise.
                if short_name(&target).eq_ignore_ascii_case(&n.text) {
                    imports.push(target);
                } else {
                    edits.push(TextEdit {
                        start: n.span.start,
                        end: n.span.end,
                        new_text: format!("\\{target}"),
                    });
                }
            } else if !short_name(&target).eq_ignore_ascii_case(&n.text) {
                edits.push(TextEdit {
                    start: n.span.start,
                    end: n.span.end,
                    new_text: short_name(&target).to_string(),
                });
            }
        } else {
            // `Sub\Foo` was relative to the old namespace.
            edits.push(TextEdit {
                start: n.span.start,
                end: n.span.end,
                new_text: format!("\\{target}"),
            });
        }
    }
    Ok((edits, imports))
}

/// The `qualified_name` or `name` node that holds the class name at `span`.
fn name_node_at(root: Node, span: Span) -> Option<Node> {
    let mut n = root.descendant_for_byte_range(span.start, span.end)?;
    if n.kind() == "name" {
        if let Some(p) = n.parent().filter(|p| p.kind() == "qualified_name") {
            n = p;
        }
    }
    matches!(n.kind(), "name" | "qualified_name").then_some(n)
}

/// The enclosing node of `kind` within a few levels.
fn ancestor<'t>(n: Node<'t>, kind: &str, levels: usize) -> Option<Node<'t>> {
    let mut cur = n.parent();
    for _ in 0..levels {
        let p = cur?;
        if p.kind() == kind {
            return Some(p);
        }
        cur = p.parent();
    }
    None
}

/// A qualified word in a comment or string around `span`: `App\Models\User`, also with
/// doubled backslashes (`'App\\Models\\User'`).
fn qualified_word(src: &str, span: Span) -> Span {
    let is_char = |c: char| c.is_alphanumeric() || c == '_' || c == '\\';
    let mut start = span.start;
    while let Some(c) = src[..start].chars().next_back() {
        if !is_char(c) {
            break;
        }
        start -= c.len_utf8();
    }
    let mut end = span.end;
    while let Some(c) = src[end..].chars().next() {
        if !is_char(c) {
            break;
        }
        end += c.len_utf8();
    }
    Span::new(start, end)
}

/// One usage: rewritten in place, or an import the file needs.
#[allow(clippy::too_many_arguments)]
fn reference_edit(
    root: Node,
    src: &str,
    file: &FileIndex,
    span: Span,
    m: &ClassMove,
    final_ns: &str,
    moved_file: bool,
    edits: &mut Vec<TextEdit>,
    imports: &mut Vec<String>,
) {
    let node = name_node_at(root, span);
    let in_comment_or_string = node.is_none()
        || root
            .descendant_for_byte_range(span.start, span.end)
            .is_some_and(|n| {
                let mut cur = Some(n);
                while let Some(x) = cur {
                    if matches!(
                        x.kind(),
                        "comment" | "string" | "encapsed_string" | "heredoc" | "nowdoc"
                    ) {
                        return true;
                    }
                    cur = x.parent();
                }
                false
            });
    if in_comment_or_string {
        let word = qualified_word(src, span);
        let written = &src[word.start..word.end];
        let doubled = written.contains("\\\\");
        let plain = written.replace("\\\\", "\\");
        let fq = plain.starts_with('\\');
        let bare = plain.trim_start_matches('\\');
        let sep = if doubled { "\\\\" } else { "\\" };
        let render = |fqn: &str| {
            let t = fqn.replace('\\', sep);
            if fq {
                format!("{sep}{t}")
            } else {
                t
            }
        };
        if bare.eq_ignore_ascii_case(&m.old_fqn) {
            edits.push(TextEdit {
                start: word.start,
                end: word.end,
                new_text: render(&m.new_fqn),
            });
        } else if !bare.contains('\\') && !moved_file && !imported(file, bare) {
            // A PHPDoc type by its short name: it resolved through the namespace.
            let in_doc = root
                .descendant_for_byte_range(span.start, span.end)
                .is_some_and(|n| n.kind() == "comment" && text(n, src).starts_with("/**"));
            if in_doc && !namespace_of(&m.new_fqn).eq_ignore_ascii_case(final_ns) {
                imports.push(m.new_fqn.clone());
            }
        } else if bare.contains('\\')
            && file
                .resolve_class(bare, span.start)
                .eq_ignore_ascii_case(&m.old_fqn)
        {
            edits.push(TextEdit {
                start: word.start,
                end: word.end,
                new_text: format!("\\{}", m.new_fqn),
            });
        }
        return;
    }
    let Some(n) = node else { return };
    let written = text(n, src);
    // `use App\Models\User;` and `use App\Models\{User, Order};`
    if let Some(clause) = n
        .parent()
        .filter(|p| p.kind() == "namespace_use_clause")
        .filter(|p| p.named_child(0) == Some(n))
    {
        let group = ancestor(clause, "namespace_use_group", 1);
        let decl = ancestor(clause, "namespace_use_declaration", 3);
        let alias = clause.child_by_field_name("alias");
        if let Some(group) = group {
            let prefix = decl
                .and_then(|d| {
                    let mut c = d.walk();
                    let found = d
                        .named_children(&mut c)
                        .find(|x| x.kind() == "namespace_name");
                    found
                })
                .map(|p| text(p, src).trim_matches('\\').to_string())
                .unwrap_or_default();
            let lower_new = m.new_fqn.to_ascii_lowercase();
            let lower_prefix = format!("{}\\", prefix.to_ascii_lowercase());
            if lower_new.starts_with(&lower_prefix) {
                edits.push(TextEdit {
                    start: n.start_byte(),
                    end: n.end_byte(),
                    new_text: m.new_fqn[prefix.len() + 1..].to_string(),
                });
            } else {
                // Out of the group, into its own `use` statement.
                let mut c = group.walk();
                let items: Vec<Node> = group
                    .named_children(&mut c)
                    .filter(|x| x.kind() == "namespace_use_clause")
                    .collect();
                let i = items.iter().position(|x| *x == clause).unwrap_or(0);
                let (s, e) = if items.len() == 1 {
                    (clause.start_byte(), clause.end_byte())
                } else if i + 1 < items.len() {
                    (clause.start_byte(), items[i + 1].start_byte())
                } else {
                    (items[i - 1].end_byte(), clause.end_byte())
                };
                edits.push(TextEdit {
                    start: s,
                    end: e,
                    new_text: String::new(),
                });
                if let Some(d) = decl {
                    let line = match alias {
                        Some(a) => format!("\nuse {} as {};", m.new_fqn, text(a, src)),
                        None => format!("\nuse {};", m.new_fqn),
                    };
                    edits.push(TextEdit {
                        start: d.end_byte(),
                        end: d.end_byte(),
                        new_text: line,
                    });
                }
            }
            return;
        }
        // A redundant import in the class's new namespace goes away.
        let single = decl.is_some_and(|d| {
            let mut c = d.walk();
            let count = d
                .named_children(&mut c)
                .filter(|x| x.kind() == "namespace_use_clause")
                .count();
            count == 1
        });
        let same_name =
            alias.is_none_or(|a| text(a, src).eq_ignore_ascii_case(short_name(&m.new_fqn)));
        if single && same_name && namespace_of(&m.new_fqn).eq_ignore_ascii_case(final_ns) {
            let d = decl.expect("single implies a declaration");
            let (s, e, t) = crate::imports::remove_lines(src, d.start_byte(), d.end_byte());
            edits.push(TextEdit {
                start: s,
                end: e,
                new_text: t,
            });
            return;
        }
        let lead = if written.starts_with('\\') { "\\" } else { "" };
        edits.push(TextEdit {
            start: n.start_byte(),
            end: n.end_byte(),
            new_text: format!("{lead}{}", m.new_fqn),
        });
        return;
    }
    if written.starts_with('\\') {
        edits.push(TextEdit {
            start: n.start_byte(),
            end: n.end_byte(),
            new_text: format!("\\{}", m.new_fqn),
        });
        return;
    }
    if moved_file {
        // Bare and relative names of a moved file are the namespace pass's.
        return;
    }
    if written.contains('\\') {
        if !imported(file, written)
            || file
                .resolve_class(written, n.start_byte())
                .eq_ignore_ascii_case(&m.old_fqn)
        {
            edits.push(TextEdit {
                start: n.start_byte(),
                end: n.end_byte(),
                new_text: format!("\\{}", m.new_fqn),
            });
        }
        return;
    }
    if imported(file, written) {
        // Through its import, which is a usage of its own.
        return;
    }
    if !namespace_of(&m.new_fqn).eq_ignore_ascii_case(final_ns) {
        imports.push(m.new_fqn.clone());
    }
}

/// The edits of one file when the classes in `moves` move. `usages` are the spans of the
/// moved classes' names Find Usages reported in this file (with the index of the move);
/// `new_ns` is set when the file itself moves into that namespace.
pub fn file_edits(
    src: &str,
    path: &str,
    moves: &[ClassMove],
    usages: &[(Span, usize)],
    new_ns: Option<&str>,
) -> Result<Vec<TextEdit>, String> {
    let tree = parse(src).ok_or_else(|| cannot("The file cannot be parsed"))?;
    let root = tree.root_node();
    let file = index_source(path, src);
    let moves_map = Moves(moves);
    let mut edits = Vec::new();
    let mut imports = Vec::new();
    let final_ns = match new_ns {
        Some(ns) => ns.trim_matches('\\').to_string(),
        None => file_namespace(root, src),
    };
    if let Some(ns) = new_ns {
        let (e, i) = retarget(root, src, &file, ns.trim_matches('\\'), &moves_map)?;
        edits.extend(e);
        imports.extend(i);
    }
    for (span, i) in usages {
        let Some(m) = moves.get(*i) else { continue };
        reference_edit(
            root,
            src,
            &file,
            *span,
            m,
            &final_ns,
            new_ns.is_some(),
            &mut edits,
            &mut imports,
        );
    }
    // Imports the file already has (or is losing) are not added again.
    let removed: Vec<String> = file
        .uses
        .iter()
        .filter(|u| u.kind == UseKind::Class)
        .filter(|u| {
            edits
                .iter()
                .any(|e| e.new_text.is_empty() && e.start <= u.range.start && u.range.end <= e.end)
        })
        .map(|u| u.fqn.to_ascii_lowercase())
        .collect();
    imports.retain(|f| !removed.contains(&f.to_ascii_lowercase()));
    let mut out = edits;
    if !imports.is_empty() {
        // A file leaving its namespace for the global one gets its imports after `<?php`.
        let mut at = file.clone();
        if new_ns.is_some_and(|ns| ns.trim_matches('\\').is_empty()) {
            at.namespaces.clear();
        }
        out.extend(add_imports(&at, src, &imports));
    }
    Ok(tidy(out))
}

/// `src` after [`file_edits`].
pub fn moved_text(
    src: &str,
    path: &str,
    moves: &[ClassMove],
    usages: &[(Span, usize)],
    new_ns: Option<&str>,
) -> Result<String, String> {
    Ok(apply_edits(
        src,
        &file_edits(src, path, moves, usages, new_ns)?,
    ))
}

/// The text of a new file holding only the class `info` of `src` (with the file's opening
/// tag, `declare`, namespace and imports), still in its old namespace. Move Class puts the
/// result through [`file_edits`] to move it.
pub fn extract_class_file(src: &str, info: &ClassInfo) -> String {
    let Some(tree) = parse(src) else {
        return src[info.span.start..info.span.end].to_string();
    };
    let root = tree.root_node();
    let mut head = String::from("<?php\n");
    let mut c = root.walk();
    for n in root.named_children(&mut c) {
        if n.kind() == "declare_statement" {
            head.push('\n');
            head.push_str(text(n, src));
            head.push('\n');
        }
    }
    if !info.namespace.is_empty() {
        head.push_str(&format!("\nnamespace {};\n", info.namespace));
    }
    let uses: Vec<Node> = super::collect_nodes(root, true, &mut |n| {
        n.kind() == "namespace_use_declaration"
            && n.parent().is_some_and(|p| {
                p.kind() == "program"
                    || p.kind() == "compound_statement"
                        && p.parent()
                            .is_some_and(|q| q.kind() == "namespace_definition")
            })
    });
    if !uses.is_empty() {
        head.push('\n');
        for u in uses {
            head.push_str(text(u, src));
            head.push('\n');
        }
    }
    let indent = super::leading_ws(src, info.span.start);
    let body = super::reindent(&src[info.span.start..info.span.end], &indent, "");
    let cleaned = crate::imports::remove_unused_imports(&format!("{head}\n{body}\n"));
    cleaned.unwrap_or_else(|| format!("{head}\n{body}\n"))
}

/// The edits removing the class `info` from its file (with the blank line after it).
pub fn remove_class(src: &str, info: &ClassInfo) -> TextEdit {
    let (s, e, t) = crate::imports::remove_lines(src, info.span.start, info.span.end);
    TextEdit {
        start: s,
        end: e,
        new_text: t,
    }
}

/// Copy Class: the text of the copy named `new_name` in `new_ns` (the class renamed with
/// the references to itself, the namespace and imports adjusted).
pub fn copy_class_text(
    src: &str,
    info: &ClassInfo,
    new_name: &str,
    new_ns: &str,
) -> Result<String, String> {
    super::check_name(new_name)?;
    check_namespace(new_ns)?;
    let single = extract_class_file(src, info);
    let tree = parse(&single).ok_or_else(|| cannot("The class cannot be parsed"))?;
    let root = tree.root_node();
    // Rename the class and the names of itself (`self` style references by name).
    let mut renames = Vec::new();
    let classes = classes_in(&single);
    let Some(copy) = classes.iter().find(|c| c.name == info.name) else {
        return Err(cannot("The class cannot be found"));
    };
    renames.push(TextEdit {
        start: copy.name_span.start,
        end: copy.name_span.end,
        new_text: new_name.to_string(),
    });
    let mut names = code_class_names(root, &single);
    names.extend(all_doc_class_names(root, &single));
    let file = index_source("copy.php", &single);
    for n in names {
        let fqn = file.resolve_class(&n.text, n.span.start);
        if fqn.eq_ignore_ascii_case(&info.fqn)
            && n.span.start >= copy.span.start
            && n.span.end <= copy.span.end
        {
            let text = if n.is_bare() {
                new_name.to_string()
            } else {
                format!("\\{}", qualify(new_ns, new_name))
            };
            renames.push(TextEdit {
                start: n.span.start,
                end: n.span.end,
                new_text: text,
            });
        }
    }
    let renamed = apply_edits(&single, &tidy(renames));
    let moves = [ClassMove::new(info.fqn.clone(), qualify(new_ns, new_name))];
    moved_text(&renamed, "copy.php", &moves, &[], Some(new_ns))
}

/// Fully qualified references to `fqn` in the code and PHPDoc of `src` (a class naming
/// itself in a file Move Class creates).
pub fn fq_references(src: &str, fqn: &str) -> Vec<Span> {
    let Some(tree) = parse(src) else {
        return vec![];
    };
    let root = tree.root_node();
    let mut names = code_class_names(root, src);
    names.extend(all_doc_class_names(root, src));
    names
        .into_iter()
        .filter(|n| n.is_fully_qualified() && n.text[1..].eq_ignore_ascii_case(fqn))
        .map(|n| {
            // The short name, like a usage Find Usages reports.
            let short = short_name(&n.text).len();
            Span::new(n.span.end - short, n.span.end)
        })
        .collect()
}

/// The PSR-4 file name of a class: `<dir>/<Name>.php`.
pub fn class_file_path(dir: &str, name: &str) -> String {
    let dir = dir.trim_matches('/');
    if dir.is_empty() {
        format!("{name}.php")
    } else {
        format!("{dir}/{name}.php")
    }
}

/// Group usages by file: `(path, [(span, move index)])` in the order of the files.
pub fn group_by_file(usages: &[(String, Span, usize)]) -> Vec<(String, Vec<(Span, usize)>)> {
    let mut out: Vec<(String, Vec<(Span, usize)>)> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for (path, span, i) in usages {
        match at.get(path) {
            Some(&k) => out[k].1.push((*span, *i)),
            None => {
                at.insert(path.clone(), out.len());
                out.push((path.clone(), vec![(*span, *i)]));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(src: &str, word: &str) -> Vec<(Span, usize)> {
        src.match_indices(word)
            .filter(|(i, _)| {
                let before = src[..*i].chars().next_back();
                let after = src[i + word.len()..].chars().next();
                !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
                    && !after.is_some_and(|c| c.is_alphanumeric() || c == '_')
            })
            .map(|(i, _)| (Span::new(i, i + word.len()), 0))
            .collect()
    }

    fn user_move() -> Vec<ClassMove> {
        vec![ClassMove::new("App\\Models\\User", "App\\Domain\\User")]
    }

    #[test]
    fn finds_the_class_at_the_caret() {
        let src = "<?php\n\nnamespace App\\Models;\n\n/** Doc */\nfinal class User extends Model\n{\n    public function x() {}\n}\n";
        let c = class_at(src, src.find("User").unwrap() + 1).unwrap();
        assert_eq!(c.fqn, "App\\Models\\User");
        assert_eq!(c.kind, "class");
        assert!(c.sole);
        assert_eq!(&src[c.span.start..c.span.start + 3], "/**");
        assert!(class_at(src, src.find("x()").unwrap()).is_some());
        assert!(class_at(src, 2).is_none());
        let two = "<?php\nclass A {}\nfunction f() {}\n";
        assert!(!class_at(two, two.find('A').unwrap()).unwrap().sole);
        assert_eq!(classes_in(two).len(), 1);
    }

    #[test]
    fn moves_the_class_file_into_the_new_namespace() {
        let src = "<?php\n\ndeclare(strict_types=1);\n\nnamespace App\\Models;\n\nuse Illuminate\\Database\\Eloquent\\Model;\n\n/**\n * @property-read Collection<int, Order> $orders\n */\nclass User extends Model\n{\n    public function orders(): HasMany\n    {\n        return $this->hasMany(Order::class);\n    }\n\n    public function me(): \\App\\Models\\User\n    {\n        return new User();\n    }\n\n    public function status(): Sub\\Status\n    {\n    }\n}\n";
        let usages: Vec<(Span, usize)> = spans(src, "User")
            .into_iter()
            .filter(|(s, _)| src[..s.start].ends_with("Models\\"))
            .collect();
        let out = moved_text(
            src,
            "app/Models/User.php",
            &user_move(),
            &usages,
            Some("App\\Domain"),
        )
        .unwrap();
        assert_eq!(
            out,
            "<?php\n\ndeclare(strict_types=1);\n\nnamespace App\\Domain;\n\nuse App\\Models\\Collection;\nuse App\\Models\\HasMany;\nuse App\\Models\\Order;\nuse Illuminate\\Database\\Eloquent\\Model;\n\n/**\n * @property-read Collection<int, Order> $orders\n */\nclass User extends Model\n{\n    public function orders(): HasMany\n    {\n        return $this->hasMany(Order::class);\n    }\n\n    public function me(): \\App\\Domain\\User\n    {\n        return new User();\n    }\n\n    public function status(): \\App\\Models\\Sub\\Status\n    {\n    }\n}\n"
        );
    }

    #[test]
    fn classes_moving_together_keep_their_short_names() {
        let src = "<?php\nnamespace App\\Models;\n\nclass User\n{\n    public function o(Order $o): Invoice {}\n}\n";
        let moves = vec![
            ClassMove::new("App\\Models\\User", "App\\Domain\\User"),
            ClassMove::new("App\\Models\\Order", "App\\Domain\\Order"),
        ];
        let out = moved_text(src, "u.php", &moves, &[], Some("App\\Domain")).unwrap();
        assert_eq!(
            out,
            "<?php\nnamespace App\\Domain;\n\nuse App\\Models\\Invoice;\n\nclass User\n{\n    public function o(Order $o): Invoice {}\n}\n"
        );
    }

    #[test]
    fn global_files_get_a_namespace_and_namespaced_ones_can_lose_it() {
        let src = "<?php\n\nclass User extends Exception {}\n";
        let moves = vec![ClassMove::new("User", "App\\User")];
        let out = moved_text(src, "u.php", &moves, &[], Some("App")).unwrap();
        assert_eq!(
            out,
            "<?php\n\nnamespace App;\n\nuse Exception;\n\nclass User extends Exception {}\n"
        );
        let src = "<?php\n\nnamespace App;\n\nclass User extends Base {}\n";
        let moves = vec![ClassMove::new("App\\User", "User")];
        let out = moved_text(src, "u.php", &moves, &[], Some("")).unwrap();
        assert_eq!(
            out,
            "<?php\n\nuse App\\Base;\n\nclass User extends Base {}\n"
        );
    }

    #[test]
    fn updates_imports_and_qualified_names_elsewhere() {
        let src = "<?php\n\nnamespace App\\Http;\n\nuse App\\Models\\User;\nuse App\\Models\\User as Customer;\n\nclass C\n{\n    /** @var \\App\\Models\\User */\n    private $u;\n\n    public function f(User $u, Customer $c): \\App\\Models\\User\n    {\n        return 'App\\Models\\User' . \"App\\\\Models\\\\User\";\n    }\n}\n";
        let usages: Vec<(Span, usize)> = spans(src, "User")
            .into_iter()
            .filter(|(s, _)| src[..s.start].trim_end_matches('\\').ends_with("Models"))
            .collect();
        let out = moved_text(src, "c.php", &user_move(), &usages, None).unwrap();
        assert_eq!(
            out,
            "<?php\n\nnamespace App\\Http;\n\nuse App\\Domain\\User;\nuse App\\Domain\\User as Customer;\n\nclass C\n{\n    /** @var \\App\\Domain\\User */\n    private $u;\n\n    public function f(User $u, Customer $c): \\App\\Domain\\User\n    {\n        return 'App\\Domain\\User' . \"App\\\\Domain\\\\User\";\n    }\n}\n"
        );
    }

    #[test]
    fn imports_the_class_where_its_old_namespace_found_it() {
        let src = "<?php\n\nnamespace App\\Models;\n\nclass Order\n{\n    /** @return User */\n    public function user(): User { return new User(); }\n}\n";
        let usages = spans(src, "User");
        let out = moved_text(src, "o.php", &user_move(), &usages, None).unwrap();
        assert_eq!(
            out,
            "<?php\n\nnamespace App\\Models;\n\nuse App\\Domain\\User;\n\nclass Order\n{\n    /** @return User */\n    public function user(): User { return new User(); }\n}\n"
        );
        // A file in the new namespace drops its import.
        let src = "<?php\n\nnamespace App\\Domain;\n\nuse App\\Models\\User;\n\nclass Account { public User $u; }\n";
        let usages: Vec<(Span, usize)> = spans(src, "User").into_iter().take(1).collect();
        let out = moved_text(src, "a.php", &user_move(), &usages, None).unwrap();
        assert_eq!(
            out,
            "<?php\n\nnamespace App\\Domain;\n\nclass Account { public User $u; }\n"
        );
    }

    #[test]
    fn group_imports() {
        let src = "<?php\nnamespace X;\n\nuse App\\{Models\\User, Models\\Order};\nuse App\\Models\\{User as U, Order as O};\n";
        let usages: Vec<(Span, usize)> = spans(src, "User")
            .into_iter()
            .map(|(s, _)| (s, 0))
            .collect();
        let out = moved_text(src, "x.php", &user_move(), &usages, None).unwrap();
        assert_eq!(
            out,
            "<?php\nnamespace X;\n\nuse App\\{Domain\\User, Models\\Order};\nuse App\\Models\\{Order as O};\nuse App\\Domain\\User as U;\n"
        );
    }

    #[test]
    fn copies_a_class_under_a_new_name() {
        let src = "<?php\n\nnamespace App\\Models;\n\nuse Illuminate\\Support\\Str;\nuse Unused\\Thing;\n\n/** A user. */\nclass User extends Base\n{\n    public static function make(): User\n    {\n        return new static(Str::uuid());\n    }\n}\n\nfunction helper() {}\n";
        let info = class_at(src, src.find("User").unwrap()).unwrap();
        assert!(!info.sole);
        let out = copy_class_text(src, &info, "Customer", "App\\Billing").unwrap();
        assert_eq!(
            out,
            "<?php\n\nnamespace App\\Billing;\n\nuse App\\Models\\Base;\nuse Illuminate\\Support\\Str;\n\n/** A user. */\nclass Customer extends Base\n{\n    public static function make(): Customer\n    {\n        return new static(Str::uuid());\n    }\n}\n"
        );
        let removal = remove_class(src, &info);
        let rest = apply_edits(src, &[removal]);
        assert!(rest.contains("function helper"));
        assert!(!rest.contains("class User"));
        assert!(copy_class_text(src, &info, "2x", "App").is_err());
        assert!(check_namespace("App\\2x").is_err());
        assert_eq!(
            class_file_path("app/Billing/", "User"),
            "app/Billing/User.php"
        );
    }
}
