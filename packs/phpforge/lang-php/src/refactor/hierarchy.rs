//! The class hierarchy refactorings of PHPStorm's Refactor menu:
//!
//! - [`extract_interface`] (Extract Interface): a new interface declaring the chosen public
//!   methods (with their PHPDoc) and holding the chosen constants; the class implements it
//!   and, optionally, parameter types that only use the interface's methods name it.
//! - [`extract_superclass`] (Extract Superclass): a new class the chosen members move into
//!   (methods marked "Make abstract" stay and get an abstract declaration there); the class
//!   extends it, and it extends the class's old parent.
//! - [`pull_up`] (Pull Members Up): members move into a superclass or interface of the
//!   hierarchy (interfaces get declarations).
//! - [`push_down`] (Push Members Down): members are copied into every direct subclass and
//!   leave the class (or stay as abstract declarations, "Keep abstract").
//!
//! Members a moved member uses that do not move with it, members of the class that use a
//! pushed member, and similar problems are reported for PHPStorm's "Problems Detected"
//! dialog. Class names in moved code are imported in the target file (fully qualified when
//! the short name is taken). New files come from the PHP Class / Interface file templates,
//! which the caller renders ([`Skeleton`]).

use std::collections::HashSet;

use forge_codeintel::UsageOptions;
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use super::classes::{add_imports, all_doc_class_names, code_class_names, imported, qualify, tidy};
use super::move_class::{check_namespace, class_at, class_file_path, with_doc};
use super::move_members::{class_body_end, insert_members};
use super::project::{at_declaration, usage_spans, FileOp, Project, ProjectPlan};
use super::{cannot, check_name, line_of, parse, text, Span};
use crate::generate::{apply_edits, TextEdit};
use crate::index::SearchKind;
use crate::model::{short_name, FileIndex};
use crate::navigate::is_vendor;
use crate::parser::index_source;

// ---- The members of a class ----

/// A member the dialogs list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassMember {
    /// How PHPStorm lists it, unique in the class: `orders()`, `$count`, `ROLE`.
    pub id: String,
    /// `orders`, `count` (no `$`), `ROLE`.
    pub name: String,
    /// `method`, `property` or `constant`.
    pub kind: String,
    /// `public function orders(): array`.
    pub signature: String,
    pub visibility: String,
    pub is_static: bool,
    pub is_abstract: bool,
    pub constructor: bool,
    /// The ids of the other members of the class it uses (`$this->x`, `self::X`, ...).
    pub uses: Vec<String>,
    /// With its PHPDoc.
    pub span: Span,
    pub name_span: Span,
}

/// A class with its members (the dialogs' data).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassMembers {
    pub name: String,
    pub fqn: String,
    pub namespace: String,
    /// `class`, `interface`, `trait` or `enum`.
    pub kind: String,
    pub is_abstract: bool,
    pub name_span: Span,
    /// The declaration with its PHPDoc.
    pub span: Span,
    /// The parent class (fully qualified).
    pub parent: Option<String>,
    /// Implemented interfaces (an interface: the interfaces it extends), fully qualified.
    pub interfaces: Vec<String>,
    pub members: Vec<ClassMember>,
    /// The id of the member at the caret.
    pub selected: Option<String>,
}

impl ClassMembers {
    fn member(&self, id: &str) -> Option<&ClassMember> {
        self.members.iter().find(|m| m.id == id)
    }
}

/// A class or interface of the hierarchy (Pull Members Up targets, Push Members Down
/// subclasses).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyClass {
    pub name: String,
    pub fqn: String,
    /// `class` or `interface`.
    pub kind: String,
    pub path: String,
}

/// A chosen member; `make_abstract`: "Make abstract" (Extract Superclass, Pull Members Up)
/// or "Keep abstract" (Push Members Down) of a method.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MemberChoice {
    pub id: String,
    pub make_abstract: bool,
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn visibility(n: Node, src: &str) -> String {
    let mut c = n.walk();
    let v = n
        .children(&mut c)
        .find(|ch| ch.kind() == "visibility_modifier")
        .map(|v| text(v, src).to_ascii_lowercase());
    v.unwrap_or_else(|| "public".into())
}

fn has_child(n: Node, kind: &str) -> bool {
    let mut c = n.walk();
    let found = n.children(&mut c).any(|ch| ch.kind() == kind);
    found
}

fn kind_of_class(n: Node) -> Option<&'static str> {
    Some(match n.kind() {
        "class_declaration" => "class",
        "interface_declaration" => "interface",
        "trait_declaration" => "trait",
        "enum_declaration" => "enum",
        _ => return None,
    })
}

/// The class-like declaration whose name is at `name`.
fn class_node(root: Node<'_>, name: Span) -> Option<Node<'_>> {
    let n = root
        .descendant_for_byte_range(name.start, name.end)?
        .parent()?;
    kind_of_class(n).map(|_| n)
}

/// The members of a class body with their nodes (constructor-promoted properties and
/// declarations of several properties or constants in one statement are left out).
fn member_nodes<'t>(class: Node<'t>, src: &str) -> Vec<(ClassMember, Node<'t>)> {
    let Some(body) = class.child_by_field_name("body") else {
        return vec![];
    };
    let mut out = Vec::new();
    let mut c = body.walk();
    for m in body.named_children(&mut c) {
        let base = |id: String, name: String, kind: &str, signature: String, name_span: Span| {
            ClassMember {
                id,
                name,
                kind: kind.into(),
                signature,
                visibility: visibility(m, src),
                is_static: has_child(m, "static_modifier"),
                is_abstract: has_child(m, "abstract_modifier"),
                constructor: false,
                uses: vec![],
                span: with_doc(m, src),
                name_span,
            }
        };
        match m.kind() {
            "method_declaration" => {
                let Some(name) = m.child_by_field_name("name") else {
                    continue;
                };
                let head_end = m
                    .child_by_field_name("body")
                    .map_or(m.end_byte(), |b| b.start_byte());
                let n = text(name, src).to_string();
                let mut member = base(
                    format!("{n}()"),
                    n.clone(),
                    "method",
                    one_line(src[m.start_byte()..head_end].trim().trim_end_matches(';')),
                    Span::of(name),
                );
                member.constructor = n.eq_ignore_ascii_case("__construct");
                out.push((member, m));
            }
            "property_declaration" => {
                let mut c2 = m.walk();
                let els: Vec<Node> = m
                    .named_children(&mut c2)
                    .filter(|e| e.kind() == "property_element")
                    .collect();
                let [el] = els.as_slice() else { continue };
                let Some(name) = el.child_by_field_name("name") else {
                    continue;
                };
                let n = text(name, src).trim_start_matches('$').to_string();
                out.push((
                    base(
                        format!("${n}"),
                        n,
                        "property",
                        one_line(text(m, src).trim_end_matches(';')),
                        Span::new(name.start_byte() + 1, name.end_byte()),
                    ),
                    m,
                ));
            }
            "const_declaration" => {
                let mut c2 = m.walk();
                let els: Vec<Node> = m
                    .named_children(&mut c2)
                    .filter(|e| e.kind() == "const_element")
                    .collect();
                let [el] = els.as_slice() else { continue };
                let mut c3 = el.walk();
                let Some(name) = el.named_children(&mut c3).find(|x| x.kind() == "name") else {
                    continue;
                };
                let n = text(name, src).to_string();
                out.push((
                    base(
                        n.clone(),
                        n,
                        "constant",
                        one_line(text(m, src).trim_end_matches(';')),
                        Span::of(name),
                    ),
                    m,
                ));
            }
            _ => {}
        }
    }
    out
}

/// The ids of the members of the class that `n` uses: `$this->x`, `$this->f()`,
/// `self::X`, `static::f()`, `self::$x` and the same with the class's own name.
fn member_references(n: Node, src: &str, class_name: &str) -> Vec<(String, Span)> {
    let is_self = |scope: Node| {
        let t = text(scope, src);
        matches!(scope.kind(), "relative_scope")
            && (t.eq_ignore_ascii_case("self") || t.eq_ignore_ascii_case("static"))
            || matches!(scope.kind(), "name") && t.eq_ignore_ascii_case(class_name)
    };
    let is_this = |o: Node| o.kind() == "variable_name" && text(o, src) == "$this";
    let mut out = Vec::new();
    let nodes = super::collect_nodes(n, true, &mut |x| {
        matches!(
            x.kind(),
            "member_call_expression"
                | "nullsafe_member_call_expression"
                | "member_access_expression"
                | "nullsafe_member_access_expression"
                | "scoped_call_expression"
                | "scoped_property_access_expression"
                | "class_constant_access_expression"
        )
    });
    for x in nodes {
        let found = match x.kind() {
            "member_call_expression" | "nullsafe_member_call_expression" => x
                .child_by_field_name("object")
                .filter(|o| is_this(*o))
                .and(x.child_by_field_name("name"))
                .filter(|nm| nm.kind() == "name")
                .map(|nm| (format!("{}()", text(nm, src)), nm)),
            "member_access_expression" | "nullsafe_member_access_expression" => x
                .child_by_field_name("object")
                .filter(|o| is_this(*o))
                .and(x.child_by_field_name("name"))
                .filter(|nm| nm.kind() == "name")
                .map(|nm| (format!("${}", text(nm, src)), nm)),
            "scoped_call_expression" => x
                .child_by_field_name("scope")
                .filter(|s| is_self(*s))
                .and(x.child_by_field_name("name"))
                .map(|nm| (format!("{}()", text(nm, src)), nm)),
            "scoped_property_access_expression" => x
                .child_by_field_name("scope")
                .filter(|s| is_self(*s))
                .and(x.child_by_field_name("name"))
                .map(|nm| (text(nm, src).to_string(), nm)),
            "class_constant_access_expression" => x
                .named_child(0)
                .filter(|s| is_self(*s))
                .and(x.named_child(1))
                .filter(|nm| !text(*nm, src).eq_ignore_ascii_case("class"))
                .map(|nm| (text(nm, src).to_string(), nm)),
            _ => None,
        };
        if let Some((id, nm)) = found {
            out.push((id, Span::of(nm)));
        }
    }
    out
}

/// The class at `pos` with its members and what each of them uses.
pub fn class_members(src: &str, pos: usize) -> Option<ClassMembers> {
    let info = class_at(src, pos)?;
    let tree = parse(src)?;
    let root = tree.root_node();
    let class = class_node(root, info.name_span)?;
    let file = index_source("class.php", src);
    let indexed = file
        .classes
        .iter()
        .find(|c| c.fqn.eq_ignore_ascii_case(&info.fqn));
    let (parent, interfaces) = match (info.kind.as_str(), indexed) {
        ("interface", Some(c)) => (None, c.extends.clone()),
        (_, Some(c)) => (c.extends.first().cloned(), c.implements.clone()),
        _ => (None, vec![]),
    };
    let nodes = member_nodes(class, src);
    let ids: Vec<String> = nodes.iter().map(|(m, _)| m.id.clone()).collect();
    let mut members = Vec::new();
    let mut selected = None;
    for (mut m, n) in nodes {
        let mut uses: Vec<String> = Vec::new();
        for (id, _) in member_references(n, src, &info.name) {
            if let Some(known) = ids.iter().find(|i| same_id(i, &id)) {
                if *known != m.id && !uses.contains(known) {
                    uses.push(known.clone());
                }
            }
        }
        m.uses = uses;
        if m.span.start <= pos && pos <= m.span.end {
            selected = Some(m.id.clone());
        }
        members.push(m);
    }
    Some(ClassMembers {
        name: info.name,
        fqn: info.fqn,
        namespace: info.namespace,
        kind: info.kind,
        is_abstract: has_child(class, "abstract_modifier"),
        name_span: info.name_span,
        span: info.span,
        parent,
        interfaces,
        members,
        selected,
    })
}

/// Member ids are the same (method names ignore case like PHP).
fn same_id(a: &str, b: &str) -> bool {
    if a.ends_with("()") {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

/// `method orders()`, `property $count`, `constant ROLE`.
fn describe(m: &ClassMember) -> String {
    format!("{} {}", m.kind, m.id)
}

/// `Method orders()` (at the start of a message).
fn subject(m: &ClassMember) -> String {
    capitalized(&describe(m))
}

fn capitalized(s: &str) -> String {
    let mut cs = s.chars();
    match cs.next() {
        Some(f) => f.to_uppercase().chain(cs).collect(),
        None => String::new(),
    }
}

// ---- Code in another file ----

/// Code that goes into another file: class names the target resolves differently are
/// imported there (fully qualified when the short name is taken).
struct Relocation {
    source: FileIndex,
    target: FileIndex,
    target_at: usize,
    /// The class the code goes into: its short name cannot be imported for another class.
    own: String,
    /// Classes to import in the target file.
    imports: Vec<String>,
}

impl Relocation {
    fn new(source: &str, target: &str, target_at: usize, own: &str) -> Self {
        Self {
            source: index_source("source.php", source),
            target: index_source("target.php", target),
            target_at,
            own: own.to_string(),
            imports: vec![],
        }
    }

    /// How the target names the class `fqn`, queuing an import when it needs one.
    fn name_for(&mut self, fqn: &str) -> String {
        let fqn = fqn.trim_start_matches('\\');
        let short = short_name(fqn);
        if self
            .target
            .resolve_class(short, self.target_at)
            .eq_ignore_ascii_case(fqn)
        {
            return short.to_string();
        }
        let in_target_ns = self
            .target
            .namespace_at(self.target_at)
            .eq_ignore_ascii_case(super::classes::namespace_of(fqn));
        let taken = (short.eq_ignore_ascii_case(&self.own) && !in_target_ns)
            || imported(&self.target, short)
            || self
                .imports
                .iter()
                .any(|i| short_name(i).eq_ignore_ascii_case(short) && !i.eq_ignore_ascii_case(fqn));
        if taken {
            return format!("\\{fqn}");
        }
        if !self.imports.iter().any(|i| i.eq_ignore_ascii_case(fqn)) {
            self.imports.push(fqn.to_string());
        }
        short.to_string()
    }

    /// `code` (written at `at` of the source file) with its class names fit for the target.
    fn code(&mut self, code: &str, at: usize) -> String {
        let head = "<?php class __Forge_Relocate {\n";
        let wrapped = format!("{head}{code}\n}}");
        let Some(tree) = parse(&wrapped) else {
            return code.to_string();
        };
        let root = tree.root_node();
        let mut names = code_class_names(root, &wrapped);
        names.extend(all_doc_class_names(root, &wrapped));
        let mut edits = Vec::new();
        for n in names {
            if n.is_fully_qualified() || n.text == "__Forge_Relocate" {
                continue;
            }
            let fqn = self.source.resolve_class(&n.text, at);
            let there = self.target.resolve_class(&n.text, self.target_at);
            if fqn.eq_ignore_ascii_case(&there) {
                continue;
            }
            let new_text = self.name_for(&fqn);
            edits.push(TextEdit {
                start: n.span.start - head.len(),
                end: n.span.end - head.len(),
                new_text,
            });
        }
        apply_edits(code, &tidy(edits))
    }

    /// The import edits of the target file (`target_src`).
    fn import_edits(&self, target_src: &str) -> Vec<TextEdit> {
        add_imports(&self.target, target_src, &self.imports)
    }
}

/// `code` (starting at `at` of `src`, whose line has the old indentation) indented by
/// `indent`.
fn place(src: &str, at: usize, code: &str, indent: &str) -> String {
    let from = super::leading_ws(src, at);
    format!("{indent}{}", super::reindent(code, &from, indent))
}

/// A member's code with its visibility changed to `protected` (private members other
/// members of the class still use).
fn member_code(src: &str, m: &ClassMember, node: Node, protect: bool) -> String {
    let code = &src[m.span.start..m.span.end];
    if !protect || m.visibility != "private" {
        return code.to_string();
    }
    let mut c = node.walk();
    let vis = node
        .children(&mut c)
        .find(|ch| ch.kind() == "visibility_modifier");
    match vis {
        Some(v) => apply_edits(
            code,
            &[TextEdit {
                start: v.start_byte() - m.span.start,
                end: v.end_byte() - m.span.start,
                new_text: "protected".into(),
            }],
        ),
        None => code.to_string(),
    }
}

/// How a method is declared without its body.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Declaration {
    /// `public function f(): T;` (interfaces).
    Interface,
    /// `abstract <visibility> function f(): T;` (private becomes protected).
    Abstract,
}

/// The declaration of a method (with its PHPDoc, without attributes and body).
fn method_declaration(src: &str, m: &ClassMember, node: Node, how: Declaration) -> String {
    let doc = &src[m.span.start..node.start_byte()];
    let mut c = node.walk();
    let func = node
        .children(&mut c)
        .find(|ch| ch.kind() == "function")
        .map_or(node.start_byte(), |f| f.start_byte());
    let end = node
        .child_by_field_name("body")
        .map_or(node.end_byte(), |b| b.start_byte());
    let head = src[func..end].trim_end().trim_end_matches(';').trim_end();
    let mut out = doc.to_string();
    let vis = match how {
        Declaration::Interface => "public".to_string(),
        Declaration::Abstract => {
            out.push_str("abstract ");
            if m.visibility == "private" {
                "protected".into()
            } else {
                m.visibility.clone()
            }
        }
    };
    out.push_str(&vis);
    out.push(' ');
    if m.is_static {
        out.push_str("static ");
    }
    out.push_str(head);
    out.push(';');
    out
}

/// The edit adding `abstract ` to a class (none when it is abstract already).
fn abstract_edit(class: Node) -> Option<TextEdit> {
    if class.kind() != "class_declaration" || has_child(class, "abstract_modifier") {
        return None;
    }
    let mut c = class.walk();
    let kw = class.children(&mut c).find(|ch| ch.kind() == "class")?;
    Some(TextEdit {
        start: kw.start_byte(),
        end: kw.start_byte(),
        new_text: "abstract ".into(),
    })
}

/// The edit adding `name` to the interfaces the class implements.
fn implements_edit(class: Node, src: &str, name: &str) -> TextEdit {
    let mut c = class.walk();
    let children: Vec<Node> = class.children(&mut c).collect();
    if let Some(clause) = children
        .iter()
        .find(|n| n.kind() == "class_interface_clause")
    {
        return TextEdit {
            start: clause.end_byte(),
            end: clause.end_byte(),
            new_text: format!(", {name}"),
        };
    }
    let after = children
        .iter()
        .rfind(|n| matches!(n.kind(), "base_clause" | "name" | "primitive_type"))
        .map_or(class.start_byte(), |n| n.end_byte());
    let _ = src;
    TextEdit {
        start: after,
        end: after,
        new_text: format!(" implements {name}"),
    }
}

/// The edit making the class extend `name` (replacing its old parent).
fn extends_edit(class: Node, name: &str) -> Option<TextEdit> {
    let mut c = class.walk();
    let children: Vec<Node> = class.children(&mut c).collect();
    if let Some(base) = children.iter().find(|n| n.kind() == "base_clause") {
        let parent = base.named_child(0)?;
        return Some(TextEdit {
            start: parent.start_byte(),
            end: parent.end_byte(),
            new_text: name.to_string(),
        });
    }
    let name_node = class.child_by_field_name("name")?;
    Some(TextEdit {
        start: name_node.end_byte(),
        end: name_node.end_byte(),
        new_text: format!(" extends {name}"),
    })
}

/// The edit removing a member with its line and a blank line next to it.
fn remove_edit(src: &str, span: Span) -> TextEdit {
    let (start, mut end, new_text) = crate::imports::remove_lines(src, span.start, span.end);
    // The first member: no blank line is left after the `{`.
    if new_text.is_empty() && src[..start].trim_end().ends_with('{') {
        if let Some(nl) = src[end..].find('\n') {
            if src[end..end + nl].trim().is_empty() && src[..end].ends_with('\n') {
                end += nl + 1;
            }
        }
    }
    TextEdit {
        start,
        end,
        new_text,
    }
}

// ---- New files ----

/// The class or interface a refactoring creates; the caller renders the file template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTypeFile {
    /// `interface` or `class`.
    pub kind: String,
    pub name: String,
    pub namespace: String,
    pub directory: String,
    /// The parent class, fully qualified.
    pub extends: Option<String>,
}

/// Renders a new file: its path and text with an empty body (PHPStorm's "PHP Class" or
/// "PHP Interface" template).
pub type Skeleton<'a> = dyn Fn(&NewTypeFile) -> Result<(String, String), String> + 'a;

/// The built-in templates without a file header (tests and a fallback).
pub fn plain_skeleton(t: &NewTypeFile) -> Result<(String, String), String> {
    let mut out = String::from("<?php\n\n");
    if !t.namespace.is_empty() {
        out.push_str(&format!("namespace {};\n\n", t.namespace));
    }
    let mut decl = format!("{} {}", t.kind, t.name);
    if let Some(parent) = &t.extends {
        let parent = parent.trim_start_matches('\\');
        if super::classes::namespace_of(parent).eq_ignore_ascii_case(&t.namespace)
            || !parent.contains('\\')
        {
            decl.push_str(&format!(" extends {}", short_name(parent)));
        } else {
            out.push_str(&format!("use {parent};\n\n"));
            decl.push_str(&format!(" extends {}", short_name(parent)));
        }
    }
    out.push_str(&format!("{decl}\n{{\n\n}}\n"));
    Ok((class_file_path(&t.directory, &t.name), out))
}

/// The declaration named `name` in a file.
fn declaration_in<'t>(root: Node<'t>, src: &str, name: &str) -> Option<Node<'t>> {
    let decls = super::collect_nodes(root, false, &mut |n| kind_of_class(n).is_some());
    let mut all = decls;
    if all.is_empty() {
        // `collect_nodes` does not enter declarations below the root's own children.
        let mut c = root.walk();
        all = root.named_children(&mut c).collect();
    }
    all.into_iter().find(|n| {
        kind_of_class(*n).is_some()
            && n.child_by_field_name("name")
                .is_some_and(|x| text(x, src).eq_ignore_ascii_case(name))
    })
}

/// Every class-like declaration of a file (also inside braced namespaces).
fn declarations<'t>(root: Node<'t>) -> Vec<Node<'t>> {
    let mut out = Vec::new();
    fn walk<'t>(n: Node<'t>, out: &mut Vec<Node<'t>>) {
        if kind_of_class(n).is_some() {
            out.push(n);
            return;
        }
        let mut c = n.walk();
        for ch in n.named_children(&mut c) {
            walk(ch, out);
        }
    }
    walk(root, &mut out);
    out
}

/// The new file's text: the skeleton with `code` as the body of its type, `abstract`
/// when asked for and the imports the code needs.
fn fill_new_file(
    skeleton: &str,
    name: &str,
    code: &str,
    reloc: &Relocation,
    make_abstract: bool,
) -> Result<String, String> {
    let tree = parse(skeleton).ok_or_else(|| cannot("The file template cannot be parsed"))?;
    let root = tree.root_node();
    let decl = declarations(root)
        .into_iter()
        .find(|n| {
            n.child_by_field_name("name")
                .is_some_and(|x| text(x, skeleton).eq_ignore_ascii_case(name))
        })
        .or_else(|| declaration_in(root, skeleton, name))
        .ok_or_else(|| cannot(format!("The file template does not declare {name}")))?;
    let body = decl
        .child_by_field_name("body")
        .ok_or_else(|| cannot("The file template has no class body"))?;
    let mut edits = Vec::new();
    if !code.is_empty() {
        edits.push(TextEdit {
            start: body.start_byte(),
            end: body.end_byte(),
            new_text: format!("{{\n{code}\n}}"),
        });
    }
    if make_abstract {
        edits.extend(abstract_edit(decl));
    }
    edits.extend(reloc.import_edits(skeleton));
    Ok(apply_edits(skeleton, &tidy(edits)))
}

// ---- Shared checks ----

fn class_target(
    p: &mut dyn Project,
    path: &str,
    text: &str,
    offset: usize,
    what: &str,
) -> Result<(String, String, ClassMembers), String> {
    at_declaration(p, path, text, offset, |t, o| {
        class_members(t, o).ok_or_else(|| {
            cannot(format!(
                "Caret should be positioned inside the class to {what}"
            ))
        })
    })
}

/// The new type's name, namespace and fully qualified name, checked like PHPStorm.
fn new_type_name(
    p: &mut dyn Project,
    name: &str,
    namespace: &str,
) -> Result<(String, String, String), String> {
    let name = name.trim();
    check_name(name).map_err(cannot)?;
    check_namespace(namespace).map_err(cannot)?;
    let ns = namespace.trim().trim_matches('\\').to_string();
    let fqn = qualify(&ns, name);
    if p.search(name, SearchKind::Class)
        .iter()
        .any(|h| h.fqn.eq_ignore_ascii_case(&fqn))
    {
        return Err(cannot(format!("Class {fqn} already exists")));
    }
    Ok((name.to_string(), ns, fqn))
}

/// The chosen members in the class's order; unknown ids are an error.
fn chosen<'a>(info: &'a ClassMembers, ids: &[String]) -> Result<Vec<&'a ClassMember>, String> {
    for id in ids {
        if info.member(id).is_none() {
            return Err(cannot(format!("Class {} has no member {id}", info.name)));
        }
    }
    Ok(info
        .members
        .iter()
        .filter(|m| ids.iter().any(|i| *i == m.id))
        .collect())
}

/// The member nodes of the class `info` in `src` by id.
fn nodes_of<'t>(root: Node<'t>, src: &str, info: &ClassMembers) -> Vec<(String, Node<'t>)> {
    class_node(root, info.name_span)
        .map(|c| {
            member_nodes(c, src)
                .into_iter()
                .map(|(m, n)| (m.id, n))
                .collect()
        })
        .unwrap_or_default()
}

fn node_of<'t>(nodes: &[(String, Node<'t>)], id: &str) -> Option<Node<'t>> {
    nodes.iter().find(|(i, _)| i == id).map(|(_, n)| *n)
}

/// The class `fqn` in the project: its file and text (project files only).
fn find_class(p: &mut dyn Project, fqn: &str) -> Option<(String, String)> {
    let fqn = fqn.trim_start_matches('\\');
    let hit = p
        .search(short_name(fqn), SearchKind::Class)
        .into_iter()
        .find(|h| h.fqn.eq_ignore_ascii_case(fqn))?;
    if is_vendor(&hit.path) || hit.path.contains("://") {
        return None;
    }
    let text = p.read(&hit.path)?;
    Some((hit.path, text))
}

// ---- Extract Interface ----

/// Extract Interface's choices.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExtractInterfaceOptions {
    pub name: String,
    pub namespace: String,
    pub directory: String,
    /// Member ids: public methods and constants.
    pub members: Vec<String>,
    /// "Replace class references with interface where possible".
    pub replace_references: bool,
}

/// Whether Extract Interface can take the member: public methods (not the constructor)
/// and constants.
pub fn extractable_to_interface(m: &ClassMember) -> bool {
    match m.kind.as_str() {
        "method" => m.visibility == "public" && !m.constructor,
        "constant" => m.visibility == "public",
        _ => false,
    }
}

/// The class Extract Interface / Superclass works on (the caret's class, or the class a
/// reference at the caret names): its file and members.
pub fn extract_target(
    p: &mut dyn Project,
    path: &str,
    text: &str,
    offset: usize,
) -> Result<(String, ClassMembers), String> {
    let (path, _, info) = class_target(p, path, text, offset, "refactor")?;
    Ok((path, info))
}

/// Extract Interface: a new interface with the chosen members; the class implements it.
pub fn extract_interface(
    p: &mut dyn Project,
    path: &str,
    text: &str,
    offset: usize,
    options: &ExtractInterfaceOptions,
    skeleton: &Skeleton,
) -> Result<ProjectPlan, String> {
    let (path, text, info) = class_target(p, path, text, offset, "extract an interface from")?;
    let (path, text) = (path.as_str(), text.as_str());
    if !matches!(info.kind.as_str(), "class" | "enum") {
        return Err(cannot(
            "Extract Interface is available for classes and enums",
        ));
    }
    let (name, ns, fqn) = new_type_name(p, &options.name, &options.namespace)?;
    let members = chosen(&info, &options.members)?;
    if let Some(m) = members.iter().find(|m| !extractable_to_interface(m)) {
        return Err(cannot(format!(
            "{} cannot be declared in an interface",
            subject(m)
        )));
    }
    let (new_path, skel) = skeleton(&NewTypeFile {
        kind: "interface".into(),
        name: name.clone(),
        namespace: ns.clone(),
        directory: options.directory.trim().trim_matches('/').to_string(),
        extends: None,
    })?;
    if p.read(&new_path).is_some() {
        return Err(cannot(format!("File {new_path} already exists")));
    }
    let tree = parse(text).ok_or_else(|| cannot("The file cannot be parsed"))?;
    let root = tree.root_node();
    let nodes = nodes_of(root, text, &info);
    let class =
        class_node(root, info.name_span).ok_or_else(|| cannot("The class cannot be found"))?;
    let indent = super::indent_unit(text);
    let skel_at = skel.rfind('{').unwrap_or(skel.len());
    let mut reloc = Relocation::new(text, &skel, skel_at, &name);
    // Constants first, then the methods, like PHPStorm.
    let mut parts = Vec::new();
    for kind in ["constant", "method"] {
        for m in members.iter().filter(|m| m.kind == kind) {
            let Some(node) = node_of(&nodes, &m.id) else {
                continue;
            };
            let code = if kind == "method" {
                method_declaration(text, m, node, Declaration::Interface)
            } else {
                text[m.span.start..m.span.end].to_string()
            };
            let code = reloc.code(&code, m.span.start);
            parts.push(place(text, m.span.start, &code, &indent));
        }
    }
    let new_text = fill_new_file(&skel, &name, &parts.join("\n\n"), &reloc, false)?;
    let mut plan = ProjectPlan::default();
    plan.ops.push(FileOp::Create {
        path: new_path,
        text: new_text,
    });

    // The class implements it; constants moved out of it.
    let mut here = Relocation::new(text, text, class.start_byte(), &info.name);
    let reference = here.name_for(&fqn);
    let mut edits = vec![implements_edit(class, text, &reference)];
    for m in members.iter().filter(|m| m.kind == "constant") {
        edits.push(remove_edit(text, m.span));
    }
    let mut own_imports = here.imports.clone();

    if options.replace_references {
        let methods: Vec<String> = members
            .iter()
            .filter(|m| m.kind == "method")
            .map(|m| m.name.clone())
            .collect();
        let constants: Vec<String> = members
            .iter()
            .filter(|m| m.kind == "constant")
            .map(|m| m.name.clone())
            .collect();
        let found = p.usages(path, text, info.name_span.start, &UsageOptions::default());
        let spans = match &found {
            Some(f) => usage_spans(p, f),
            None => vec![],
        };
        let mut files: Vec<String> = Vec::new();
        for (f, _, _) in &spans {
            if !files.contains(f) {
                files.push(f.clone());
            }
        }
        for file in files {
            let t = if file == path {
                text.to_string()
            } else {
                match p.read(&file) {
                    Some(t) => t,
                    None => continue,
                }
            };
            let Some(tree) = parse(&t) else { continue };
            let root = tree.root_node();
            let mut r = Relocation::new(&t, &t, 0, if file == path { &info.name } else { "" });
            let mut file_edits = Vec::new();
            for (_, span, _) in spans.iter().filter(|(f, _, _)| *f == file) {
                let Some(node) = replaceable(root, &t, *span, &methods, &constants) else {
                    continue;
                };
                r.target_at = node.start_byte();
                let new_text = if text_of(node, &t).starts_with('\\') {
                    format!("\\{fqn}")
                } else {
                    r.name_for(&fqn)
                };
                file_edits.push(TextEdit {
                    start: node.start_byte(),
                    end: node.end_byte(),
                    new_text,
                });
            }
            if file == path {
                edits.extend(file_edits);
                own_imports.extend(r.imports);
            } else if !file_edits.is_empty() {
                file_edits.extend(r.import_edits(&t));
                plan.edit(&file, tidy(file_edits));
            }
        }
    }
    edits.extend(add_imports(&index_source(path, text), text, &own_imports));
    plan.edit(path, tidy(edits));
    Ok(plan)
}

fn text_of<'a>(n: Node, src: &'a str) -> &'a str {
    text(n, src)
}

/// The class name at `span` when it is a parameter's type that can name the interface:
/// the function only calls the interface's methods on the parameter (and reads its
/// constants).
fn replaceable<'t>(
    root: Node<'t>,
    src: &str,
    span: Span,
    methods: &[String],
    constants: &[String],
) -> Option<Node<'t>> {
    let mut n = root.descendant_for_byte_range(span.start, span.end)?;
    if n.parent().is_some_and(|p| p.kind() == "qualified_name") {
        n = n.parent()?;
    }
    let ty = n.parent().filter(|p| p.kind() == "named_type")?;
    let mut holder = ty.parent()?;
    if holder.kind() == "optional_type" {
        holder = holder.parent()?;
    }
    if holder.kind() != "simple_parameter" {
        return None;
    }
    let var = text(holder.child_by_field_name("name")?, src);
    let function = holder.parent()?.parent()?;
    if !super::is_function_like(function) {
        return None;
    }
    let Some(body) = function.child_by_field_name("body") else {
        // An abstract or interface method: nothing uses the parameter.
        return Some(n);
    };
    let uses = super::collect_nodes(body, true, &mut |x| {
        x.kind() == "variable_name" && text(x, src) == var
    });
    for u in uses {
        let p = u.parent()?;
        let ok = match p.kind() {
            "member_call_expression" | "nullsafe_member_call_expression" => {
                p.child_by_field_name("object") == Some(u)
                    && p.child_by_field_name("name").is_some_and(|nm| {
                        methods
                            .iter()
                            .any(|m| m.eq_ignore_ascii_case(text(nm, src)))
                    })
            }
            "class_constant_access_expression" => {
                p.named_child(0) == Some(u)
                    && p.named_child(1)
                        .is_some_and(|nm| constants.iter().any(|c| c == text(nm, src)))
            }
            _ => false,
        };
        if !ok {
            return None;
        }
    }
    Some(n)
}

// ---- Extract Superclass ----

/// Extract Superclass's choices.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExtractSuperclassOptions {
    pub name: String,
    pub namespace: String,
    pub directory: String,
    pub members: Vec<MemberChoice>,
}

/// Members that stay in the class (not chosen, or chosen as abstract) use `m`: a private
/// `m` becomes protected when it moves up.
fn used_by_staying(info: &ClassMembers, m: &ClassMember, moving: &HashSet<String>) -> bool {
    info.members
        .iter()
        .any(|s| !moving.contains(&s.id) && s.uses.iter().any(|u| *u == m.id))
}

/// Extract Superclass: a new class with the chosen members, between the class and its
/// old parent.
pub fn extract_superclass(
    p: &mut dyn Project,
    path: &str,
    text: &str,
    offset: usize,
    options: &ExtractSuperclassOptions,
    skeleton: &Skeleton,
) -> Result<ProjectPlan, String> {
    let (path, text, info) = class_target(p, path, text, offset, "extract a superclass from")?;
    let (path, text) = (path.as_str(), text.as_str());
    if info.kind != "class" {
        return Err(cannot("Extract Superclass is available for classes"));
    }
    let (name, ns, fqn) = new_type_name(p, &options.name, &options.namespace)?;
    let ids: Vec<String> = options.members.iter().map(|m| m.id.clone()).collect();
    let members = chosen(&info, &ids)?;
    let is_abstract = |m: &ClassMember| {
        m.kind == "method"
            && options
                .members
                .iter()
                .any(|c| c.id == m.id && c.make_abstract)
    };
    let any_abstract = members.iter().any(|m| is_abstract(m));
    let (new_path, skel) = skeleton(&NewTypeFile {
        kind: "class".into(),
        name: name.clone(),
        namespace: ns.clone(),
        directory: options.directory.trim().trim_matches('/').to_string(),
        extends: info.parent.clone(),
    })?;
    if p.read(&new_path).is_some() {
        return Err(cannot(format!("File {new_path} already exists")));
    }
    let mut plan = ProjectPlan::default();
    // Moved code that uses members staying behind.
    let moving: HashSet<String> = members
        .iter()
        .filter(|m| !is_abstract(m))
        .map(|m| m.id.clone())
        .collect();
    let declared: HashSet<String> = members.iter().map(|m| m.id.clone()).collect();
    for m in members.iter().filter(|m| !is_abstract(m)) {
        for u in &m.uses {
            if !declared.contains(u) {
                let used = info.member(u).map_or(u.clone(), describe);
                plan.conflict(
                    format!(
                        "{} uses {used}, which is not moved to the superclass {name}",
                        subject(m)
                    ),
                    Some(path),
                    Some(line_of(text, m.span.start)),
                );
            }
        }
    }
    let tree = parse(text).ok_or_else(|| cannot("The file cannot be parsed"))?;
    let root = tree.root_node();
    let nodes = nodes_of(root, text, &info);
    let class =
        class_node(root, info.name_span).ok_or_else(|| cannot("The class cannot be found"))?;
    let indent = super::indent_unit(text);
    let skel_at = skel.rfind('{').unwrap_or(skel.len());
    let mut reloc = Relocation::new(text, &skel, skel_at, &name);
    let mut parts = Vec::new();
    let mut edits = Vec::new();
    for m in &members {
        let Some(node) = node_of(&nodes, &m.id) else {
            continue;
        };
        let code = if is_abstract(m) {
            method_declaration(text, m, node, Declaration::Abstract)
        } else {
            edits.push(remove_edit(text, m.span));
            member_code(text, m, node, used_by_staying(&info, m, &moving))
        };
        let code = reloc.code(&code, m.span.start);
        parts.push(place(text, m.span.start, &code, &indent));
    }
    let new_text = fill_new_file(&skel, &name, &parts.join("\n\n"), &reloc, any_abstract)?;
    plan.ops.push(FileOp::Create {
        path: new_path,
        text: new_text,
    });
    let mut here = Relocation::new(text, text, class.start_byte(), &info.name);
    let reference = here.name_for(&fqn);
    edits.extend(extends_edit(class, &reference));
    edits.extend(here.import_edits(text));
    plan.edit(path, tidy(edits));
    Ok(plan)
}

// ---- Pull Members Up ----

/// The superclasses and interfaces of a class (project files only), nearest first.
pub fn supertypes(p: &mut dyn Project, info: &ClassMembers) -> Vec<HierarchyClass> {
    let mut out: Vec<HierarchyClass> = Vec::new();
    let mut queue: Vec<String> = info.parent.iter().cloned().collect();
    queue.extend(info.interfaces.iter().cloned());
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(info.fqn.to_ascii_lowercase());
    let mut i = 0;
    while i < queue.len() && out.len() < 50 {
        let fqn = queue[i].trim_start_matches('\\').to_string();
        i += 1;
        if !seen.insert(fqn.to_ascii_lowercase()) {
            continue;
        }
        let Some((path, text)) = find_class(p, &fqn) else {
            continue;
        };
        let file = index_source(&path, &text);
        let Some(c) = file
            .classes
            .iter()
            .find(|c| c.fqn.eq_ignore_ascii_case(&fqn))
        else {
            continue;
        };
        let interface = c.kind == crate::model::SymbolKind::Interface;
        out.push(HierarchyClass {
            name: c.name.clone(),
            fqn: c.fqn.clone(),
            kind: if interface { "interface" } else { "class" }.into(),
            path: path.clone(),
        });
        queue.extend(c.extends.iter().cloned());
        queue.extend(c.implements.iter().cloned());
    }
    // Classes before interfaces, like PHPStorm's list.
    out.sort_by_key(|c| c.kind == "interface");
    out
}

/// Pull Members Up / Push Members Down at the caret: the class (the declaration of a
/// reference at the caret) with its members.
pub fn hierarchy_target(
    p: &mut dyn Project,
    path: &str,
    text: &str,
    offset: usize,
) -> Result<(String, ClassMembers), String> {
    let (path, _, info) = class_target(p, path, text, offset, "move members of")?;
    Ok((path, info))
}

/// Pull Members Up's choices.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PullUpOptions {
    /// The superclass or interface, fully qualified.
    pub target: String,
    pub members: Vec<MemberChoice>,
}

/// Pull Members Up: the chosen members into the superclass or interface `target`.
pub fn pull_up(
    p: &mut dyn Project,
    path: &str,
    text: &str,
    offset: usize,
    options: &PullUpOptions,
) -> Result<ProjectPlan, String> {
    let info = class_members(text, offset).ok_or_else(|| cannot("The class cannot be found"))?;
    let target_fqn = options.target.trim().trim_start_matches('\\');
    if !supertypes(p, &info)
        .iter()
        .any(|s| s.fqn.eq_ignore_ascii_case(target_fqn))
    {
        return Err(cannot(format!(
            "{target_fqn} is not a superclass or interface of {} in the project",
            info.name
        )));
    }
    let (tpath, ttext) =
        find_class(p, target_fqn).ok_or_else(|| cannot("The target class cannot be read"))?;
    let same_file = tpath == path;
    let ttext = if same_file { text.to_string() } else { ttext };
    let tfile = index_source(&tpath, &ttext);
    let tclass = tfile
        .classes
        .iter()
        .find(|c| c.fqn.eq_ignore_ascii_case(target_fqn))
        .ok_or_else(|| cannot("The target class cannot be found"))?;
    let interface = tclass.kind == crate::model::SymbolKind::Interface;
    let ids: Vec<String> = options.members.iter().map(|m| m.id.clone()).collect();
    let members = chosen(&info, &ids)?;
    if members.is_empty() {
        return Err(cannot("Select the members to pull up"));
    }
    let wants_abstract = |m: &ClassMember| {
        m.kind == "method"
            && options
                .members
                .iter()
                .any(|c| c.id == m.id && c.make_abstract)
    };
    let mut plan = ProjectPlan::default();
    let tline = line_of(&ttext, tclass.name_range.start);
    let has_member = |m: &ClassMember| {
        tclass.members.iter().any(|x| {
            let same_kind = matches!(
                (m.kind.as_str(), x.kind),
                ("method", crate::model::SymbolKind::Method)
                    | ("property", crate::model::SymbolKind::Property)
                    | ("constant", crate::model::SymbolKind::Constant)
            );
            same_kind
                && if m.kind == "method" {
                    x.name.eq_ignore_ascii_case(&m.name)
                } else {
                    x.name == m.name
                }
        })
    };
    let has_id = |id: &str| info.member(id).is_some_and(has_member);
    let tkind = if interface { "interface" } else { "class" };
    // What happens to each member: declared in the target (the class keeps it) or moved.
    let mut declared = Vec::new();
    let mut moved = Vec::new();
    for m in &members {
        if has_member(m) {
            plan.conflict(
                format!(
                    "{} {} already contains {}",
                    capitalized(tkind),
                    tclass.name,
                    describe(m)
                ),
                Some(&tpath),
                Some(tline),
            );
        }
        if interface {
            match m.kind.as_str() {
                "property" => {
                    plan.conflict(
                        format!(
                            "{} cannot be pulled up: interfaces cannot have properties",
                            subject(m)
                        ),
                        Some(path),
                        Some(line_of(text, m.span.start)),
                    );
                }
                "method" if m.visibility != "public" || m.constructor => {
                    plan.conflict(
                        format!(
                            "{} is not public and cannot be declared in interface {}",
                            subject(m),
                            tclass.name
                        ),
                        Some(path),
                        Some(line_of(text, m.span.start)),
                    );
                }
                "method" => declared.push(*m),
                _ => moved.push(*m),
            }
        } else if wants_abstract(m) {
            declared.push(*m);
        } else {
            moved.push(*m);
        }
    }
    let moving: HashSet<String> = moved.iter().map(|m| m.id.clone()).collect();
    let in_target: HashSet<String> = moved
        .iter()
        .chain(&declared)
        .map(|m| m.id.clone())
        .collect();
    for m in &moved {
        for u in &m.uses {
            if in_target.contains(u) || has_id(u) {
                continue;
            }
            let used = info.member(u);
            let what = used.map_or(u.clone(), describe);
            let message = if used.is_some_and(|x| x.visibility == "private") {
                format!(
                    "{} uses private {what}, which is not accessible from {tkind} {}",
                    subject(m),
                    tclass.name
                )
            } else {
                format!(
                    "{} uses {what}, which is not pulled up to {tkind} {}",
                    subject(m),
                    tclass.name
                )
            };
            plan.conflict(message, Some(path), Some(line_of(text, m.span.start)));
        }
    }
    let tree = parse(text).ok_or_else(|| cannot("The file cannot be parsed"))?;
    let root = tree.root_node();
    let nodes = nodes_of(root, text, &info);
    let ttree = parse(&ttext).ok_or_else(|| cannot("The target file cannot be parsed"))?;
    let tnode = class_node(
        ttree.root_node(),
        Span::new(tclass.name_range.start, tclass.name_range.end),
    )
    .ok_or_else(|| cannot("The target class cannot be found"))?;
    let close = class_body_end(
        &ttext,
        Span::new(tclass.name_range.start, tclass.name_range.end),
    )
    .ok_or_else(|| cannot("The target class has no body"))?;
    let indent = super::indent_unit(&ttext);
    let mut reloc = Relocation::new(text, &ttext, close, &tclass.name);
    let mut parts = Vec::new();
    let mut source_edits = Vec::new();
    let mut needs_abstract = false;
    for m in &members {
        let Some(node) = node_of(&nodes, &m.id) else {
            continue;
        };
        let code = if declared.iter().any(|d| d.id == m.id) {
            if interface {
                method_declaration(text, m, node, Declaration::Interface)
            } else {
                needs_abstract = true;
                method_declaration(text, m, node, Declaration::Abstract)
            }
        } else if moving.contains(&m.id) {
            source_edits.push(remove_edit(text, m.span));
            member_code(text, m, node, used_by_staying(&info, m, &moving))
        } else {
            continue;
        };
        let code = if same_file {
            code
        } else {
            reloc.code(&code, m.span.start)
        };
        parts.push(place(text, m.span.start, &code, &indent));
    }
    let mut target_edits = Vec::new();
    if !parts.is_empty() {
        target_edits.push(insert_members(&ttext, close, &parts.join("\n\n")));
    }
    if needs_abstract {
        target_edits.extend(abstract_edit(tnode));
    }
    if !same_file {
        target_edits.extend(reloc.import_edits(&ttext));
    }
    plan.edit(path, tidy(source_edits));
    plan.edit(&tpath, tidy(target_edits));
    Ok(plan)
}

// ---- Push Members Down ----

/// A direct subclass (or implementing class / sub-interface) of a class.
struct Subclass {
    path: String,
    class: HierarchyClass,
    name_span: Span,
    span: Span,
}

/// The direct subclasses of the class `info` declared in `path` (`text`), from the usages
/// of its name in `extends` / `implements`.
fn subclasses(p: &mut dyn Project, path: &str, text: &str, info: &ClassMembers) -> Vec<Subclass> {
    let Some(found) = p.usages(path, text, info.name_span.start, &UsageOptions::default()) else {
        return vec![];
    };
    let mut out: Vec<Subclass> = Vec::new();
    for (file, span, _) in usage_spans(p, &found) {
        if is_vendor(&file) {
            continue;
        }
        let t = if file == path {
            text.to_string()
        } else {
            match p.read(&file) {
                Some(t) => t,
                None => continue,
            }
        };
        let Some(tree) = parse(&t) else { continue };
        let Some(mut n) = tree
            .root_node()
            .descendant_for_byte_range(span.start, span.end)
        else {
            continue;
        };
        if n.parent().is_some_and(|x| x.kind() == "qualified_name") {
            n = n.parent().expect("checked");
        }
        let Some(clause) = n
            .parent()
            .filter(|x| matches!(x.kind(), "base_clause" | "class_interface_clause"))
        else {
            continue;
        };
        let Some(decl) = clause.parent().filter(|d| kind_of_class(*d).is_some()) else {
            continue;
        };
        let Some(name) = decl.child_by_field_name("name") else {
            continue;
        };
        let Some(c) = class_at(&t, name.start_byte()) else {
            continue;
        };
        if c.fqn.eq_ignore_ascii_case(&info.fqn)
            || out.iter().any(|s| s.class.fqn.eq_ignore_ascii_case(&c.fqn))
        {
            continue;
        }
        out.push(Subclass {
            path: file.clone(),
            class: HierarchyClass {
                name: c.name.clone(),
                fqn: c.fqn.clone(),
                kind: if c.kind == "interface" {
                    "interface"
                } else {
                    "class"
                }
                .into(),
                path: file,
            },
            name_span: c.name_span,
            span: c.span,
        });
    }
    out
}

/// The direct subclasses of the class at `offset` (Push Members Down dialog).
pub fn direct_subclasses(
    p: &mut dyn Project,
    path: &str,
    text: &str,
    info: &ClassMembers,
) -> Vec<HierarchyClass> {
    subclasses(p, path, text, info)
        .into_iter()
        .map(|s| s.class)
        .collect()
}

/// Push Members Down's choices (`make_abstract`: "Keep abstract").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PushDownOptions {
    pub members: Vec<MemberChoice>,
}

/// Push Members Down: the chosen members are copied into every direct subclass (that does
/// not declare them itself) and leave the class.
pub fn push_down(
    p: &mut dyn Project,
    path: &str,
    text: &str,
    offset: usize,
    options: &PushDownOptions,
) -> Result<ProjectPlan, String> {
    let info = class_members(text, offset).ok_or_else(|| cannot("The class cannot be found"))?;
    let ids: Vec<String> = options.members.iter().map(|m| m.id.clone()).collect();
    let members = chosen(&info, &ids)?;
    if members.is_empty() {
        return Err(cannot("Select the members to push down"));
    }
    let subs = subclasses(p, path, text, &info);
    if subs.is_empty() {
        return Err(cannot(format!(
            "{} {} has no subclasses to push members down to",
            capitalized(&info.kind),
            info.name
        )));
    }
    let interface = info.kind == "interface";
    let keep_abstract = |m: &ClassMember| {
        !interface
            && m.kind == "method"
            && options
                .members
                .iter()
                .any(|c| c.id == m.id && c.make_abstract)
    };
    let mut plan = ProjectPlan::default();
    let pushed: HashSet<String> = members
        .iter()
        .filter(|m| !keep_abstract(m))
        .map(|m| m.id.clone())
        .collect();
    // Members that stay and use a pushed member.
    for s in info.members.iter().filter(|s| !ids.contains(&s.id)) {
        for u in s.uses.iter().filter(|u| pushed.contains(*u)) {
            let what = info.member(u).map_or(u.clone(), describe);
            plan.conflict(
                format!(
                    "{} uses {what}, which is pushed down from {}",
                    subject(s),
                    info.name
                ),
                Some(path),
                Some(line_of(text, s.span.start)),
            );
        }
    }
    // Usages outside the class and its subclasses.
    for m in members.iter().filter(|m| pushed.contains(&m.id)) {
        let Some(found) = p.usages(path, text, m.name_span.start, &UsageOptions::default()) else {
            continue;
        };
        let mut reported: Vec<String> = Vec::new();
        for (file, span, _) in usage_spans(p, &found) {
            let inside = (file == path && info.span.contains(span))
                || subs.iter().any(|s| s.path == file && s.span.contains(span));
            if inside || reported.contains(&file) {
                continue;
            }
            reported.push(file.clone());
            let line = p.read(&file).map(|t| line_of(&t, span.start));
            plan.conflict(
                format!(
                    "{} is used in {file}, outside the subclasses of {}",
                    subject(m),
                    info.name
                ),
                Some(&file),
                line,
            );
        }
    }
    let tree = parse(text).ok_or_else(|| cannot("The file cannot be parsed"))?;
    let root = tree.root_node();
    let nodes = nodes_of(root, text, &info);
    let class =
        class_node(root, info.name_span).ok_or_else(|| cannot("The class cannot be found"))?;
    // The class: members leave it (abstract declarations stay).
    let mut source_edits = Vec::new();
    let mut make_abstract = false;
    for m in &members {
        let Some(node) = node_of(&nodes, &m.id) else {
            continue;
        };
        if keep_abstract(m) {
            if !m.is_abstract {
                make_abstract = true;
                source_edits.push(TextEdit {
                    start: m.span.start,
                    end: m.span.end,
                    new_text: method_declaration(text, m, node, Declaration::Abstract),
                });
            }
        } else {
            source_edits.push(remove_edit(text, m.span));
        }
    }
    if make_abstract {
        source_edits.extend(abstract_edit(class));
    }
    plan.edit(path, tidy(source_edits));
    // Every subclass gets the members it does not declare itself.
    for sub in &subs {
        let stext = if sub.path == path {
            text.to_string()
        } else {
            match p.read(&sub.path) {
                Some(t) => t,
                None => continue,
            }
        };
        let sfile = index_source(&sub.path, &stext);
        let Some(sclass) = sfile
            .classes
            .iter()
            .find(|c| c.fqn.eq_ignore_ascii_case(&sub.class.fqn))
        else {
            continue;
        };
        let Some(close) = class_body_end(&stext, sub.name_span) else {
            continue;
        };
        let indent = super::indent_unit(&stext);
        let mut reloc = Relocation::new(text, &stext, close, &sub.class.name);
        let mut parts = Vec::new();
        for m in &members {
            let declares = sclass.members.iter().any(|x| {
                if m.kind == "method" {
                    x.kind == crate::model::SymbolKind::Method
                        && x.name.eq_ignore_ascii_case(&m.name)
                } else {
                    x.name == m.name && x.kind != crate::model::SymbolKind::Method
                }
            });
            // Interface methods are implemented by the classes already.
            if declares || (interface && m.kind == "method") {
                continue;
            }
            let Some(node) = node_of(&nodes, &m.id) else {
                continue;
            };
            let code = member_code(text, m, node, false);
            let code = if sub.path == path {
                code
            } else {
                reloc.code(&code, m.span.start)
            };
            parts.push(place(text, m.span.start, &code, &indent));
        }
        if parts.is_empty() {
            continue;
        }
        let mut edits = vec![insert_members(&stext, close, &parts.join("\n\n"))];
        if sub.path != path {
            edits.extend(reloc.import_edits(&stext));
        }
        plan.edit(&sub.path, tidy(edits));
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refactor::project::tests::TestProject;
    use crate::refactor::test_util::apply;

    const COMPOSER: &str = r#"{"autoload": {"psr-4": {"App\\": "app/"}}}"#;

    const USER: &str = "<?php\n\nnamespace App\\Models;\n\nuse App\\Support\\Mailer;\n\nclass User extends Model\n{\n    public const ROLE = 'user';\n\n    private string $email = '';\n\n    public function __construct(private int $id) {}\n\n    /**\n     * The user's orders.\n     * @return Order[]\n     */\n    public function orders(): array\n    {\n        return [new Order()];\n    }\n\n    public function notify(Mailer $mailer): void\n    {\n        $mailer->send($this->email, self::ROLE);\n    }\n\n    protected function secret(): string\n    {\n        return $this->email;\n    }\n}\n";
    const MODEL: &str = "<?php\n\nnamespace App\\Models;\n\nabstract class Model\n{\n}\n";
    const ORDER: &str = "<?php\n\nnamespace App\\Models;\n\nclass Order\n{\n}\n";
    const MAILER: &str = "<?php\n\nnamespace App\\Support;\n\nclass Mailer\n{\n    public function send(string $to, string $role): void {}\n}\n";
    const SERVICE: &str = "<?php\n\nnamespace App\\Services;\n\nuse App\\Models\\User;\n\nclass Notifier\n{\n    public function ping(User $user): array\n    {\n        return $user->orders();\n    }\n\n    public function mail(User $user): string\n    {\n        return $user->secret();\n    }\n}\n";

    fn project() -> TestProject {
        TestProject::new(&[
            ("composer.json", COMPOSER),
            ("app/Models/User.php", USER),
            ("app/Models/Model.php", MODEL),
            ("app/Models/Order.php", ORDER),
            ("app/Support/Mailer.php", MAILER),
            ("app/Services/Notifier.php", SERVICE),
        ])
    }

    #[test]
    fn lists_members_with_their_dependencies() {
        let info = class_members(USER, USER.find("notify").unwrap()).unwrap();
        let ids: Vec<&str> = info.members.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "ROLE",
                "$email",
                "__construct()",
                "orders()",
                "notify()",
                "secret()"
            ]
        );
        assert_eq!(info.selected.as_deref(), Some("notify()"));
        assert_eq!(info.parent.as_deref(), Some("App\\Models\\Model"));
        let notify = info.member("notify()").unwrap();
        assert_eq!(notify.uses, ["$email", "ROLE"]);
        assert_eq!(
            notify.signature,
            "public function notify(Mailer $mailer): void"
        );
        assert!(info.member("__construct()").unwrap().constructor);
        assert!(!extractable_to_interface(info.member("secret()").unwrap()));
        assert!(extractable_to_interface(info.member("ROLE").unwrap()));
    }

    #[test]
    fn extracts_an_interface() {
        let mut p = project();
        let options = ExtractInterfaceOptions {
            name: "HasOrders".into(),
            namespace: "App\\Contracts".into(),
            directory: "app/Contracts".into(),
            members: vec!["orders()".into(), "notify()".into(), "ROLE".into()],
            replace_references: true,
        };
        let plan = extract_interface(
            &mut p,
            "app/Models/User.php",
            USER,
            USER.find("class User").unwrap(),
            &options,
            &plain_skeleton,
        )
        .unwrap();
        assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
        let out = p.apply(&plan);
        assert_eq!(
            out["app/Contracts/HasOrders.php"].as_deref().unwrap(),
            "<?php\n\nnamespace App\\Contracts;\n\nuse App\\Models\\Order;\nuse App\\Support\\Mailer;\n\ninterface HasOrders\n{\n    public const ROLE = 'user';\n\n    /**\n     * The user's orders.\n     * @return Order[]\n     */\n    public function orders(): array;\n\n    public function notify(Mailer $mailer): void;\n}\n"
        );
        let user = out["app/Models/User.php"].as_deref().unwrap();
        assert!(user.contains("use App\\Contracts\\HasOrders;\nuse App\\Support\\Mailer;"));
        assert!(
            user.contains(
                "class User extends Model implements HasOrders\n{\n    private string $email"
            ),
            "{user}"
        );
        assert!(!user.contains("ROLE = "));
        // Only the parameter whose methods are all in the interface is replaced.
        let service = out["app/Services/Notifier.php"].as_deref().unwrap();
        assert!(service.contains("use App\\Contracts\\HasOrders;\nuse App\\Models\\User;"));
        assert!(service.contains("public function ping(HasOrders $user)"));
        assert!(service.contains("public function mail(User $user)"));
    }

    #[test]
    fn refuses_bad_interface_requests() {
        let mut p = project();
        let at = USER.find("class User").unwrap();
        let mut options = ExtractInterfaceOptions {
            name: "Order".into(),
            namespace: "App\\Models".into(),
            directory: "app/Models".into(),
            members: vec![],
            replace_references: false,
        };
        let err = extract_interface(
            &mut p,
            "app/Models/User.php",
            USER,
            at,
            &options,
            &plain_skeleton,
        )
        .unwrap_err();
        assert!(
            err.contains("Class App\\Models\\Order already exists"),
            "{err}"
        );
        options.name = "1x".into();
        assert!(extract_interface(
            &mut p,
            "app/Models/User.php",
            USER,
            at,
            &options,
            &plain_skeleton
        )
        .is_err());
        options.name = "Secretive".into();
        options.members = vec!["secret()".into()];
        let err = extract_interface(
            &mut p,
            "app/Models/User.php",
            USER,
            at,
            &options,
            &plain_skeleton,
        )
        .unwrap_err();
        assert!(
            err.contains("Method secret() cannot be declared in an interface"),
            "{err}"
        );
    }

    #[test]
    fn extracts_a_superclass_between_the_class_and_its_parent() {
        let mut p = project();
        let options = ExtractSuperclassOptions {
            name: "Customer".into(),
            namespace: "App\\Domain".into(),
            directory: "app/Domain".into(),
            members: vec![
                MemberChoice {
                    id: "$email".into(),
                    make_abstract: false,
                },
                MemberChoice {
                    id: "notify()".into(),
                    make_abstract: false,
                },
                MemberChoice {
                    id: "orders()".into(),
                    make_abstract: true,
                },
            ],
        };
        let plan = extract_superclass(
            &mut p,
            "app/Models/User.php",
            USER,
            USER.find("notify").unwrap(),
            &options,
            &plain_skeleton,
        )
        .unwrap();
        // notify() uses ROLE, which stays.
        assert_eq!(plan.conflicts.len(), 1);
        assert!(plan.conflicts[0]
            .message
            .contains("Method notify() uses constant ROLE, which is not moved"));
        let out = p.apply(&plan);
        let parent = out["app/Domain/Customer.php"].as_deref().unwrap();
        assert_eq!(
            parent,
            "<?php\n\nnamespace App\\Domain;\n\nuse App\\Models\\Model;\nuse App\\Models\\Order;\nuse App\\Support\\Mailer;\n\nabstract class Customer extends Model\n{\n    protected string $email = '';\n\n    /**\n     * The user's orders.\n     * @return Order[]\n     */\n    abstract public function orders(): array;\n\n    public function notify(Mailer $mailer): void\n    {\n        $mailer->send($this->email, self::ROLE);\n    }\n}\n"
        );
        let user = out["app/Models/User.php"].as_deref().unwrap();
        assert!(user.contains("use App\\Domain\\Customer;"));
        assert!(user.contains("class User extends Customer\n"));
        assert!(!user.contains("function notify"));
        assert!(!user.contains("$email = ''"));
        assert!(user.contains("public function orders(): array\n    {"));
    }

    const BASE: &str = "<?php\n\nnamespace App\\Shapes;\n\nabstract class Shape implements HasArea\n{\n    protected string $name = 'shape';\n}\n";
    const AREA: &str = "<?php\n\nnamespace App\\Shapes;\n\ninterface HasArea\n{\n}\n";
    const CIRCLE: &str = "<?php\n\nnamespace App\\Shapes;\n\nuse App\\Units\\Meter;\n\nclass Circle extends Shape\n{\n    private float $radius = 1.0;\n\n    public function area(): float\n    {\n        return M_PI * $this->radius ** 2;\n    }\n\n    public function describe(): string\n    {\n        return $this->name . ': ' . $this->area();\n    }\n\n    public function unit(): Meter\n    {\n        return new Meter();\n    }\n}\n";
    const SQUARE: &str = "<?php\n\nnamespace App\\Shapes;\n\nclass Square extends Shape\n{\n    public function describe(): string\n    {\n        return 'square';\n    }\n}\n";
    const METER: &str = "<?php\n\nnamespace App\\Units;\n\nclass Meter\n{\n}\n";
    const USE_SHAPE: &str = "<?php\n\nnamespace App;\n\nuse App\\Shapes\\Shape;\n\nfunction show(Shape $s): string\n{\n    return $s->describe();\n}\n";

    fn shapes() -> TestProject {
        TestProject::new(&[
            ("composer.json", COMPOSER),
            ("app/Shapes/Shape.php", BASE),
            ("app/Shapes/HasArea.php", AREA),
            ("app/Shapes/Circle.php", CIRCLE),
            ("app/Shapes/Square.php", SQUARE),
            ("app/Units/Meter.php", METER),
            ("app/show.php", USE_SHAPE),
        ])
    }

    #[test]
    fn finds_the_supertypes() {
        let mut p = shapes();
        let info = class_members(CIRCLE, CIRCLE.find("area").unwrap()).unwrap();
        let names: Vec<String> = supertypes(&mut p, &info)
            .into_iter()
            .map(|s| format!("{} {}", s.kind, s.fqn))
            .collect();
        assert_eq!(
            names,
            ["class App\\Shapes\\Shape", "interface App\\Shapes\\HasArea"]
        );
    }

    #[test]
    fn pulls_members_up_into_a_class() {
        let mut p = shapes();
        let options = PullUpOptions {
            target: "App\\Shapes\\Shape".into(),
            members: vec![
                MemberChoice {
                    id: "area()".into(),
                    make_abstract: true,
                },
                MemberChoice {
                    id: "describe()".into(),
                    make_abstract: false,
                },
                MemberChoice {
                    id: "unit()".into(),
                    make_abstract: false,
                },
            ],
        };
        let plan = pull_up(
            &mut p,
            "app/Shapes/Circle.php",
            CIRCLE,
            CIRCLE.find("area").unwrap(),
            &options,
        )
        .unwrap();
        assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
        let out = p.apply(&plan);
        assert_eq!(
            out["app/Shapes/Shape.php"].as_deref().unwrap(),
            "<?php\n\nnamespace App\\Shapes;\n\nuse App\\Units\\Meter;\n\nabstract class Shape implements HasArea\n{\n    protected string $name = 'shape';\n\n    abstract public function area(): float;\n\n    public function describe(): string\n    {\n        return $this->name . ': ' . $this->area();\n    }\n\n    public function unit(): Meter\n    {\n        return new Meter();\n    }\n}\n"
        );
        let circle = out["app/Shapes/Circle.php"].as_deref().unwrap();
        assert!(circle.contains("public function area(): float"));
        assert!(!circle.contains("function describe"));
        assert!(!circle.contains("function unit"));
    }

    #[test]
    fn reports_pull_up_problems() {
        let mut p = shapes();
        // area() uses the private $radius, which stays.
        let options = PullUpOptions {
            target: "App\\Shapes\\Shape".into(),
            members: vec![MemberChoice {
                id: "area()".into(),
                make_abstract: false,
            }],
        };
        let at = CIRCLE.find("area").unwrap();
        let plan = pull_up(&mut p, "app/Shapes/Circle.php", CIRCLE, at, &options).unwrap();
        assert_eq!(plan.conflicts.len(), 1);
        assert!(plan.conflicts[0].message.contains(
            "Method area() uses private property $radius, which is not accessible from class Shape"
        ));
        // Into an interface: a declaration; properties cannot go there.
        let options = PullUpOptions {
            target: "App\\Shapes\\HasArea".into(),
            members: vec![
                MemberChoice {
                    id: "area()".into(),
                    make_abstract: false,
                },
                MemberChoice {
                    id: "$radius".into(),
                    make_abstract: false,
                },
            ],
        };
        let plan = pull_up(&mut p, "app/Shapes/Circle.php", CIRCLE, at, &options).unwrap();
        assert_eq!(plan.conflicts.len(), 1);
        assert!(plan.conflicts[0]
            .message
            .contains("interfaces cannot have properties"));
        let out = p.apply(&plan);
        assert!(out["app/Shapes/HasArea.php"]
            .as_deref()
            .unwrap()
            .contains("interface HasArea\n{\n    public function area(): float;\n}"));
        assert!(!out.contains_key("app/Shapes/Circle.php"));
        // Not a supertype.
        let options = PullUpOptions {
            target: "App\\Units\\Meter".into(),
            members: vec![],
        };
        assert!(pull_up(&mut p, "app/Shapes/Circle.php", CIRCLE, at, &options).is_err());
    }

    #[test]
    fn pushes_members_down_into_every_subclass() {
        const SHAPE: &str = "<?php\n\nnamespace App\\Shapes;\n\nuse App\\Units\\Meter;\n\nabstract class Shape\n{\n    protected string $name = 'shape';\n\n    public function unit(): Meter\n    {\n        return new Meter();\n    }\n\n    public function describe(): string\n    {\n        return $this->name;\n    }\n}\n";
        let mut p = TestProject::new(&[
            ("composer.json", COMPOSER),
            ("app/Shapes/Shape.php", SHAPE),
            ("app/Shapes/HasArea.php", AREA),
            ("app/Shapes/Circle.php", CIRCLE.replace("    public function unit(): Meter\n    {\n        return new Meter();\n    }\n", "").replace("\n\n}\n", "\n}\n").as_str()),
            ("app/Shapes/Square.php", SQUARE),
            ("app/Units/Meter.php", METER),
            ("app/show.php", USE_SHAPE),
        ]);
        let info = class_members(SHAPE, SHAPE.find("unit").unwrap()).unwrap();
        let subs: Vec<String> = direct_subclasses(&mut p, "app/Shapes/Shape.php", SHAPE, &info)
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(subs.len(), 2, "{subs:?}");
        let options = PushDownOptions {
            members: vec![
                MemberChoice {
                    id: "unit()".into(),
                    make_abstract: false,
                },
                MemberChoice {
                    id: "describe()".into(),
                    make_abstract: true,
                },
            ],
        };
        let plan = push_down(
            &mut p,
            "app/Shapes/Shape.php",
            SHAPE,
            SHAPE.find("unit").unwrap(),
            &options,
        )
        .unwrap();
        assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
        let out = p.apply(&plan);
        assert_eq!(
            out["app/Shapes/Shape.php"].as_deref().unwrap(),
            "<?php\n\nnamespace App\\Shapes;\n\nuse App\\Units\\Meter;\n\nabstract class Shape\n{\n    protected string $name = 'shape';\n\n    abstract public function describe(): string;\n}\n"
        );
        let square = out["app/Shapes/Square.php"].as_deref().unwrap();
        // Square declares describe() itself; it gets unit() with the import.
        assert!(square.contains("use App\\Units\\Meter;"));
        assert!(square.contains(
            "    public function unit(): Meter\n    {\n        return new Meter();\n    }\n}\n"
        ));
        assert_eq!(square.matches("function describe").count(), 1);
        let circle = out["app/Shapes/Circle.php"].as_deref().unwrap();
        assert!(circle.contains("public function unit(): Meter"));
    }

    #[test]
    fn reports_push_down_problems() {
        let mut p = shapes();
        let options = PushDownOptions {
            members: vec![MemberChoice {
                id: "$name".into(),
                make_abstract: false,
            }],
        };
        let plan = push_down(
            &mut p,
            "app/Shapes/Shape.php",
            BASE,
            BASE.find("$name").unwrap(),
            &options,
        )
        .unwrap();
        assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
        // Circle and Square both get $name.
        let out = p.apply(&plan);
        assert!(out["app/Shapes/Square.php"]
            .as_deref()
            .unwrap()
            .contains("protected string $name = 'shape';"));
        // A class without subclasses.
        let plan = push_down(
            &mut p,
            "app/Shapes/Square.php",
            SQUARE,
            SQUARE.find("describe").unwrap(),
            &PushDownOptions {
                members: vec![MemberChoice {
                    id: "describe()".into(),
                    make_abstract: false,
                }],
            },
        );
        assert!(plan.unwrap_err().contains("has no subclasses"));
    }

    #[test]
    fn declarations_and_relocation() {
        let src = "<?php\nnamespace A;\nuse B\\C;\nclass X {\n    /** Doc. */\n    #[Attr]\n    final private static function f(C $c): ?C { return $c; }\n}\n";
        let info = class_members(src, src.find("function f").unwrap()).unwrap();
        let tree = parse(src).unwrap();
        let nodes = nodes_of(tree.root_node(), src, &info);
        let m = &info.members[0];
        let node = node_of(&nodes, &m.id).unwrap();
        assert_eq!(
            method_declaration(src, m, node, Declaration::Abstract),
            "/** Doc. */\n    abstract protected static function f(C $c): ?C;"
        );
        assert_eq!(
            method_declaration(src, m, node, Declaration::Interface),
            "/** Doc. */\n    public static function f(C $c): ?C;"
        );
        let target = "<?php\nnamespace D;\nuse E\\C;\nclass Y {}\n";
        let mut r = Relocation::new(src, target, 40, "Y");
        assert_eq!(
            r.code("function f(C $c, X $x) {}", 60),
            "function f(\\B\\C $c, X $x) {}"
        );
        assert_eq!(r.imports, ["A\\X"]);
        let edits = r.import_edits(target);
        assert_eq!(
            apply(target, &edits),
            "<?php\nnamespace D;\nuse A\\X;\nuse E\\C;\nclass Y {}\n"
        );
    }
}
