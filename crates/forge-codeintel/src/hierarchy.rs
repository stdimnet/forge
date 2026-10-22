//! Type, Method and Call Hierarchy (Navigate | Type Hierarchy `Ctrl+H`, Method Hierarchy
//! `Ctrl+Shift+H`, Call Hierarchy `Ctrl+Alt+H`).
//!
//! Sources (the language servers' `typeHierarchy` / `callHierarchy`, a pack's own index)
//! fill the data model: a [`TypeGraph`] of types with their direct supertypes, and
//! [`CallNode`]s for the callers or callees of one function. The views of the Hierarchy tool
//! window are computed here from that data, so toggling a view, a scope or the sorting needs
//! no new query:
//!
//! - [`type_view`]: Class Hierarchy, Supertypes Hierarchy and Subtypes Hierarchy.
//! - [`method_view`]: the classes of a method's hierarchy with PHPStorm's marks (defined,
//!   should be defined, not defined but inherited).
//! - [`call_view`]: one level of callers or callees, filtered by scope and sorted.
//! - [`merge_type_graphs`] / [`merge_call_nodes`]: answers of several sources in one.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::edit::Range;

/// At most this many nodes per tree, against pathological or cyclic hierarchies.
const MAX_NODES: usize = 2000;

/// What a hierarchy item is; decides its icon.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HierarchyKind {
    #[default]
    Class,
    Interface,
    Trait,
    Enum,
    Method,
    Constructor,
    Function,
    Property,
    /// Top-level code of a file (a call outside any function).
    File,
    Other,
}

impl HierarchyKind {
    /// From an LSP `SymbolKind` number.
    pub fn from_lsp(kind: u32) -> Self {
        match kind {
            1 => Self::File,
            5 | 23 => Self::Class,
            11 => Self::Interface,
            10 => Self::Enum,
            6 => Self::Method,
            9 => Self::Constructor,
            12 => Self::Function,
            7 | 8 => Self::Property,
            _ => Self::Other,
        }
    }

    pub fn is_type(self) -> bool {
        matches!(
            self,
            Self::Class | Self::Interface | Self::Trait | Self::Enum
        )
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// A type or a function in a hierarchy. Positions are LSP positions of the name.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyItem {
    /// Unique in its graph (an index uses the FQN, servers `path:line:column`).
    pub id: String,
    /// `PaymentGateway`, `charge()`, `PaymentService::pay()`.
    pub name: String,
    pub kind: HierarchyKind,
    /// Namespace of a type, declaring class of a method (shown greyed out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Workspace-relative path; empty for a type the sources know only by name.
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub line: u32,
    #[serde(default)]
    pub column: u32,
    /// An abstract class or method.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_abstract: bool,
    /// Source data to expand the item later (a language server's item and server id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handle: Option<Value>,
}

impl HierarchyItem {
    /// The id servers' items get: `path:line:column`.
    pub fn location_id(path: &str, line: u32, column: u32) -> String {
        format!("{path}:{line}:{column}")
    }

    fn same_place(&self, other: &HierarchyItem) -> bool {
        !self.path.is_empty() && self.path == other.path && self.line == other.line
    }
}

/// Whether a type declares the method of a Method Hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MethodPresence {
    /// Declared with a body (or in a trait the type uses).
    Defined,
    /// Declared abstract (an abstract method, an interface method).
    Abstract,
}

/// A type with its direct supertypes (ids of other entries).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeEntry {
    pub item: HierarchyItem,
    /// Parent class (classes) or parent interfaces (interfaces).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extends: Vec<String>,
    /// Implemented interfaces and used traits.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implements: Vec<String>,
    /// Method Hierarchy only: whether the type declares the method.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<MethodPresence>,
}

impl TypeEntry {
    fn supers(&self) -> impl Iterator<Item = &String> {
        self.extends.iter().chain(&self.implements)
    }
}

/// The types around one type: its supertypes and subtypes, transitively.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeGraph {
    /// Id of the type the hierarchy was opened for.
    pub root: String,
    pub types: Vec<TypeEntry>,
    /// Method Hierarchy only: the method's name (`charge`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
}

/// The three views of the Type Hierarchy.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TypeView {
    /// The superclass chain down to the type, then its subtypes.
    #[default]
    Class,
    Supertypes,
    Subtypes,
}

/// PHPStorm's hierarchy scopes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HierarchyScope {
    #[default]
    All,
    /// Project files outside test folders.
    Production,
    /// Project files in test folders (`tests/`, `__tests__/`, `*Test.php`, `*.spec.ts`).
    Test,
    /// Only the type itself (Type / Method Hierarchy) or calls inside its class (Call
    /// Hierarchy).
    ThisClass,
}

/// PHPStorm's marks in the Method Hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MethodMark {
    /// Method is defined in the class.
    Defined,
    /// Method is declared abstract in the class (or an interface).
    Abstract,
    /// Method should be defined since the class is not abstract.
    ShouldDefine,
    /// Method is not defined in the class, but it inherits a definition.
    Inherited,
}

/// A node of a computed tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyNode {
    pub item: HierarchyItem,
    pub children: Vec<HierarchyNode>,
    /// The type the hierarchy was opened for (shown in bold).
    #[serde(default, skip_serializing_if = "is_false")]
    pub current: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mark: Option<MethodMark>,
}

/// The tree of a Type or Method Hierarchy tab.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyView {
    pub roots: Vec<HierarchyNode>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TypeViewRequest {
    pub graph: TypeGraph,
    pub view: TypeView,
    pub scope: HierarchyScope,
    pub sort_alphabetically: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MethodViewRequest {
    pub graph: TypeGraph,
    /// "Hide Classes Where Method Is Not Implemented".
    pub hide_not_implemented: bool,
    pub scope: HierarchyScope,
    pub sort_alphabetically: bool,
}

/// Whether a project path is test code.
pub fn is_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let (dir, name) = lower.rsplit_once('/').unwrap_or(("", lower.as_str()));
    dir.split('/')
        .any(|s| matches!(s, "tests" | "test" | "__tests__" | "spec" | "specs"))
        || name.ends_with("test.php")
        || name.ends_with("_test.go")
        || name.ends_with("_test.py")
        || name.starts_with("test_")
        || [".test.", ".spec."].iter().any(|m| name.contains(m))
}

/// Library code (outside the project, or only known by name) is in no scope but "All".
fn is_project_path(path: &str) -> bool {
    !path.is_empty() && !path.contains("://") && !path.starts_with("vendor/")
}

fn in_scope(scope: HierarchyScope, item: &HierarchyItem) -> bool {
    match scope {
        HierarchyScope::All | HierarchyScope::ThisClass => true,
        HierarchyScope::Production => is_project_path(&item.path) && !is_test_path(&item.path),
        HierarchyScope::Test => is_project_path(&item.path) && is_test_path(&item.path),
    }
}

/// Lookups over a graph.
struct Graph<'a> {
    entries: HashMap<&'a str, &'a TypeEntry>,
    subs: HashMap<&'a str, Vec<&'a str>>,
}

impl<'a> Graph<'a> {
    fn new(graph: &'a TypeGraph) -> Self {
        let entries: HashMap<&str, &TypeEntry> = graph
            .types
            .iter()
            .map(|t| (t.item.id.as_str(), t))
            .collect();
        let mut subs: HashMap<&str, Vec<&str>> = HashMap::new();
        for t in &graph.types {
            let mut seen = HashSet::new();
            for s in t.supers() {
                if entries.contains_key(s.as_str()) && seen.insert(s.as_str()) {
                    subs.entry(s.as_str()).or_default().push(&t.item.id);
                }
            }
        }
        Graph { entries, subs }
    }

    fn get(&self, id: &str) -> Option<&'a TypeEntry> {
        self.entries.get(id).copied()
    }

    fn supers(&self, id: &str) -> Vec<&'a str> {
        let Some(e) = self.get(id) else {
            return vec![];
        };
        let mut seen = HashSet::new();
        e.supers()
            .map(String::as_str)
            .filter(|s| self.entries.contains_key(s) && seen.insert(*s))
            .collect()
    }

    fn subs(&self, id: &str) -> Vec<&'a str> {
        self.subs.get(id).cloned().unwrap_or_default()
    }

    /// The node for `id` with `children(id)` below it, recursively (no id twice on a path).
    fn tree(
        &self,
        id: &str,
        children: &dyn Fn(&str) -> Vec<&'a str>,
        path: &mut Vec<String>,
        budget: &mut usize,
    ) -> Option<HierarchyNode> {
        let entry = self.get(id)?;
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        path.push(id.to_string());
        let mut kids = vec![];
        for c in children(id) {
            if path.iter().any(|p| p == c) {
                continue;
            }
            kids.extend(self.tree(c, children, path, budget));
        }
        path.pop();
        Some(HierarchyNode {
            item: entry.item.clone(),
            children: kids,
            current: false,
            mark: None,
        })
    }

    fn subtypes_tree(&self, id: &str, budget: &mut usize) -> Option<HierarchyNode> {
        self.tree(id, &|i| self.subs(i), &mut Vec::new(), budget)
    }

    /// The superclass chain from the top down to `id` (classes only, through `extends`).
    fn class_chain(&self, id: &str) -> Vec<&'a str> {
        let mut chain = vec![];
        let mut seen = HashSet::new();
        let mut cur = self.get(id).and_then(|e| e.extends.first());
        while let Some(parent) = cur {
            let Some(e) = self.get(parent) else { break };
            if !seen.insert(parent.as_str()) || e.item.kind != HierarchyKind::Class {
                break;
            }
            chain.push(e.item.id.as_str());
            cur = e.extends.first();
        }
        chain.reverse();
        chain
    }

    /// Supertypes of `id` breadth-first (nearest first, `extends` before `implements`).
    fn ancestors(&self, id: &str) -> Vec<&'a str> {
        let mut out = vec![];
        let mut seen: HashSet<&str> = HashSet::new();
        let mut queue: VecDeque<&str> = self.supers(id).into();
        while let Some(s) = queue.pop_front() {
            if s == id || !seen.insert(s) {
                continue;
            }
            out.push(s);
            queue.extend(self.supers(s));
        }
        out
    }
}

fn mark_current(node: &mut HierarchyNode, id: &str) {
    if node.item.id == id {
        node.current = true;
    }
    for c in &mut node.children {
        mark_current(c, id);
    }
}

/// Keeps nodes that match, the current node and every node on the way to one of them.
fn prune(mut node: HierarchyNode, keep: &dyn Fn(&HierarchyNode) -> bool) -> Option<HierarchyNode> {
    node.children = std::mem::take(&mut node.children)
        .into_iter()
        .filter_map(|c| prune(c, keep))
        .collect();
    (node.current || !node.children.is_empty() || keep(&node)).then_some(node)
}

fn order(a: &HierarchyItem, b: &HierarchyItem, alphabetically: bool) -> std::cmp::Ordering {
    if alphabetically {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.container.cmp(&b.container))
    } else {
        (a.path.is_empty(), &a.path, a.line, &a.name).cmp(&(
            b.path.is_empty(),
            &b.path,
            b.line,
            &b.name,
        ))
    }
}

/// Sorts children alphabetically; otherwise they keep the sources' order (superclass before
/// interfaces, subtypes as the source listed them).
fn sort_tree(node: &mut HierarchyNode) {
    node.children.sort_by(|a, b| order(&a.item, &b.item, true));
    for c in &mut node.children {
        sort_tree(c);
    }
}

fn scoped(
    root: Option<HierarchyNode>,
    scope: HierarchyScope,
    current: &str,
    sort_alphabetically: bool,
) -> HierarchyView {
    let Some(mut root) = root else {
        return HierarchyView::default();
    };
    mark_current(&mut root, current);
    let root = match scope {
        HierarchyScope::All => Some(root),
        HierarchyScope::ThisClass => prune(root, &|_| false),
        _ => prune(root, &|n| in_scope(scope, &n.item)),
    };
    let roots = root
        .map(|mut r| {
            if sort_alphabetically {
                sort_tree(&mut r);
            }
            vec![r]
        })
        .unwrap_or_default();
    HierarchyView { roots }
}

/// The tree of a Type Hierarchy view.
pub fn type_view(req: &TypeViewRequest) -> HierarchyView {
    let g = Graph::new(&req.graph);
    let id = req.graph.root.as_str();
    let mut budget = MAX_NODES;
    let root = match req.view {
        TypeView::Subtypes => g.subtypes_tree(id, &mut budget),
        TypeView::Supertypes => g.tree(id, &|i| g.supers(i), &mut Vec::new(), &mut budget),
        TypeView::Class => {
            let chain = g.class_chain(id);
            let mut node = g.subtypes_tree(id, &mut budget);
            for parent in chain.iter().rev() {
                node = g.get(parent).map(|e| HierarchyNode {
                    item: e.item.clone(),
                    children: node.into_iter().collect(),
                    current: false,
                    mark: None,
                });
            }
            node
        }
    };
    scoped(root, req.scope, id, req.sort_alphabetically)
}

/// The type a Method Hierarchy starts at: the topmost supertype of the graph's root that
/// declares the method (the root itself when none does).
pub fn method_base(graph: &TypeGraph) -> String {
    let g = Graph::new(graph);
    let declares = |id: &str| g.get(id).is_some_and(|e| e.method.is_some());
    g.ancestors(&graph.root)
        .into_iter()
        .filter(|a| declares(a))
        .find(|a| !g.ancestors(a).into_iter().any(|x| declares(x)))
        .unwrap_or(&graph.root)
        .to_string()
}

/// The tree of a Method Hierarchy: the classes below the topmost declaration of the method,
/// each with its mark.
pub fn method_view(req: &MethodViewRequest) -> HierarchyView {
    let g = Graph::new(&req.graph);
    let base = method_base(&req.graph);
    let mut budget = MAX_NODES;
    let mut root = g.subtypes_tree(&base, &mut budget);

    // Whether a type has a definition of the method (its own or inherited).
    let mut implemented: HashMap<String, bool> = HashMap::new();
    fn has_definition(
        g: &Graph,
        id: &str,
        memo: &mut HashMap<String, bool>,
        visiting: &mut HashSet<String>,
    ) -> bool {
        if let Some(v) = memo.get(id) {
            return *v;
        }
        if !visiting.insert(id.to_string()) {
            return false;
        }
        let own = g
            .get(id)
            .is_some_and(|e| e.method == Some(MethodPresence::Defined));
        let v = own
            || g.supers(id)
                .into_iter()
                .any(|s| has_definition(g, s, memo, visiting));
        visiting.remove(id);
        memo.insert(id.to_string(), v);
        v
    }
    fn assign(g: &Graph, node: &mut HierarchyNode, memo: &mut HashMap<String, bool>) {
        let e = g.get(&node.item.id);
        node.mark = Some(match e.and_then(|e| e.method) {
            Some(MethodPresence::Defined) => MethodMark::Defined,
            Some(MethodPresence::Abstract) => MethodMark::Abstract,
            None if has_definition(g, &node.item.id, memo, &mut HashSet::new()) => {
                MethodMark::Inherited
            }
            None if matches!(node.item.kind, HierarchyKind::Class | HierarchyKind::Enum)
                && !node.item.is_abstract =>
            {
                MethodMark::ShouldDefine
            }
            None => MethodMark::Inherited,
        });
        for c in &mut node.children {
            assign(g, c, memo);
        }
    }
    if let Some(r) = &mut root {
        assign(&g, r, &mut implemented);
    }
    if req.hide_not_implemented {
        if let Some(mut r) = root.take() {
            mark_current(&mut r, &req.graph.root);
            root = prune(r, &|n| {
                matches!(n.mark, Some(MethodMark::Defined | MethodMark::Abstract))
            });
        }
    }
    scoped(root, req.scope, &req.graph.root, req.sort_alphabetically)
}

/// Merges two graphs of the same type: entries at the same place are one entry (the first
/// graph's id wins), supertypes and method presence are combined.
pub fn merge_type_graphs(mut a: TypeGraph, b: TypeGraph) -> TypeGraph {
    let mut rename: HashMap<String, String> = HashMap::new();
    for t in &b.types {
        let same = a
            .types
            .iter()
            .find(|x| x.item.id == t.item.id || x.item.same_place(&t.item));
        if let Some(x) = same {
            rename.insert(t.item.id.clone(), x.item.id.clone());
        }
    }
    let map = |id: &String| rename.get(id).cloned().unwrap_or_else(|| id.clone());
    for t in b.types {
        let extends: Vec<String> = t.extends.iter().map(map).collect();
        let implements: Vec<String> = t.implements.iter().map(map).collect();
        let id = map(&t.item.id);
        match a.types.iter_mut().find(|x| x.item.id == id) {
            Some(x) => {
                for e in extends {
                    if !x.extends.contains(&e) && !x.implements.contains(&e) {
                        x.extends.push(e);
                    }
                }
                for i in implements {
                    if !x.extends.contains(&i) && !x.implements.contains(&i) {
                        x.implements.push(i);
                    }
                }
                x.method = x.method.or(t.method);
            }
            None => a.types.push(TypeEntry {
                item: HierarchyItem { id, ..t.item },
                extends,
                implements,
                method: t.method,
            }),
        }
    }
    if a.types.is_empty() || !a.types.iter().any(|t| t.item.id == a.root) {
        a.root = map(&b.root);
    }
    a.method = a.method.or(b.method);
    a
}

/// Callers or callees.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CallDirection {
    #[default]
    Callers,
    Callees,
}

/// Where a call is made: the line in the caller.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallSite {
    pub path: String,
    pub range: Range,
    /// The trimmed source line, for the usages list.
    #[serde(default)]
    pub preview: String,
}

/// A caller (with the places it calls from) or a callee (with the places it is called at).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallNode {
    pub item: HierarchyItem,
    #[serde(default)]
    pub calls: Vec<CallSite>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CallViewRequest {
    /// The function the hierarchy was opened for ("This Class" compares with its class).
    pub base: HierarchyItem,
    pub calls: Vec<CallNode>,
    pub scope: HierarchyScope,
    pub sort_alphabetically: bool,
}

fn same_class(a: &HierarchyItem, b: &HierarchyItem) -> bool {
    match (&a.container, &b.container) {
        (Some(x), Some(y)) => x.eq_ignore_ascii_case(y) && (a.path.is_empty() || a.path == b.path),
        _ => a.path == b.path && !a.path.is_empty(),
    }
}

/// One level of a Call Hierarchy after the scope and the sorting.
pub fn call_view(req: &CallViewRequest) -> Vec<CallNode> {
    let mut out: Vec<CallNode> = req
        .calls
        .iter()
        .filter(|n| match req.scope {
            HierarchyScope::ThisClass => same_class(&n.item, &req.base),
            scope => in_scope(scope, &n.item),
        })
        .cloned()
        .collect();
    out.sort_by(|a, b| order(&a.item, &b.item, req.sort_alphabetically));
    for n in &mut out {
        n.calls
            .sort_by_key(|c| (c.path.clone(), c.range.start.line, c.range.start.character));
    }
    out
}

/// Merges two answers for the same function: callers at the same place are one node with the
/// call sites of both.
pub fn merge_call_nodes(mut a: Vec<CallNode>, b: Vec<CallNode>) -> Vec<CallNode> {
    for n in b {
        match a
            .iter_mut()
            .find(|x| x.item.id == n.item.id || x.item.same_place(&n.item))
        {
            Some(x) => {
                for c in n.calls {
                    if !x
                        .calls
                        .iter()
                        .any(|y| y.path == c.path && y.range == c.range)
                    {
                        x.calls.push(c);
                    }
                }
            }
            None => a.push(n),
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::Position;

    fn item(id: &str, kind: HierarchyKind, path: &str, line: u32) -> HierarchyItem {
        HierarchyItem {
            id: id.into(),
            name: id.rsplit('\\').next().unwrap().into(),
            kind,
            path: path.into(),
            line,
            ..Default::default()
        }
    }

    fn entry(id: &str, kind: HierarchyKind, path: &str, ext: &[&str], imp: &[&str]) -> TypeEntry {
        TypeEntry {
            item: item(id, kind, path, 0),
            extends: ext.iter().map(|s| s.to_string()).collect(),
            implements: imp.iter().map(|s| s.to_string()).collect(),
            method: None,
        }
    }

    /// Model <- Order <- SpecialOrder, Model implements Arrayable, Order uses HasTotal,
    /// tests/OrderFake extends Order.
    fn graph(root: &str) -> TypeGraph {
        use HierarchyKind::*;
        TypeGraph {
            root: root.into(),
            types: vec![
                entry("Arrayable", Interface, "vendor/Arrayable.php", &[], &[]),
                entry("Model", Class, "vendor/Model.php", &[], &["Arrayable"]),
                entry("HasTotal", Trait, "app/HasTotal.php", &[], &[]),
                entry("Order", Class, "app/Order.php", &["Model"], &["HasTotal"]),
                entry(
                    "SpecialOrder",
                    Class,
                    "app/SpecialOrder.php",
                    &["Order"],
                    &[],
                ),
                entry("OrderFake", Class, "tests/OrderFake.php", &["Order"], &[]),
            ],
            method: None,
        }
    }

    fn names(view: &HierarchyView) -> String {
        fn walk(n: &HierarchyNode, depth: usize, out: &mut Vec<String>) {
            let mark = match n.mark {
                Some(MethodMark::Defined) => " +",
                Some(MethodMark::Abstract) => " a",
                Some(MethodMark::ShouldDefine) => " !",
                Some(MethodMark::Inherited) => " -",
                None => "",
            };
            let cur = if n.current { "*" } else { "" };
            out.push(format!("{}{}{cur}{mark}", "  ".repeat(depth), n.item.name));
            for c in &n.children {
                walk(c, depth + 1, out);
            }
        }
        let mut out = vec![];
        for r in &view.roots {
            walk(r, 0, &mut out);
        }
        out.join("\n")
    }

    fn view(graph: TypeGraph, view: TypeView, scope: HierarchyScope, sort: bool) -> String {
        names(&type_view(&TypeViewRequest {
            graph,
            view,
            scope,
            sort_alphabetically: sort,
        }))
    }

    #[test]
    fn class_hierarchy_shows_the_superclass_chain_and_the_subtypes() {
        assert_eq!(
            view(graph("Order"), TypeView::Class, HierarchyScope::All, false),
            "Model\n  Order*\n    SpecialOrder\n    OrderFake"
        );
        assert_eq!(
            view(graph("Order"), TypeView::Class, HierarchyScope::All, true),
            "Model\n  Order*\n    OrderFake\n    SpecialOrder"
        );
    }

    #[test]
    fn supertypes_and_subtypes() {
        assert_eq!(
            view(
                graph("Order"),
                TypeView::Supertypes,
                HierarchyScope::All,
                false
            ),
            "Order*\n  Model\n    Arrayable\n  HasTotal"
        );
        assert_eq!(
            view(
                graph("Arrayable"),
                TypeView::Subtypes,
                HierarchyScope::All,
                false
            ),
            "Arrayable*\n  Model\n    Order\n      SpecialOrder\n      OrderFake"
        );
        // An interface has no superclass chain: Class Hierarchy shows its subtypes.
        assert_eq!(
            view(
                graph("Arrayable"),
                TypeView::Class,
                HierarchyScope::All,
                false
            ),
            view(
                graph("Arrayable"),
                TypeView::Subtypes,
                HierarchyScope::All,
                false
            )
        );
    }

    #[test]
    fn scopes_keep_the_way_to_matching_types() {
        assert_eq!(
            view(
                graph("Order"),
                TypeView::Subtypes,
                HierarchyScope::Test,
                false
            ),
            "Order*\n  OrderFake"
        );
        assert_eq!(
            view(
                graph("Order"),
                TypeView::Subtypes,
                HierarchyScope::Production,
                false
            ),
            "Order*\n  SpecialOrder"
        );
        // Vendor classes are not production code, but stay as the way to the current type.
        assert_eq!(
            view(
                graph("Order"),
                TypeView::Class,
                HierarchyScope::Production,
                false
            ),
            "Model\n  Order*\n    SpecialOrder"
        );
        assert_eq!(
            view(
                graph("Order"),
                TypeView::Class,
                HierarchyScope::ThisClass,
                false
            ),
            "Model\n  Order*"
        );
    }

    #[test]
    fn cycles_do_not_loop() {
        let mut g = graph("Order");
        g.types[1].extends.push("SpecialOrder".into()); // Model extends SpecialOrder
        let out = view(g, TypeView::Supertypes, HierarchyScope::All, false);
        assert_eq!(
            out,
            "Order*\n  Model\n    SpecialOrder\n    Arrayable\n  HasTotal"
        );
    }

    fn method_graph() -> TypeGraph {
        use HierarchyKind::*;
        let mut abstract_base = entry(
            "BaseGateway",
            Class,
            "app/BaseGateway.php",
            &[],
            &["Gateway"],
        );
        abstract_base.item.is_abstract = true;
        let mut g = TypeGraph {
            root: "Stripe".into(),
            types: vec![
                entry("Gateway", Interface, "app/Gateway.php", &[], &[]),
                abstract_base,
                entry("Stripe", Class, "app/Stripe.php", &["BaseGateway"], &[]),
                entry("StripeEu", Class, "app/StripeEu.php", &["Stripe"], &[]),
                entry("Broken", Class, "app/Broken.php", &["BaseGateway"], &[]),
                entry("Charges", Trait, "app/Charges.php", &[], &[]),
                entry(
                    "Mixed",
                    Class,
                    "tests/Mixed.php",
                    &[],
                    &["Gateway", "Charges"],
                ),
            ],
            method: Some("charge".into()),
        };
        g.types[0].method = Some(MethodPresence::Abstract);
        g.types[2].method = Some(MethodPresence::Defined);
        g.types[5].method = Some(MethodPresence::Defined);
        g
    }

    #[test]
    fn method_hierarchy_marks_classes_like_phpstorm() {
        let g = method_graph();
        assert_eq!(method_base(&g), "Gateway");
        let out = names(&method_view(&MethodViewRequest {
            graph: g.clone(),
            ..Default::default()
        }));
        assert_eq!(
            out,
            "Gateway a\n  BaseGateway -\n    Stripe* +\n      StripeEu -\n    Broken !\n  Mixed -"
        );
        let hidden = names(&method_view(&MethodViewRequest {
            graph: g,
            hide_not_implemented: true,
            sort_alphabetically: true,
            ..Default::default()
        }));
        assert_eq!(hidden, "Gateway a\n  BaseGateway -\n    Stripe* +");
    }

    #[test]
    fn merging_graphs_joins_entries_at_the_same_place() {
        let a = graph("Order");
        let b = TypeGraph {
            root: "app/Order.php:0:6".into(),
            types: vec![
                TypeEntry {
                    item: item(
                        "app/Order.php:0:6",
                        HierarchyKind::Class,
                        "app/Order.php",
                        0,
                    ),
                    implements: vec!["app/Countable.php:0:0".into()],
                    ..Default::default()
                },
                TypeEntry {
                    item: item(
                        "app/Countable.php:0:0",
                        HierarchyKind::Interface,
                        "app/Countable.php",
                        0,
                    ),
                    ..Default::default()
                },
            ],
            method: None,
        };
        let m = merge_type_graphs(a, b);
        assert_eq!(m.root, "Order");
        assert_eq!(m.types.len(), 7);
        let order = m.types.iter().find(|t| t.item.id == "Order").unwrap();
        assert_eq!(order.implements, vec!["HasTotal", "app/Countable.php:0:0"]);
        // A graph from nothing takes the other's root.
        let only = merge_type_graphs(TypeGraph::default(), graph("Model"));
        assert_eq!(only.root, "Model");
    }

    fn call(name: &str, container: Option<&str>, path: &str, line: u32) -> CallNode {
        CallNode {
            item: HierarchyItem {
                id: format!("{path}:{line}"),
                name: name.into(),
                kind: HierarchyKind::Method,
                container: container.map(String::from),
                path: path.into(),
                line,
                ..Default::default()
            },
            calls: vec![CallSite {
                path: path.into(),
                range: Range::new(Position::new(line + 2, 4), Position::new(line + 2, 10)),
                preview: "$this->charge($order);".into(),
            }],
        }
    }

    #[test]
    fn call_views_filter_by_scope_and_sort() {
        let base = call(
            "charge()",
            Some("App\\Payments\\PaymentService"),
            "app/PaymentService.php",
            10,
        )
        .item;
        let calls = vec![
            call(
                "test_pay()",
                Some("Tests\\PaymentTest"),
                "tests/PaymentTest.php",
                3,
            ),
            call(
                "pay()",
                Some("App\\Payments\\PaymentService"),
                "app/PaymentService.php",
                20,
            ),
            call(
                "checkout()",
                Some("App\\Http\\CheckoutController"),
                "app/Http/CheckoutController.php",
                5,
            ),
            call("array_map()", None, "", 0),
        ];
        let names = |scope, sort| {
            call_view(&CallViewRequest {
                base: base.clone(),
                calls: calls.clone(),
                scope,
                sort_alphabetically: sort,
            })
            .into_iter()
            .map(|n| n.item.name)
            .collect::<Vec<_>>()
        };
        assert_eq!(
            names(HierarchyScope::All, false),
            ["checkout()", "pay()", "test_pay()", "array_map()"]
        );
        assert_eq!(
            names(HierarchyScope::All, true),
            ["array_map()", "checkout()", "pay()", "test_pay()"]
        );
        assert_eq!(
            names(HierarchyScope::Production, false),
            ["checkout()", "pay()"]
        );
        assert_eq!(names(HierarchyScope::Test, false), ["test_pay()"]);
        assert_eq!(names(HierarchyScope::ThisClass, false), ["pay()"]);
    }

    #[test]
    fn merging_calls_joins_call_sites() {
        let a = vec![call("pay()", None, "app/A.php", 1)];
        let mut b = vec![
            call("pay()", None, "app/A.php", 1),
            call("x()", None, "app/B.php", 1),
        ];
        b[0].calls[0].range.start.line = 9;
        let m = merge_call_nodes(a, b);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].calls.len(), 2);
    }

    #[test]
    fn test_paths() {
        for p in [
            "tests/Unit/UserTest.php",
            "src/__tests__/a.ts",
            "app/OrderTest.php",
            "src/a.spec.ts",
            "x/test_a.py",
        ] {
            assert!(is_test_path(p), "{p}");
        }
        for p in ["app/Models/User.php", "src/latest.ts", "app/Testing.php"] {
            assert!(!is_test_path(p), "{p}");
        }
    }
}
