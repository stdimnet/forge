//! V8 heap snapshots (`.heapsnapshot`, Chrome DevTools / `node --heapsnapshot-signal` /
//! `v8.writeHeapSnapshot()`): the flat `nodes` and `edges` arrays described by
//! `snapshot.meta`, and the `strings` table.
//!
//! The file is read as a stream straight into compact number arrays (no JSON tree), the
//! dominator tree is computed over the strong edges (Cooper, Harvey and Kennedy's
//! iterative algorithm on the DFS postorder), and the Memory view gets DevTools' Summary:
//! objects grouped by constructor with count, shallow size and retained size (objects
//! nested in an object of the same constructor counted once), plus the biggest objects.

use std::collections::HashMap;
use std::io::Read;

use serde::de::{Deserializer, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};

use crate::model::{metric, InfoItem, SnapshotKind};
use crate::Snapshot;

/// Objects listed per constructor, and in the biggest objects.
const OBJECTS_PER_CLASS: usize = 20;
const BIGGEST: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeapObject {
    /// The snapshot's object id (`@id` in DevTools).
    pub id: u64,
    pub name: String,
    pub class_name: String,
    pub shallow: i64,
    pub retained: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeapClass {
    /// The constructor (`Map`, `Report`) or a group (`(string)`, `(closure)`, `(array)`).
    pub name: String,
    pub count: u64,
    pub shallow: i64,
    pub retained: i64,
    /// The largest objects of the class by retained size.
    pub objects: Vec<HeapObject>,
}

/// The Memory view of a heap snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeapSummary {
    pub node_count: usize,
    pub edge_count: usize,
    /// Size of every object reachable from the roots.
    pub total_size: i64,
    pub object_count: u64,
    /// Objects only reachable through weak references (left out of the summary).
    pub unreachable_size: i64,
    /// Constructors by retained size.
    pub classes: Vec<HeapClass>,
    pub biggest: Vec<HeapObject>,
}

/// A JSON number array read into `u32`s without building a JSON tree.
#[derive(Default)]
struct Numbers(Vec<u32>);

impl<'de> Deserialize<'de> for Numbers {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Numbers;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an array of numbers")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Numbers, A::Error> {
                let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(1 << 24));
                while let Some(v) = seq.next_element::<f64>()? {
                    out.push(v.clamp(0.0, u32::MAX as f64) as u32);
                }
                Ok(Numbers(out))
            }
        }
        d.deserialize_seq(V)
    }
}

#[derive(Deserialize)]
struct Meta {
    node_fields: Vec<String>,
    node_types: Vec<serde_json::Value>,
    edge_fields: Vec<String>,
    edge_types: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct Head {
    meta: Meta,
}

#[derive(Deserialize)]
struct Raw {
    snapshot: Head,
    nodes: Numbers,
    edges: Numbers,
    #[serde(default)]
    strings: Vec<String>,
}

fn types(v: Option<&serde_json::Value>) -> Vec<String> {
    v.and_then(|t| t.as_array())
        .map(|a| {
            a.iter()
                .map(|s| s.as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn field(fields: &[String], name: &str) -> Result<usize, String> {
    fields
        .iter()
        .position(|f| f == name)
        .ok_or_else(|| format!("the heap snapshot has no `{name}` field"))
}

/// Read a heap snapshot from a stream (a file of any size).
pub fn parse_reader<R: Read>(reader: R, name: &str) -> Result<Snapshot, String> {
    let mut de =
        serde_json::Deserializer::from_reader(std::io::BufReader::with_capacity(1 << 20, reader));
    let raw = Raw::deserialize(&mut de).map_err(|e| format!("not a V8 heap snapshot: {e}"))?;
    summarize(raw, name)
}

/// Read a heap snapshot from memory.
pub fn parse(data: &[u8], name: &str) -> Result<Snapshot, String> {
    let raw: Raw =
        serde_json::from_slice(data).map_err(|e| format!("not a V8 heap snapshot: {e}"))?;
    summarize(raw, name)
}

fn class_of(kind: &str, name: &str) -> Option<String> {
    Some(match kind {
        "synthetic" => return None,
        "object" | "native" => {
            if name.is_empty() {
                "(anonymous)".into()
            } else {
                name.to_string()
            }
        }
        "hidden" => "(system)".into(),
        "array" => "(array)".into(),
        "string" | "concatenated string" | "sliced string" => "(string)".into(),
        "code" => "(compiled code)".into(),
        "closure" => "(closure)".into(),
        "regexp" => "(regexp)".into(),
        "number" | "heap number" => "(number)".into(),
        "symbol" => "(symbol)".into(),
        "bigint" => "(bigint)".into(),
        "object shape" => "(object shape)".into(),
        other => format!("({other})"),
    })
}

fn display_name(kind: &str, name: &str, class: &str) -> String {
    match kind {
        "string" | "concatenated string" | "sliced string" => {
            let mut s: String = name.chars().take(60).collect();
            if s.len() < name.len() {
                s.push('…');
            }
            format!("\"{s}\"")
        }
        "closure" => format!("{}()", if name.is_empty() { "(anonymous)" } else { name }),
        "regexp" => name.to_string(),
        _ => class.to_string(),
    }
}

fn summarize(raw: Raw, name: &str) -> Result<Snapshot, String> {
    let meta = &raw.snapshot.meta;
    let nf = meta.node_fields.len();
    let ef = meta.edge_fields.len();
    if nf == 0 || ef == 0 {
        return Err("the heap snapshot has no node or edge fields".into());
    }
    let (n_type, n_name, n_id, n_size, n_edges) = (
        field(&meta.node_fields, "type")?,
        field(&meta.node_fields, "name")?,
        field(&meta.node_fields, "id")?,
        field(&meta.node_fields, "self_size")?,
        field(&meta.node_fields, "edge_count")?,
    );
    let (e_type, e_to) = (
        field(&meta.edge_fields, "type")?,
        field(&meta.edge_fields, "to_node")?,
    );
    let node_types = types(meta.node_types.get(n_type));
    let edge_types = types(meta.edge_types.get(e_type));
    let weak = edge_types
        .iter()
        .position(|t| t == "weak")
        .map(|i| i as u32);
    let nodes = &raw.nodes.0;
    let edges = &raw.edges.0;
    let strings = &raw.strings;
    let n = nodes.len() / nf;
    let m = edges.len() / ef;
    if n == 0 {
        return Err("the heap snapshot has no nodes".into());
    }
    let kind_of = |i: usize| -> &str {
        node_types
            .get(nodes[i * nf + n_type] as usize)
            .map(String::as_str)
            .unwrap_or("hidden")
    };
    let name_of = |i: usize| -> &str {
        strings
            .get(nodes[i * nf + n_name] as usize)
            .map(String::as_str)
            .unwrap_or("")
    };
    let size_of = |i: usize| nodes[i * nf + n_size] as i64;

    // First edge of every node (edges are stored node by node).
    let mut first = vec![0u32; n + 1];
    for i in 0..n {
        first[i + 1] = first[i] + nodes[i * nf + n_edges];
    }
    if first[n] as usize > m {
        return Err("the heap snapshot's edges do not match its nodes".into());
    }
    let strong = |e: usize| weak != Some(edges[e * ef + e_type]);
    let target = |e: usize| (edges[e * ef + e_to] as usize) / nf;

    // DFS postorder over strong edges from the root (node 0).
    const NONE: u32 = u32::MAX;
    let mut post = vec![NONE; n];
    let mut order: Vec<u32> = Vec::new();
    let mut visited = vec![false; n];
    let mut stack: Vec<(u32, u32)> = vec![(0, first[0])];
    visited[0] = true;
    while let Some(top) = stack.last_mut() {
        let (v, e) = *top;
        if e < first[v as usize + 1] {
            top.1 += 1;
            let e = e as usize;
            if !strong(e) {
                continue;
            }
            let t = target(e);
            if t < n && !visited[t] {
                visited[t] = true;
                stack.push((t as u32, first[t]));
            }
        } else {
            post[v as usize] = order.len() as u32;
            order.push(v);
            stack.pop();
        }
    }
    let r = order.len();
    // Predecessors (by postorder number) of every reachable node.
    let mut pred_count = vec![0u32; r + 1];
    for v in 0..n {
        if post[v] == NONE {
            continue;
        }
        for e in first[v] as usize..first[v + 1] as usize {
            let t = target(e);
            if strong(e) && t < n && post[t] != NONE {
                pred_count[post[t] as usize + 1] += 1;
            }
        }
    }
    for i in 0..r {
        pred_count[i + 1] += pred_count[i];
    }
    let mut preds = vec![0u32; pred_count[r] as usize];
    let mut fill = pred_count.clone();
    for v in 0..n {
        if post[v] == NONE {
            continue;
        }
        for e in first[v] as usize..first[v + 1] as usize {
            let t = target(e);
            if strong(e) && t < n && post[t] != NONE {
                let slot = &mut fill[post[t] as usize];
                preds[*slot as usize] = post[v];
                *slot += 1;
            }
        }
    }
    // Immediate dominators by postorder number; the root is the last.
    let root = (r - 1) as u32;
    let mut idom = vec![NONE; r];
    idom[root as usize] = root;
    let mut changed = true;
    while changed {
        changed = false;
        for b in (0..root).rev() {
            let mut new = NONE;
            for p in &preds[pred_count[b as usize] as usize..pred_count[b as usize + 1] as usize] {
                if idom[*p as usize] == NONE {
                    continue;
                }
                new = if new == NONE {
                    *p
                } else {
                    let (mut a, mut c) = (*p, new);
                    while a != c {
                        while a < c {
                            a = idom[a as usize];
                        }
                        while c < a {
                            c = idom[c as usize];
                        }
                    }
                    a
                };
            }
            if new != NONE && idom[b as usize] != new {
                idom[b as usize] = new;
                changed = true;
            }
        }
    }
    // Retained sizes: dominators come after the nodes they dominate in postorder.
    let mut retained: Vec<i64> = order.iter().map(|v| size_of(*v as usize)).collect();
    for b in 0..root as usize {
        let d = idom[b];
        if d != NONE && d as usize != b {
            retained[d as usize] += retained[b];
        }
    }

    // Constructors.
    let mut class_ids: HashMap<String, usize> = HashMap::new();
    let mut classes: Vec<HeapClass> = Vec::new();
    let mut class_of_post = vec![NONE; r];
    let mut total_size = 0i64;
    let mut object_count = 0u64;
    for (p, v) in order.iter().enumerate() {
        let v = *v as usize;
        let Some(c) = class_of(kind_of(v), name_of(v)) else {
            continue;
        };
        let id = *class_ids.entry(c.clone()).or_insert_with(|| {
            classes.push(HeapClass {
                name: c,
                count: 0,
                shallow: 0,
                retained: 0,
                objects: Vec::new(),
            });
            classes.len() - 1
        });
        class_of_post[p] = id as u32;
        classes[id].count += 1;
        classes[id].shallow += size_of(v);
        total_size += size_of(v);
        object_count += 1;
    }
    // Retained size per constructor: walk the dominator tree, counting an object only when
    // no dominator of it has the same constructor.
    let mut child_start = vec![0u32; r + 1];
    for b in 0..root as usize {
        if idom[b] != NONE {
            child_start[idom[b] as usize + 1] += 1;
        }
    }
    for i in 0..r {
        child_start[i + 1] += child_start[i];
    }
    let mut dom_children = vec![0u32; child_start[r] as usize];
    let mut fill = child_start.clone();
    for b in 0..root as usize {
        if idom[b] != NONE {
            let slot = &mut fill[idom[b] as usize];
            dom_children[*slot as usize] = b as u32;
            *slot += 1;
        }
    }
    let mut active = vec![0u32; classes.len()];
    let mut walk: Vec<(u32, bool)> = vec![(root, false)];
    while let Some((b, exit)) = walk.pop() {
        let c = class_of_post[b as usize];
        if exit {
            if c != NONE {
                active[c as usize] -= 1;
            }
            continue;
        }
        if c != NONE {
            if active[c as usize] == 0 {
                classes[c as usize].retained += retained[b as usize];
            }
            active[c as usize] += 1;
        }
        walk.push((b, true));
        for ch in
            &dom_children[child_start[b as usize] as usize..child_start[b as usize + 1] as usize]
        {
            walk.push((*ch, false));
        }
    }
    // The biggest objects, overall and per constructor.
    let mut by_size: Vec<u32> = (0..r as u32)
        .filter(|p| class_of_post[*p as usize] != NONE)
        .collect();
    by_size.sort_by(|a, b| {
        retained[*b as usize]
            .cmp(&retained[*a as usize])
            .then(a.cmp(b))
    });
    let object = |p: u32| -> HeapObject {
        let v = order[p as usize] as usize;
        let class = &classes[class_of_post[p as usize] as usize].name;
        HeapObject {
            id: nodes[v * nf + n_id] as u64,
            name: display_name(kind_of(v), name_of(v), class),
            class_name: class.clone(),
            shallow: size_of(v),
            retained: retained[p as usize],
        }
    };
    let biggest: Vec<HeapObject> = by_size
        .iter()
        .filter(|p| !matches!(kind_of(order[**p as usize] as usize), "hidden" | "code"))
        .take(BIGGEST)
        .map(|p| object(*p))
        .collect();
    let mut per_class: Vec<Vec<HeapObject>> = vec![Vec::new(); classes.len()];
    for p in &by_size {
        let c = class_of_post[*p as usize] as usize;
        if per_class[c].len() < OBJECTS_PER_CLASS {
            per_class[c].push(object(*p));
        }
    }
    for (c, objects) in classes.iter_mut().zip(per_class) {
        c.objects = objects;
    }
    classes.sort_by(|a, b| b.retained.cmp(&a.retained).then(a.name.cmp(&b.name)));
    let unreachable_size = (0..n)
        .filter(|v| post[*v] == NONE)
        .map(size_of)
        .sum::<i64>();
    let summary = HeapSummary {
        node_count: n,
        edge_count: m,
        total_size,
        object_count,
        unreachable_size,
        classes,
        biggest,
    };
    Ok(Snapshot {
        kind: SnapshotKind::V8Heap,
        title: name.to_string(),
        info: vec![
            InfoItem {
                label: "Objects".into(),
                value: object_count.to_string(),
            },
            InfoItem {
                label: "Nodes / edges".into(),
                value: format!("{n} / {m}"),
            },
        ],
        metrics: vec![metric("Retained size", "bytes")],
        metric: 0,
        calls_label: "Count".into(),
        total: total_size,
        functions: Vec::new(),
        edges: Vec::new(),
        view: None,
        heap: Some(summary),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: &[u8] = include_bytes!("../tests/fixtures/small.heapsnapshot");

    #[test]
    fn summarizes_a_heap_snapshot() {
        let s = parse(SMALL, "small.heapsnapshot").unwrap();
        assert_eq!(s.kind, SnapshotKind::V8Heap);
        let h = s.heap.as_ref().unwrap();
        assert_eq!((h.node_count, h.edge_count), (12, 13));
        assert_eq!(h.total_size, 2_010);
        assert_eq!(s.total, 2_010);
        // The second Cache is only weakly reachable.
        assert_eq!(h.unreachable_size, 40);
        let class = |name: &str| h.classes.iter().find(|c| c.name == name).unwrap();
        let row = |c: &HeapClass| (c.count, c.shallow, c.retained);
        assert_eq!(row(class("Global")), (1, 100, 2_010));
        assert_eq!(row(class("Cache")), (1, 40, 40));
        // The Map is retained by the Cache and the closure: neither dominates it.
        assert_eq!(row(class("Map")), (1, 32, 1_306));
        assert_eq!(row(class("(array)")), (1, 1_000, 1_274));
        assert_eq!(row(class("(closure)")), (1, 64, 564));
        assert_eq!(row(class("(compiled code)")), (1, 500, 500));
        assert_eq!(row(class("(string)")), (1, 24, 24));
        // A Report inside a Report is counted once.
        assert_eq!(row(class("Report")), (2, 250, 250));
        assert_eq!(h.classes[0].name, "Global");
        let reports = &class("Report").objects;
        assert_eq!((reports[0].id, reports[0].retained), (19, 250));
        assert_eq!(reports[1].retained, 50);
        // Biggest objects (system and code objects left out).
        let names: Vec<_> = h.biggest.iter().take(4).map(|o| o.name.as_str()).collect();
        assert_eq!(names, ["Global", "Map", "(array)", "render()"]);
        assert!(h.biggest.iter().any(|o| o.name == "\"row 1\""));
        // The stream reader gives the same result.
        assert_eq!(parse_reader(SMALL, "small.heapsnapshot").unwrap(), s);
        assert!(parse(b"{\"snapshot\":{}}", "x").is_err());
        assert!(parse(b"[]", "x").is_err());
    }
}
