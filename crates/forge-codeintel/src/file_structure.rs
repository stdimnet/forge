//! The File Structure popup (Ctrl+F12): the Structure view's tree of the current file as
//! the rows the popup shows, after its toolbar toggles ("Show Inherited", "Show Fields",
//! "Show Lambdas", "Sort Alphabetically") and its speed search.
//!
//! Nodes are the Structure view's JSON objects of any language pack (`name`, `kind`, a
//! 0-based `line`, optional `children`, `inheritedFrom` and `lambda`), so PHP, Lua and web
//! files share one model. Typing filters with fuzzy matching (`forge-fuzzy`) and keeps the
//! parents of every match; without a query the node at the caret is selected.

use forge_fuzzy::{fuzzy_match_with, MatchOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The header lines of a scope around the caret: from its first line to its `{`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderLines {
    pub line: u32,
    #[serde(default)]
    pub open_line: Option<u32>,
}

fn yes() -> bool {
    true
}

/// What the popup shows.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStructureRequest {
    pub nodes: Vec<Value>,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub sort_alpha: bool,
    #[serde(default = "yes")]
    pub show_fields: bool,
    #[serde(default)]
    pub show_inherited: bool,
    #[serde(default = "yes")]
    pub show_lambdas: bool,
    /// 0-based line of the caret.
    #[serde(default)]
    pub caret_line: Option<u32>,
    /// The scopes around the caret (any order).
    #[serde(default)]
    pub enclosing: Vec<HeaderLines>,
}

/// One visible row of the tree.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStructureRow {
    /// Child indices from the root, `0/2/1`: stable while the toggles stay the same.
    pub id: String,
    pub depth: u32,
    /// The node without its children.
    pub node: Value,
    /// Highlighted characters of the name (`char` indices).
    pub positions: Vec<usize>,
    /// The name matches the query (false for parents kept for a match below them).
    pub matched: bool,
    pub has_children: bool,
}

/// The rows and which toolbar toggles apply to the file.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStructureView {
    pub rows: Vec<FileStructureRow>,
    /// Index into `rows`.
    pub selected: Option<usize>,
    pub has_fields: bool,
    pub has_lambdas: bool,
    pub has_inherited: bool,
}

fn kind(node: &Value) -> &str {
    node.get("kind").and_then(Value::as_str).unwrap_or("")
}

fn name(node: &Value) -> &str {
    node.get("name").and_then(Value::as_str).unwrap_or("")
}

fn line(node: &Value) -> Option<u32> {
    node.get("line").and_then(Value::as_u64).map(|l| l as u32)
}

fn children(node: &Value) -> &[Value] {
    node.get("children")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Properties, constants and variables: what "Show Fields" hides.
pub fn is_field(node: &Value) -> bool {
    matches!(kind(node), "property" | "constant" | "variable")
}

fn is_inherited(node: &Value) -> bool {
    node.get("inheritedFrom").is_some_and(|v| !v.is_null())
}

fn is_lambda(node: &Value) -> bool {
    node.get("lambda").and_then(Value::as_bool).unwrap_or(false)
}

/// The order of kinds when sorting alphabetically (PHPStorm keeps kinds grouped).
fn kind_rank(node: &Value) -> u8 {
    match kind(node) {
        "constant" | "enumCase" => 0,
        "property" => 1,
        "method" => 2,
        _ => 3,
    }
}

/// The children of one level in display order, with their original indices.
fn ordered<'a>(nodes: &'a [Value], req: &FileStructureRequest) -> Vec<(usize, &'a Value)> {
    let mut out: Vec<(usize, &Value)> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| req.show_fields || !is_field(n))
        .filter(|(_, n)| req.show_inherited || !is_inherited(n))
        .filter(|(_, n)| req.show_lambdas || !is_lambda(n))
        .collect();
    if req.sort_alpha {
        out.sort_by(|(_, a), (_, b)| {
            kind_rank(a)
                .cmp(&kind_rank(b))
                .then_with(|| name(a).to_lowercase().cmp(&name(b).to_lowercase()))
        });
    }
    out
}

struct Builder<'r> {
    req: &'r FileStructureRequest,
    opts: MatchOptions,
    rows: Vec<FileStructureRow>,
    scores: Vec<Option<i32>>,
}

impl Builder<'_> {
    /// Add `nodes` below `prefix`; returns whether any of them (or below) matched.
    fn level(&mut self, nodes: &[Value], prefix: &str, depth: u32) -> bool {
        let filtering = !self.req.query.trim().is_empty();
        let mut any = false;
        for (index, node) in ordered(nodes, self.req) {
            let id = if prefix.is_empty() {
                index.to_string()
            } else {
                format!("{prefix}/{index}")
            };
            let m = filtering
                .then(|| fuzzy_match_with(self.req.query.trim(), name(node), self.opts))
                .flatten();
            let at = self.rows.len();
            let mut shown = node.clone();
            let kids = children(node);
            if let Some(o) = shown.as_object_mut() {
                o.remove("children");
            }
            self.rows.push(FileStructureRow {
                id: id.clone(),
                depth,
                node: shown,
                positions: m.as_ref().map(|m| m.positions.clone()).unwrap_or_default(),
                matched: m.is_some(),
                has_children: !ordered(kids, self.req).is_empty(),
            });
            self.scores.push(m.as_ref().map(|m| m.score));
            let below = self.level(kids, &id, depth + 1);
            if filtering && m.is_none() && !below {
                // Neither this node nor anything below it matches.
                self.rows.truncate(at);
                self.scores.truncate(at);
                continue;
            }
            any = true;
        }
        any
    }
}

fn walk(nodes: &[Value], f: &mut impl FnMut(&Value)) {
    for n in nodes {
        f(n);
        walk(children(n), f);
    }
}

/// The node at the caret: one declared on the caret's line, else the innermost scope
/// around the caret that is shown, else the last top-level node above the caret.
fn caret_row(rows: &[FileStructureRow], req: &FileStructureRequest) -> Option<usize> {
    let caret = req.caret_line?;
    let deepest = |pred: &dyn Fn(u32) -> bool| {
        rows.iter()
            .enumerate()
            .filter(|(_, r)| line(&r.node).is_some_and(pred))
            .max_by_key(|(i, r)| (r.depth, *i))
            .map(|(i, _)| i)
    };
    deepest(&|l| l == caret)
        .or_else(|| {
            deepest(&|l| {
                req.enclosing
                    .iter()
                    .any(|h| l >= h.line && l <= h.open_line.unwrap_or(h.line).max(h.line))
            })
        })
        .or_else(|| {
            rows.iter()
                .enumerate()
                .filter(|(_, r)| r.depth == 0 && line(&r.node).is_some_and(|l| l <= caret))
                .map(|(i, _)| i)
                .next_back()
        })
}

/// The popup's rows for `req`.
pub fn file_structure_view(req: &FileStructureRequest) -> FileStructureView {
    let mut b = Builder {
        req,
        opts: MatchOptions::default(),
        rows: Vec::new(),
        scores: Vec::new(),
    };
    b.level(&req.nodes, "", 0);
    let Builder { rows, scores, .. } = b;
    let selected = if req.query.trim().is_empty() {
        caret_row(&rows, req).or((!rows.is_empty()).then_some(0))
    } else {
        // The best match; the first one on equal scores.
        scores
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.map(|s| (i, s)))
            .max_by(|(ia, a), (ib, b)| a.cmp(b).then(ib.cmp(ia)))
            .map(|(i, _)| i)
    };
    let (mut has_fields, mut has_lambdas, mut has_inherited) = (false, false, false);
    walk(&req.nodes, &mut |n| {
        has_fields |= is_field(n);
        has_lambdas |= is_lambda(n);
        has_inherited |= is_inherited(n);
    });
    FileStructureView {
        rows,
        selected,
        has_fields,
        has_lambdas,
        has_inherited,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn class() -> Vec<Value> {
        vec![json!({
            "name": "OrderController", "kind": "class", "line": 4,
            "children": [
                { "name": "orders", "kind": "property", "line": 6 },
                { "name": "index", "kind": "method", "line": 8, "detail": "()" },
                { "name": "show", "kind": "method", "line": 13 },
                { "name": "save", "kind": "method", "line": 1, "inheritedFrom": "Controller", "path": "app/Http/Controllers/Controller.php" },
                { "name": "callback", "kind": "function", "line": 15, "lambda": true },
            ]
        })]
    }

    fn req(query: &str) -> FileStructureRequest {
        FileStructureRequest {
            nodes: class(),
            query: query.into(),
            sort_alpha: false,
            show_fields: true,
            show_inherited: false,
            show_lambdas: true,
            caret_line: None,
            enclosing: Vec::new(),
        }
    }

    fn names(v: &FileStructureView) -> Vec<&str> {
        v.rows.iter().map(|r| name(&r.node)).collect()
    }

    #[test]
    fn toggles_hide_fields_inherited_members_and_lambdas() {
        let v = file_structure_view(&req(""));
        assert_eq!(
            names(&v),
            ["OrderController", "orders", "index", "show", "callback"]
        );
        assert!(v.rows[0].has_children && !v.rows[1].has_children);
        assert_eq!(v.rows[2].id, "0/1");
        assert!(v.rows[2].node.get("children").is_none());
        assert!(v.has_fields && v.has_lambdas && v.has_inherited);

        let mut r = req("");
        r.show_fields = false;
        r.show_inherited = true;
        r.show_lambdas = false;
        assert_eq!(
            names(&file_structure_view(&r)),
            ["OrderController", "index", "show", "save"]
        );
    }

    #[test]
    fn sorts_alphabetically_keeping_kinds_grouped() {
        let mut r = req("");
        r.sort_alpha = true;
        assert_eq!(
            names(&file_structure_view(&r)),
            ["OrderController", "orders", "index", "show", "callback"]
        );
        r.nodes = vec![
            json!({"name": "b", "kind": "method", "line": 0}),
            json!({"name": "A", "kind": "method", "line": 1}),
            json!({"name": "c", "kind": "constant", "line": 2}),
        ];
        assert_eq!(names(&file_structure_view(&r)), ["c", "A", "b"]);
    }

    #[test]
    fn speed_search_keeps_parents_and_selects_the_best_match() {
        let v = file_structure_view(&req("sh"));
        assert_eq!(names(&v), ["OrderController", "show"]);
        assert!(!v.rows[0].matched && v.rows[1].matched);
        assert_eq!(v.rows[1].positions, vec![0, 1]);
        assert_eq!(v.selected, Some(1));
        assert!(file_structure_view(&req("zzz")).rows.is_empty());
        assert_eq!(file_structure_view(&req("zzz")).selected, None);
    }

    #[test]
    fn selects_the_node_at_the_caret() {
        let mut r = req("");
        r.caret_line = Some(13);
        assert_eq!(file_structure_view(&r).selected, Some(3));
        // Inside the body of `index`.
        r.caret_line = Some(10);
        r.enclosing = vec![
            HeaderLines {
                line: 4,
                open_line: Some(5),
            },
            HeaderLines {
                line: 8,
                open_line: Some(8),
            },
        ];
        assert_eq!(file_structure_view(&r).selected, Some(2));
        // Between the members: the class.
        r.enclosing.truncate(1);
        assert_eq!(file_structure_view(&r).selected, Some(0));
        // Outside of every scope: the last node above.
        r.enclosing.clear();
        r.caret_line = Some(30);
        assert_eq!(file_structure_view(&r).selected, Some(0));
    }
}
