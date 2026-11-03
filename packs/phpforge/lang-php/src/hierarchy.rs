//! Type, Method and Call Hierarchy on the PHP index, used when no language server answers
//! (none running, or one without hierarchies). The answers are the data model of
//! [`forge_codeintel::hierarchy`], which computes the views:
//!
//! - [`HierarchyQuery::Type`]: the class at the caret with its supertypes (parents,
//!   interfaces, traits; vendor classes too) and every project class below it.
//! - [`HierarchyQuery::Method`]: the same for the class declaring the method at the caret,
//!   plus the subtypes of the method's topmost declaration, each with the method's presence.
//! - [`HierarchyQuery::Call`] the method or function at the caret, [`HierarchyQuery::Callers`]
//!   the methods and functions calling it (grouped, with their call sites) and
//!   [`HierarchyQuery::Callees`] the ones it calls.
//!
//! Offsets inside are UTF-8 bytes; positions handed out are LSP positions.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use forge_codeintel::hierarchy::{
    CallNode, CallSite, HierarchyItem, HierarchyKind, MethodPresence, TypeEntry, TypeGraph,
};
use forge_codeintel::{classify_usage, Position, UsageKind};
use serde::{Deserialize, Serialize};

use crate::hover::word_at;
use crate::index::ProjectIndex;
use crate::model::{short_name, ClassLike, FileIndex, Member, SymbolKind};
use crate::navigate::{
    find_usages, is_keyword, resolve_word, symbol_at, Lines, Resolved, SymbolRef,
};
use crate::parser;

/// What to compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HierarchyQuery {
    Type,
    Method,
    /// The method or function a Call Hierarchy starts at.
    Call,
    Callers,
    Callees,
}

/// The index's answer to a [`HierarchyQuery`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PhpHierarchy {
    /// Type and Method Hierarchy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph: Option<TypeGraph>,
    /// `PaymentGateway`, `PaymentGateway::charge()`, `PaymentService::pay()`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Call Hierarchy: the function at the caret.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<HierarchyItem>,
    /// Callers or callees.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<CallNode>,
}

/// File contents: the editor buffer of `rel`, the rest from disk.
struct Texts {
    root: PathBuf,
    rel: String,
    src: String,
    cache: HashMap<String, Option<String>>,
}

impl Texts {
    fn new(index: &ProjectIndex, rel: &str, src: &str) -> Self {
        Texts {
            root: index.root().to_path_buf(),
            rel: rel.to_string(),
            src: src.to_string(),
            cache: HashMap::new(),
        }
    }

    fn get(&mut self, path: &str) -> Option<String> {
        if path == self.rel {
            return Some(self.src.clone());
        }
        let root = &self.root;
        self.cache
            .entry(path.to_string())
            .or_insert_with(|| std::fs::read_to_string(root.join(path)).ok())
            .clone()
    }

    fn position(&mut self, path: &str, byte: usize) -> Option<Position> {
        let text = self.get(path)?;
        Some(Lines::new(&text).position(byte))
    }
}

fn is_vendor(path: &str) -> bool {
    path.starts_with("vendor/") || path.contains("/vendor/")
}

fn namespace_of(fqn: &str) -> Option<String> {
    fqn.rsplit_once('\\').map(|(ns, _)| ns.to_string())
}

fn type_kind(kind: SymbolKind) -> HierarchyKind {
    match kind {
        SymbolKind::Interface => HierarchyKind::Interface,
        SymbolKind::Trait => HierarchyKind::Trait,
        SymbolKind::Enum => HierarchyKind::Enum,
        _ => HierarchyKind::Class,
    }
}

fn type_item(texts: &mut Texts, path: Option<&str>, c: &ClassLike) -> HierarchyItem {
    let pos = path
        .and_then(|p| texts.position(p, c.name_range.start))
        .unwrap_or(Position::new(c.name_range.line, c.name_range.column));
    HierarchyItem {
        id: c.fqn.clone(),
        name: c.name.clone(),
        kind: type_kind(c.kind),
        container: (!c.namespace.is_empty()).then(|| c.namespace.clone()),
        detail: None,
        path: path.unwrap_or_default().to_string(),
        line: pos.line,
        column: pos.character,
        is_abstract: c.modifiers.is_abstract,
        handle: None,
    }
}

/// A type the index knows only by name (not indexed, e.g. vendor code that is skipped).
fn named_type(fqn: &str, kind: HierarchyKind) -> HierarchyItem {
    HierarchyItem {
        id: fqn.to_string(),
        name: short_name(fqn).to_string(),
        kind,
        container: namespace_of(fqn),
        ..Default::default()
    }
}

/// Builds a [`TypeGraph`] from the index.
struct GraphBuilder<'a> {
    index: &'a mut ProjectIndex,
    current: FileIndex,
    texts: Texts,
    entries: Vec<TypeEntry>,
    classes: Vec<Option<ClassLike>>,
    /// Lowercase FQN -> entry.
    ids: HashMap<String, usize>,
    /// Project classes (path, class), the current file's unsaved state included.
    project: Vec<(String, ClassLike)>,
}

impl<'a> GraphBuilder<'a> {
    fn new(index: &'a mut ProjectIndex, rel: &str, src: &str) -> Self {
        let current = parser::index_source(rel, src);
        let mut project: Vec<(String, ClassLike)> = index
            .files()
            .filter(|f| f.path != rel && !is_vendor(&f.path))
            .flat_map(|f| f.classes.iter().map(|c| (f.path.clone(), c.clone())))
            .collect();
        project.extend(current.classes.iter().map(|c| (rel.to_string(), c.clone())));
        project.sort_by(|a, b| (&a.0, a.1.range.start).cmp(&(&b.0, b.1.range.start)));
        let texts = Texts::new(index, rel, src);
        GraphBuilder {
            index,
            current,
            texts,
            entries: vec![],
            classes: vec![],
            ids: HashMap::new(),
            project,
        }
    }

    /// The class `fqn` and the file declaring it (the current file's unsaved state first).
    fn class(&mut self, fqn: &str) -> Option<(Option<String>, ClassLike)> {
        let fqn = fqn.trim_start_matches('\\');
        if let Some(c) = self
            .current
            .classes
            .iter()
            .find(|c| c.fqn.eq_ignore_ascii_case(fqn))
        {
            return Some((Some(self.current.path.clone()), c.clone()));
        }
        let c = self.index.resolve_class(fqn)?;
        let path = self.index.class_path(&c.fqn).map(str::to_string);
        Some((path, c))
    }

    /// Adds the class (or a placeholder for an unknown one); returns its entry.
    fn add(&mut self, fqn: &str, kind: HierarchyKind) -> usize {
        let key = fqn.trim_start_matches('\\').to_ascii_lowercase();
        if let Some(i) = self.ids.get(&key) {
            return *i;
        }
        let (entry, class) = match self.class(fqn) {
            Some((path, c)) => {
                let (extends, implements) = if c.kind == SymbolKind::Interface {
                    (c.extends.clone(), vec![])
                } else {
                    (
                        c.extends.clone(),
                        c.implements.iter().chain(&c.traits).cloned().collect(),
                    )
                };
                let item = type_item(&mut self.texts, path.as_deref(), &c);
                (
                    TypeEntry {
                        item,
                        extends,
                        implements,
                        method: None,
                    },
                    Some(c),
                )
            }
            None => (
                TypeEntry {
                    item: named_type(fqn.trim_start_matches('\\'), kind),
                    ..Default::default()
                },
                None,
            ),
        };
        self.entries.push(entry);
        self.classes.push(class);
        self.ids.insert(key, self.entries.len() - 1);
        self.entries.len() - 1
    }

    /// Adds the class and all its supertypes.
    fn add_with_ancestors(&mut self, fqn: &str) {
        let mut queue: VecDeque<(String, HierarchyKind)> = VecDeque::new();
        queue.push_back((fqn.to_string(), HierarchyKind::Class));
        let mut seen = HashSet::new();
        while let Some((name, kind)) = queue.pop_front() {
            if !seen.insert(name.to_ascii_lowercase()) || seen.len() > 256 {
                continue;
            }
            let i = self.add(&name, kind);
            let Some(c) = self.classes[i].clone() else {
                continue;
            };
            let parent_kind = if c.kind == SymbolKind::Interface {
                HierarchyKind::Interface
            } else {
                HierarchyKind::Class
            };
            for e in &c.extends {
                queue.push_back((e.clone(), parent_kind));
            }
            for e in &c.implements {
                queue.push_back((e.clone(), HierarchyKind::Interface));
            }
            for e in &c.traits {
                queue.push_back((e.clone(), HierarchyKind::Trait));
            }
        }
    }

    /// Adds every project class below `fqn`.
    fn add_subtypes(&mut self, fqn: &str) {
        let mut children: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, (_, c)) in self.project.iter().enumerate() {
            for s in c.extends.iter().chain(&c.implements).chain(&c.traits) {
                children
                    .entry(s.trim_start_matches('\\').to_ascii_lowercase())
                    .or_default()
                    .push(i);
            }
        }
        let mut queue: VecDeque<String> = VecDeque::from([fqn.to_ascii_lowercase()]);
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(f) = queue.pop_front() {
            if !seen.insert(f.clone()) || seen.len() > 2000 {
                continue;
            }
            for &i in children.get(&f).into_iter().flatten() {
                let fqn = self.project[i].1.fqn.clone();
                self.add(&fqn, HierarchyKind::Class);
                queue.push_back(fqn.to_ascii_lowercase());
            }
        }
    }

    /// The graph, with supertype references pointing at the entries' ids.
    fn finish(mut self, root: &str) -> TypeGraph {
        let ids: HashMap<String, String> = self
            .entries
            .iter()
            .map(|e| (e.item.id.to_ascii_lowercase(), e.item.id.clone()))
            .collect();
        let fix = |names: &mut Vec<String>| {
            for n in names.iter_mut() {
                let key = n.trim_start_matches('\\').to_ascii_lowercase();
                if let Some(id) = ids.get(&key) {
                    *n = id.clone();
                }
            }
        };
        for e in &mut self.entries {
            fix(&mut e.extends);
            fix(&mut e.implements);
        }
        let root = ids
            .get(&root.trim_start_matches('\\').to_ascii_lowercase())
            .cloned()
            .unwrap_or_else(|| root.to_string());
        TypeGraph {
            root,
            types: self.entries,
            method: None,
        }
    }
}

/// The class Ctrl+H opens: the class named at the caret, else the class around it.
fn class_at_caret(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    offset: usize,
) -> Option<String> {
    match symbol_at(index, file, src, offset) {
        Some(SymbolRef::Class { fqn }) => Some(fqn),
        _ => file.class_at(offset).map(|c| c.fqn.clone()),
    }
}

/// Type Hierarchy of the class at `offset`.
pub fn type_hierarchy(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
) -> PhpHierarchy {
    let file = parser::index_source(rel, src);
    let Some(fqn) = class_at_caret(index, &file, src, offset) else {
        return PhpHierarchy::default();
    };
    let mut b = GraphBuilder::new(index, rel, src);
    b.add_with_ancestors(&fqn);
    b.add_subtypes(&fqn);
    let graph = b.finish(&fqn);
    PhpHierarchy {
        title: Some(short_name(&graph.root).to_string()),
        graph: Some(graph),
        ..Default::default()
    }
}

fn method_presence(c: &ClassLike, name: &str) -> Option<MethodPresence> {
    let m = c.method(name)?;
    Some(
        if m.modifiers.is_abstract || c.kind == SymbolKind::Interface {
            MethodPresence::Abstract
        } else {
            MethodPresence::Defined
        },
    )
}

/// The method at the caret: a method named there, else the method around the caret.
fn method_at_caret(
    index: &mut ProjectIndex,
    file: &FileIndex,
    src: &str,
    offset: usize,
) -> Option<(String, String)> {
    match symbol_at(index, file, src, offset) {
        Some(SymbolRef::Member {
            class,
            member: SymbolKind::Method,
            name,
        }) => Some((class, name)),
        _ => {
            let c = file.class_at(offset)?;
            let m = c.methods().find(|m| m.range.contains(offset))?;
            Some((c.fqn.clone(), m.name.clone()))
        }
    }
}

/// Method Hierarchy of the method at `offset`.
pub fn method_hierarchy(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
) -> PhpHierarchy {
    let file = parser::index_source(rel, src);
    let Some((class, method)) = method_at_caret(index, &file, src, offset) else {
        return PhpHierarchy::default();
    };
    let mut b = GraphBuilder::new(index, rel, src);
    b.add_with_ancestors(&class);
    // The subtypes of every declaration above, so siblings show up too.
    let seeds: Vec<String> = (0..b.entries.len())
        .filter(|&i| {
            b.classes[i]
                .as_ref()
                .is_some_and(|c| c.method(&method).is_some())
        })
        .map(|i| b.entries[i].item.id.clone())
        .collect();
    for s in seeds.iter().chain([&class]) {
        b.add_subtypes(s);
    }
    // Their supertypes too: a trait or parent may provide the method.
    let mut i = 0;
    while i < b.entries.len() {
        let id = b.entries[i].item.id.clone();
        if b.classes[i].is_some() {
            b.add_with_ancestors(&id);
        }
        i += 1;
    }
    for (e, c) in b.entries.iter_mut().zip(&b.classes) {
        e.method = c.as_ref().and_then(|c| method_presence(c, &method));
    }
    let mut graph = b.finish(&class);
    let title = format!("{}::{}()", short_name(&graph.root), method);
    graph.method = Some(method);
    PhpHierarchy {
        graph: Some(graph),
        title: Some(title),
        ..Default::default()
    }
}

// ---- Call Hierarchy ----

fn member_item(
    texts: &mut Texts,
    path: Option<&str>,
    owner: &ClassLike,
    m: &Member,
) -> HierarchyItem {
    let pos = path
        .and_then(|p| texts.position(p, m.name_range.start))
        .unwrap_or(Position::new(m.name_range.line, m.name_range.column));
    HierarchyItem {
        id: format!("{}::{}()", owner.fqn, m.name),
        name: format!("{}()", m.name),
        kind: if m.name.eq_ignore_ascii_case("__construct") {
            HierarchyKind::Constructor
        } else {
            HierarchyKind::Method
        },
        container: Some(owner.fqn.clone()),
        detail: None,
        path: path.unwrap_or_default().to_string(),
        line: pos.line,
        column: pos.character,
        is_abstract: m.modifiers.is_abstract || owner.kind == SymbolKind::Interface,
        handle: None,
    }
}

fn function_item(texts: &mut Texts, path: &str, f: &crate::model::FunctionDecl) -> HierarchyItem {
    let pos = texts
        .position(path, f.name_range.start)
        .unwrap_or(Position::new(f.name_range.line, f.name_range.column));
    HierarchyItem {
        id: format!("{}()", f.fqn),
        name: format!("{}()", f.name),
        kind: HierarchyKind::Function,
        container: (!f.namespace.is_empty()).then(|| f.namespace.clone()),
        path: path.to_string(),
        line: pos.line,
        column: pos.character,
        ..Default::default()
    }
}

/// The declaration of a method (in `class` or the nearest ancestor declaring it).
fn method_decl(
    index: &mut ProjectIndex,
    current: &FileIndex,
    texts: &mut Texts,
    class: &str,
    name: &str,
) -> Option<HierarchyItem> {
    let fqn = class.trim_start_matches('\\');
    let (path, owner) = match current
        .classes
        .iter()
        .find(|c| c.fqn.eq_ignore_ascii_case(fqn))
    {
        Some(c) => (Some(current.path.clone()), c.clone()),
        None => {
            let c = index.resolve_class(fqn)?;
            (index.class_path(&c.fqn).map(str::to_string), c)
        }
    };
    if let Some(m) = owner.method(name) {
        return Some(member_item(texts, path.as_deref(), &owner, m));
    }
    for anc in index.ancestors(&owner) {
        if let Some(m) = anc.method(name) {
            let path = index.class_path(&anc.fqn).map(str::to_string);
            return Some(member_item(texts, path.as_deref(), &anc, m));
        }
    }
    None
}

fn function_decl(
    index: &ProjectIndex,
    current: &FileIndex,
    texts: &mut Texts,
    fqn: &str,
) -> Option<HierarchyItem> {
    if let Some(f) = current
        .functions
        .iter()
        .find(|f| f.fqn.eq_ignore_ascii_case(fqn))
    {
        return Some(function_item(texts, &current.path.clone(), f));
    }
    let (path, f) = index.files().find_map(|file| {
        file.functions
            .iter()
            .find(|f| f.fqn.eq_ignore_ascii_case(fqn))
            .map(|f| (file.path.clone(), f.clone()))
    })?;
    Some(function_item(texts, &path, &f))
}

/// The method or function declared at, named at or around `offset`.
fn callable_at(
    index: &mut ProjectIndex,
    file: &FileIndex,
    texts: &mut Texts,
    src: &str,
    offset: usize,
) -> Option<HierarchyItem> {
    match symbol_at(index, file, src, offset) {
        Some(SymbolRef::Member {
            class,
            member: SymbolKind::Method,
            name,
        }) => return method_decl(index, file, texts, &class, &name),
        Some(SymbolRef::Function { fqn }) => return function_decl(index, file, texts, &fqn),
        _ => {}
    }
    if let Some(c) = file.class_at(offset) {
        if let Some(m) = c.methods().find(|m| m.range.contains(offset)) {
            return Some(member_item(texts, Some(&file.path), c, m));
        }
    }
    file.functions
        .iter()
        .find(|f| f.range.contains(offset))
        .map(|f| function_item(texts, &file.path, f))
}

/// The byte range of the body of the callable declared at `item`.
fn callable_range(file: &FileIndex, item: &HierarchyItem, src: &str) -> Option<(usize, usize)> {
    let lines = Lines::new(src);
    let at = |r: &crate::model::Range| {
        let p = lines.position(r.start);
        p.line == item.line && p.character == item.column
    };
    for c in &file.classes {
        if let Some(m) = c.methods().find(|m| at(&m.name_range)) {
            return Some((m.range.start, m.range.end.min(src.len())));
        }
    }
    file.functions
        .iter()
        .find(|f| at(&f.name_range))
        .map(|f| (f.range.start, f.range.end.min(src.len())))
}

fn line_text(text: &str, at: usize) -> &str {
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
    &text[start..end]
}

/// Whether `(` follows the word ending at `end` (a call, not a mere reference).
fn is_call(text: &str, end: usize) -> bool {
    text[end..].trim_start().starts_with('(')
}

/// Groups call sites by the item they belong to, in order of appearance.
fn group_calls(found: Vec<(HierarchyItem, CallSite)>) -> Vec<CallNode> {
    let mut out: Vec<CallNode> = Vec::new();
    for (item, site) in found {
        match out.iter_mut().find(|n| n.item.id == item.id) {
            Some(n) => {
                if !n.calls.contains(&site) {
                    n.calls.push(site);
                }
            }
            None => out.push(CallNode {
                item,
                calls: vec![site],
            }),
        }
    }
    out
}

/// The item a call at `offset` of `file` is made from: a method, a function, or the file's
/// top-level code.
fn caller_at(texts: &mut Texts, file: &FileIndex, offset: usize) -> HierarchyItem {
    if let Some(c) = file.class_at(offset) {
        if let Some(m) = c.methods().find(|m| m.range.contains(offset)) {
            return member_item(texts, Some(&file.path), c, m);
        }
    }
    if let Some(f) = file.functions.iter().find(|f| f.range.contains(offset)) {
        return function_item(texts, &file.path, f);
    }
    HierarchyItem {
        id: file.path.clone(),
        name: file
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&file.path)
            .to_string(),
        kind: HierarchyKind::File,
        path: file.path.clone(),
        ..Default::default()
    }
}

/// The methods and functions calling the callable at `offset`, with their call sites.
pub fn callers(index: &mut ProjectIndex, rel: &str, src: &str, offset: usize) -> Vec<CallNode> {
    let Some(found) = find_usages(index, rel, src, offset) else {
        return vec![];
    };
    if !matches!(
        found.symbol,
        SymbolRef::Member {
            member: SymbolKind::Method,
            ..
        } | SymbolRef::Function { .. }
    ) {
        return vec![];
    }
    let mut texts = Texts::new(index, rel, src);
    let current = parser::index_source(rel, src);
    let mut files: HashMap<String, FileIndex> = HashMap::new();
    let mut calls = Vec::new();
    for u in found.usages.into_iter().filter(|u| u.kind.is_none()) {
        let Some(text) = texts.get(&u.path) else {
            continue;
        };
        let start = byte_at(&text, u.range.start);
        let end = byte_at(&text, u.range.end);
        if !is_call(&text, end) {
            continue;
        }
        let file = files.entry(u.path.clone()).or_insert_with(|| {
            if u.path == rel {
                current.clone()
            } else {
                index
                    .file(&u.path)
                    .cloned()
                    .unwrap_or_else(|| parser::index_source(&u.path, &text))
            }
        });
        let item = caller_at(&mut texts, file, start);
        calls.push((
            item,
            CallSite {
                path: u.path.clone(),
                range: u.range,
                preview: line_text(&text, start).trim().to_string(),
            },
        ));
    }
    group_calls(calls)
}

fn byte_at(text: &str, pos: Position) -> usize {
    let utf16 = forge_codeintel::utf16_offset(text, pos);
    crate::offsets::utf16_to_byte(text, utf16)
}

const NOT_CALLS: &[&str] = &[
    "array", "list", "isset", "empty", "unset", "eval", "exit", "die", "fn", "function", "match",
    "self", "static", "parent",
];

/// Methods and functions the callable at `offset` calls, with the places it calls them.
pub fn callees(index: &mut ProjectIndex, rel: &str, src: &str, offset: usize) -> Vec<CallNode> {
    let file = parser::index_source(rel, src);
    let mut texts = Texts::new(index, rel, src);
    let Some(item) = callable_at(index, &file, &mut texts, src, offset) else {
        return vec![];
    };
    let Some((start, end)) = callable_range(&file, &item, src) else {
        return vec![];
    };
    let lines = Lines::new(src);
    let name_at = {
        let lines_starts: Vec<usize> = std::iter::once(0)
            .chain(src.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        lines_starts
            .get(item.line as usize)
            .map(|s| s + item.column as usize)
    };
    let body = &src[start..end];
    let mut found = Vec::new();
    let mut i = 0;
    let bytes = body.as_bytes();
    while i < bytes.len() {
        let c = bytes[i];
        if !(c.is_ascii_alphabetic() || c == b'_') {
            i += 1;
            continue;
        }
        let s = i;
        while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
            i += 1;
        }
        let prev = body[..s].chars().next_back();
        if prev.is_some_and(|p| p == '$' || p.is_alphanumeric() || p == '_') {
            continue;
        }
        let (ws, we) = (start + s, start + i);
        let word = &src[ws..we];
        if !is_call(src, we)
            || Some(ws) == name_at
            || NOT_CALLS.iter().any(|k| k.eq_ignore_ascii_case(word))
            || (is_keyword(word) && !src[..ws].trim_end().ends_with("->"))
        {
            continue;
        }
        let line = line_text(src, ws);
        let col = |b: usize| {
            let line_start = src[..b].rfind('\n').map_or(0, |n| n + 1);
            src[line_start..b].encode_utf16().count() as u32
        };
        if matches!(
            classify_usage(line, col(ws), col(we)),
            UsageKind::Comment | UsageKind::StringLiteral | UsageKind::Doc
        ) {
            continue;
        }
        let Some((rs, re)) = word_at(src, ws) else {
            continue;
        };
        let callee = match resolve_word(index, &file, src, rs, re) {
            Resolved::Symbol(SymbolRef::Member {
                class,
                member: SymbolKind::Method,
                name,
            }) => method_decl(index, &file, &mut texts, &class, &name),
            Resolved::Symbol(SymbolRef::Function { fqn }) => {
                function_decl(index, &file, &mut texts, &fqn)
            }
            // `new PaymentFailed(...)`: the constructor.
            Resolved::Symbol(SymbolRef::Class { fqn }) if src[..rs].trim_end().ends_with("new") => {
                method_decl(index, &file, &mut texts, &fqn, "__construct")
            }
            Resolved::External(e) => external_item(&e, word),
            _ => None,
        };
        if let Some(callee) = callee {
            found.push((
                callee,
                CallSite {
                    path: rel.to_string(),
                    range: lines.range(ws, we),
                    preview: line.trim().to_string(),
                },
            ));
        }
    }
    group_calls(found)
}

/// A callee that is not in the project (PHP's own functions, library methods).
fn external_item(e: &crate::navigate::ExternalSymbol, word: &str) -> Option<HierarchyItem> {
    let (name, container, kind) = match &e.member {
        Some(m) if m.kind == SymbolKind::Method => (
            m.name.clone(),
            e.classes.first().cloned(),
            HierarchyKind::Method,
        ),
        Some(_) => return None,
        None => {
            let f = e.functions.last()?;
            (short_name(f).to_string(), None, HierarchyKind::Function)
        }
    };
    if !name.eq_ignore_ascii_case(word) {
        return None;
    }
    Some(HierarchyItem {
        id: format!(
            "{}{}()",
            container
                .as_ref()
                .map(|c| format!("{c}::"))
                .unwrap_or_default(),
            name
        ),
        name: format!("{name}()"),
        kind,
        container,
        ..Default::default()
    })
}

/// Type, Method or Call Hierarchy at `offset` (bytes) of `src`, the content of `rel`.
pub fn hierarchy(
    index: &mut ProjectIndex,
    rel: &str,
    src: &str,
    offset: usize,
    query: HierarchyQuery,
) -> PhpHierarchy {
    match query {
        HierarchyQuery::Type => type_hierarchy(index, rel, src, offset),
        HierarchyQuery::Method => method_hierarchy(index, rel, src, offset),
        HierarchyQuery::Call => {
            let file = parser::index_source(rel, src);
            let mut texts = Texts::new(index, rel, src);
            let root = callable_at(index, &file, &mut texts, src, offset);
            PhpHierarchy {
                title: root.as_ref().map(|r| match &r.container {
                    Some(c) if r.kind != HierarchyKind::Function => {
                        format!("{}::{}", short_name(c), r.name)
                    }
                    _ => r.name.clone(),
                }),
                root,
                ..Default::default()
            }
        }
        HierarchyQuery::Callers => PhpHierarchy {
            calls: callers(index, rel, src, offset),
            ..Default::default()
        },
        HierarchyQuery::Callees => PhpHierarchy {
            calls: callees(index, rel, src, offset),
            ..Default::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::IndexConfig;
    use forge_codeintel::hierarchy::{
        call_view, method_view, type_view, CallViewRequest, HierarchyNode, HierarchyScope,
        MethodViewRequest, TypeView, TypeViewRequest,
    };
    use std::fs;

    const GATEWAY: &str = "<?php\nnamespace App\\Contracts;\n\ninterface Gateway\n{\n    public function charge(int $cents): bool;\n}\n";
    const BASE: &str = "<?php\nnamespace App\\Payments;\n\nuse App\\Contracts\\Gateway;\n\nabstract class BaseGateway implements Gateway\n{\n    abstract protected function name(): string;\n}\n";
    const LOGS: &str = "<?php\nnamespace App\\Payments;\n\ntrait LogsCharges\n{\n    public function charge(int $cents): bool { return true; }\n}\n";
    const STRIPE: &str = "<?php\nnamespace App\\Payments;\n\nfinal class Stripe extends BaseGateway\n{\n    public function charge(int $cents): bool\n    {\n        $ok = $this->send($cents);\n        return $ok && \\is_int(round($cents));\n    }\n\n    private function send(int $cents): bool { return true; }\n\n    protected function name(): string { return 'stripe'; }\n}\n";
    const BROKEN: &str = "<?php\nnamespace App\\Payments;\n\nclass Broken extends BaseGateway\n{\n    protected function name(): string { return 'broken'; }\n}\n";
    const LOGGED: &str = "<?php\nnamespace App\\Payments;\n\nclass Logged extends BaseGateway\n{\n    use LogsCharges;\n\n    protected function name(): string { return 'logged'; }\n}\n";
    const SERVICE: &str = "<?php\nnamespace App\\Payments;\n\nuse App\\Contracts\\Gateway;\n\nfinal class PaymentService\n{\n    public function __construct(private Gateway $gateway) {}\n\n    public function pay(int $cents): void\n    {\n        // $this->gateway->charge(1) in a comment\n        if (!$this->gateway->charge($cents)) {\n            $this->gateway->charge($cents);\n        }\n    }\n}\n";
    const TEST: &str = "<?php\nnamespace Tests;\n\nuse App\\Payments\\PaymentService;\nuse App\\Payments\\Stripe;\n\nfinal class PayTest\n{\n    public function test_pay(): void\n    {\n        $service = new PaymentService(new Stripe());\n        $service->pay(100);\n    }\n}\n";
    const FAKE: &str = "<?php\nnamespace Tests;\n\nuse App\\Payments\\Stripe;\n\nclass FakeStripe extends Stripe\n{\n}\n";

    fn setup() -> (tempfile::TempDir, ProjectIndex) {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        for (p, t) in [
            ("app/Contracts/Gateway.php", GATEWAY),
            ("app/Payments/BaseGateway.php", BASE),
            ("app/Payments/LogsCharges.php", LOGS),
            ("app/Payments/Stripe.php", STRIPE),
            ("app/Payments/Broken.php", BROKEN),
            ("app/Payments/Logged.php", LOGGED),
            ("app/Payments/PaymentService.php", SERVICE),
            ("tests/PayTest.php", TEST),
            ("tests/FakeStripe.php", FAKE),
        ] {
            fs::create_dir_all(r.join(p).parent().unwrap()).unwrap();
            fs::write(r.join(p), t).unwrap();
        }
        let mut idx = ProjectIndex::new(
            r,
            IndexConfig {
                cache_file: Some(r.join("cache.json")),
                ..Default::default()
            },
        );
        idx.index_workspace(|_, _| {});
        (dir, idx)
    }

    fn at(src: &str, needle: &str) -> usize {
        src.find(needle).expect("needle") + 1
    }

    fn tree(nodes: &[HierarchyNode]) -> String {
        fn walk(n: &HierarchyNode, depth: usize, out: &mut Vec<String>) {
            let mark = n.mark.map(|m| format!(" {m:?}")).unwrap_or_default();
            out.push(format!("{}{}{mark}", "  ".repeat(depth), n.item.name));
            for c in &n.children {
                walk(c, depth + 1, out);
            }
        }
        let mut out = vec![];
        for n in nodes {
            walk(n, 0, &mut out);
        }
        out.join("\n")
    }

    fn view(g: &TypeGraph, v: TypeView) -> String {
        tree(
            &type_view(&TypeViewRequest {
                graph: g.clone(),
                view: v,
                scope: HierarchyScope::All,
                sort_alphabetically: true,
            })
            .roots,
        )
    }

    #[test]
    fn type_hierarchy_of_classes_interfaces_and_traits() {
        let (_d, mut idx) = setup();
        let h = hierarchy(
            &mut idx,
            "app/Payments/Stripe.php",
            STRIPE,
            at(STRIPE, "Stripe extends"),
            HierarchyQuery::Type,
        );
        let g = h.graph.unwrap();
        assert_eq!(h.title.as_deref(), Some("Stripe"));
        assert_eq!(g.root, "App\\Payments\\Stripe");
        assert_eq!(
            view(&g, TypeView::Class),
            "BaseGateway\n  Stripe\n    FakeStripe"
        );
        assert_eq!(
            view(&g, TypeView::Supertypes),
            "Stripe\n  BaseGateway\n    Gateway"
        );
        let stripe = g.types.iter().find(|t| t.item.id == g.root).unwrap();
        assert_eq!((stripe.item.line, stripe.item.column), (3, 12));
        assert_eq!(stripe.item.path, "app/Payments/Stripe.php");

        // From a reference: the interface and everything implementing it.
        let h = hierarchy(
            &mut idx,
            "app/Payments/PaymentService.php",
            SERVICE,
            at(SERVICE, "Gateway $gateway"),
            HierarchyQuery::Type,
        );
        let g = h.graph.unwrap();
        assert_eq!(
            view(&g, TypeView::Subtypes),
            "Gateway\n  BaseGateway\n    Broken\n    Logged\n    Stripe\n      FakeStripe"
        );
        // A trait's subtypes are the classes using it.
        let h = hierarchy(
            &mut idx,
            "app/Payments/LogsCharges.php",
            LOGS,
            at(LOGS, "LogsCharges"),
            HierarchyQuery::Type,
        );
        assert_eq!(
            view(&h.graph.unwrap(), TypeView::Subtypes),
            "LogsCharges\n  Logged"
        );
        // Outside a class there is nothing.
        assert!(hierarchy(
            &mut idx,
            "app/Payments/Stripe.php",
            STRIPE,
            2,
            HierarchyQuery::Type
        )
        .graph
        .is_none());
    }

    #[test]
    fn method_hierarchy_marks_definitions() {
        let (_d, mut idx) = setup();
        let h = hierarchy(
            &mut idx,
            "app/Payments/Stripe.php",
            STRIPE,
            at(STRIPE, "charge(int"),
            HierarchyQuery::Method,
        );
        assert_eq!(h.title.as_deref(), Some("Stripe::charge()"));
        let g = h.graph.unwrap();
        assert_eq!(g.method.as_deref(), Some("charge"));
        let v = method_view(&MethodViewRequest {
            graph: g.clone(),
            sort_alphabetically: true,
            ..Default::default()
        });
        assert_eq!(
            tree(&v.roots),
            "Gateway Abstract\n  BaseGateway Inherited\n    Broken ShouldDefine\n    Logged Inherited\n    Stripe Defined\n      FakeStripe Inherited"
        );
        // Abstract methods: `name()` from inside the abstract class.
        let h = hierarchy(
            &mut idx,
            "app/Payments/BaseGateway.php",
            BASE,
            at(BASE, "name()"),
            HierarchyQuery::Method,
        );
        let v = method_view(&MethodViewRequest {
            graph: h.graph.unwrap(),
            hide_not_implemented: true,
            sort_alphabetically: true,
            ..Default::default()
        });
        assert_eq!(
            tree(&v.roots),
            "BaseGateway Abstract\n  Broken Defined\n  Logged Defined\n  Stripe Defined"
        );
    }

    #[test]
    fn callers_are_grouped_by_calling_method() {
        let (_d, mut idx) = setup();
        let root = hierarchy(
            &mut idx,
            "app/Contracts/Gateway.php",
            GATEWAY,
            at(GATEWAY, "charge("),
            HierarchyQuery::Call,
        );
        assert_eq!(root.title.as_deref(), Some("Gateway::charge()"));
        let r = root.root.unwrap();
        assert_eq!(
            (r.path.as_str(), r.line, r.column),
            ("app/Contracts/Gateway.php", 5, 20)
        );

        let calls = callers(
            &mut idx,
            "app/Contracts/Gateway.php",
            GATEWAY,
            at(GATEWAY, "charge("),
        );
        assert_eq!(calls.len(), 1, "{calls:?}");
        let pay = &calls[0];
        assert_eq!(pay.item.name, "pay()");
        assert_eq!(
            pay.item.container.as_deref(),
            Some("App\\Payments\\PaymentService")
        );
        // Two calls, not the comment.
        assert_eq!(pay.calls.len(), 2);
        assert_eq!(pay.calls[0].range.start.line, 12);
        assert_eq!(
            pay.calls[0].preview,
            "if (!$this->gateway->charge($cents)) {"
        );

        // Callers of `pay()`, from its declaration: the test.
        let text =
            std::fs::read_to_string(idx.root().join("app/Payments/PaymentService.php")).unwrap();
        let calls = callers(
            &mut idx,
            "app/Payments/PaymentService.php",
            &text,
            at(&text, "pay(int"),
        );
        let names: Vec<_> = calls
            .iter()
            .map(|c| (c.item.name.as_str(), c.item.path.as_str()))
            .collect();
        assert_eq!(names, [("test_pay()", "tests/PayTest.php")]);
        let production = call_view(&CallViewRequest {
            base: r,
            calls,
            scope: HierarchyScope::Production,
            sort_alphabetically: false,
        });
        assert!(production.is_empty());
    }

    #[test]
    fn callees_list_methods_functions_and_constructors() {
        let (_d, mut idx) = setup();
        let calls = callees(
            &mut idx,
            "app/Payments/Stripe.php",
            STRIPE,
            at(STRIPE, "charge(int"),
        );
        let names: Vec<_> = calls
            .iter()
            .map(|c| (c.item.name.as_str(), c.item.path.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("send()", "app/Payments/Stripe.php"),
                ("is_int()", ""),
                ("round()", "")
            ]
        );
        assert_eq!(calls[0].calls[0].range.start.line, 7);

        let calls = callees(&mut idx, "tests/PayTest.php", TEST, at(TEST, "test_pay"));
        let names: Vec<_> = calls.iter().map(|c| c.item.id.as_str()).collect();
        assert_eq!(
            names,
            [
                "App\\Payments\\PaymentService::__construct()",
                "App\\Payments\\PaymentService::pay()"
            ]
        );
    }
}
