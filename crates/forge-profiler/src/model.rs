//! The snapshot model shared by every format: functions with their execution statistics
//! (calls, own and total cost per metric), the caller → callee edges, and a call tree that
//! becomes `goforge-pprof`'s [`ProfileView`] (call tree, flame graph, method list).
//!
//! Formats with real call stacks (SPX, V8) fill the tree with [`Graph::child`] and let
//! [`Graph::finish`] derive the statistics; cachegrind files only know the edges, so their
//! statistics come from the file and the tree is spread from them ([`Graph::finish_with`]).

use std::collections::HashMap;

use goforge_pprof::{Frame, Profile, ProfileView, Sample, ValueType, ViewOptions};
use serde::{Deserialize, Serialize};

use crate::heapsnapshot::HeapSummary;

/// Which tool wrote a snapshot (the Profiler source it is shown in).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SnapshotKind {
    /// Xdebug's profiler (`cachegrind.out.*`).
    Xdebug,
    /// php-spx full reports (`spx-full-*.json` + `.txt.gz`).
    Spx,
    /// V8 CPU profiles (`.cpuprofile`).
    V8Cpu,
    /// V8 sampling heap profiles (`.heapprofile`).
    V8HeapSampling,
    /// V8 heap snapshots (`.heapsnapshot`).
    V8Heap,
}

impl SnapshotKind {
    /// The Profiler source that shows the snapshot.
    pub fn source(self) -> &'static str {
        match self {
            SnapshotKind::Xdebug => "xdebug",
            SnapshotKind::Spx => "spx",
            SnapshotKind::V8Cpu => "v8-cpu",
            SnapshotKind::V8HeapSampling | SnapshotKind::V8Heap => "v8-heap",
        }
    }
}

/// A line of the snapshot's header (`cmd`, `creator`, host, …).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InfoItem {
    pub label: String,
    pub value: String,
}

/// A row of the Execution Statistics for the shown metric.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionStats {
    pub name: String,
    /// Source file (empty or `php:internal` for built-in functions).
    pub file: String,
    /// 1-based line of the function (0: unknown).
    pub line: u32,
    /// Invocations (SPX, Xdebug) or samples (V8).
    pub calls: u64,
    /// Cost in the function itself.
    pub own: i64,
    /// Cost in the function and everything it called (recursion counted once).
    pub total: i64,
}

/// A caller → callee pair (indexes into [`Snapshot::functions`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallEdge {
    pub caller: usize,
    pub callee: usize,
    pub calls: u64,
    /// Inclusive cost of the callee when called from the caller.
    pub total: i64,
}

/// A profiler snapshot as the Profiler shows it, for one metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub kind: SnapshotKind,
    pub title: String,
    pub info: Vec<InfoItem>,
    /// Every metric of the snapshot (`Time`/nanoseconds, `Memory`/bytes, …).
    pub metrics: Vec<ValueType>,
    /// The shown metric.
    pub metric: usize,
    /// The heading of the calls column: `Calls` or `Samples`.
    pub calls_label: String,
    /// Total cost of the shown metric.
    pub total: i64,
    pub functions: Vec<FunctionStats>,
    pub edges: Vec<CallEdge>,
    /// Call tree, flame graph and method list (not for heap snapshots).
    pub view: Option<ProfileView>,
    /// The Memory view (heap snapshots).
    pub heap: Option<HeapSummary>,
}

impl Snapshot {
    /// Rewrite source paths (container or remote paths to local ones, `file://` URLs).
    pub fn map_files(&mut self, map: &dyn Fn(&str) -> Option<String>) {
        for f in &mut self.functions {
            if let Some(m) = map(&f.file) {
                f.file = m;
            }
        }
        if let Some(v) = &mut self.view {
            for r in &mut v.top {
                if let Some(m) = map(&r.file) {
                    r.file = m;
                }
            }
            let mut stack = vec![&mut v.tree];
            while let Some(n) = stack.pop() {
                if let Some(m) = map(&n.file) {
                    n.file = m;
                }
                stack.extend(n.children.iter_mut());
            }
        }
    }
}

/// A function of a [`Graph`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Func {
    pub name: String,
    pub file: String,
    pub line: u32,
}

/// A node of the call tree: a function on one call path.
#[derive(Debug, Clone)]
pub(crate) struct TreeNode {
    pub func: usize,
    pub calls: u64,
    pub self_values: Vec<i64>,
    pub children: Vec<usize>,
}

/// `(caller, callee)` and (calls, inclusive cost per metric).
pub(crate) type EdgeStats = ((usize, usize), (u64, Vec<i64>));

/// Add `src` to `dst` element by element.
pub(crate) fn add_into(dst: &mut [i64], src: &[i64]) {
    for (d, s) in dst.iter_mut().zip(src) {
        *d += s;
    }
}

/// Execution statistics computed elsewhere (cachegrind), per metric.
#[derive(Debug, Clone, Default)]
pub(crate) struct Stats {
    pub calls: Vec<u64>,
    pub own: Vec<Vec<i64>>,
    pub total: Vec<Vec<i64>>,
    /// `(caller, callee)` → (calls, inclusive cost per metric).
    pub edges: Vec<EdgeStats>,
}

/// Functions plus a call tree, built by the format parsers.
#[derive(Debug, Clone)]
pub(crate) struct Graph {
    pub metrics: Vec<ValueType>,
    pub funcs: Vec<Func>,
    func_index: HashMap<(String, String, u32), usize>,
    pub nodes: Vec<TreeNode>,
    child_index: HashMap<(usize, usize), usize>,
}

/// The root of the call tree.
pub(crate) const ROOT: usize = 0;

impl Graph {
    pub fn new(metrics: Vec<ValueType>) -> Self {
        let n = metrics.len();
        Graph {
            metrics,
            funcs: Vec::new(),
            func_index: HashMap::new(),
            nodes: vec![TreeNode {
                func: usize::MAX,
                calls: 0,
                self_values: vec![0; n],
                children: Vec::new(),
            }],
            child_index: HashMap::new(),
        }
    }

    /// The function `name` in `file` at `line` (created on first use).
    pub fn func(&mut self, name: &str, file: &str, line: u32) -> usize {
        let key = (name.to_string(), file.to_string(), line);
        if let Some(i) = self.func_index.get(&key) {
            return *i;
        }
        self.funcs.push(Func {
            name: name.to_string(),
            file: file.to_string(),
            line,
        });
        let i = self.funcs.len() - 1;
        self.func_index.insert(key, i);
        i
    }

    /// The child of `parent` for `func` (created on first use).
    pub fn child(&mut self, parent: usize, func: usize) -> usize {
        if let Some(n) = self.child_index.get(&(parent, func)) {
            return *n;
        }
        self.nodes.push(TreeNode {
            func,
            calls: 0,
            self_values: vec![0; self.metrics.len()],
            children: Vec::new(),
        });
        let n = self.nodes.len() - 1;
        self.nodes[parent].children.push(n);
        self.child_index.insert((parent, func), n);
        n
    }

    /// Add cost to a node itself.
    pub fn add_self(&mut self, node: usize, values: &[i64]) {
        let s = &mut self.nodes[node].self_values;
        for (i, v) in values.iter().enumerate() {
            if let Some(x) = s.get_mut(i) {
                *x += v;
            }
        }
    }

    /// Inclusive values of every node (self plus children), children come after parents.
    fn totals(&self) -> Vec<Vec<i64>> {
        let mut parent = vec![usize::MAX; self.nodes.len()];
        for (i, n) in self.nodes.iter().enumerate() {
            for c in &n.children {
                parent[*c] = i;
            }
        }
        let mut totals: Vec<Vec<i64>> = self.nodes.iter().map(|n| n.self_values.clone()).collect();
        // Children are created after their parents: going backwards completes every node
        // before it is added to its parent.
        for i in (1..self.nodes.len()).rev() {
            let p = parent[i];
            if p == usize::MAX {
                continue;
            }
            let child = std::mem::take(&mut totals[i]);
            add_into(&mut totals[p], &child);
            totals[i] = child;
        }
        totals
    }

    /// Statistics from the call tree: own cost summed over a function's nodes, total cost
    /// of its outermost nodes only (so recursion is counted once).
    fn tree_stats(&self) -> Stats {
        let n = self.funcs.len();
        let metrics = self.metrics.len();
        let totals = self.totals();
        let mut stats = Stats {
            calls: vec![0; n],
            own: vec![vec![0; metrics]; n],
            total: vec![vec![0; metrics]; n],
            edges: Vec::new(),
        };
        let mut edges: HashMap<(usize, usize), (u64, Vec<i64>)> = HashMap::new();
        let mut active = vec![0u32; n];
        // (node, parent function, entered)
        let mut stack: Vec<(usize, usize, bool)> = self.nodes[ROOT]
            .children
            .iter()
            .rev()
            .map(|c| (*c, usize::MAX, false))
            .collect();
        while let Some((node, parent, entered)) = stack.pop() {
            let f = self.nodes[node].func;
            if entered {
                active[f] -= 1;
                continue;
            }
            let tn = &self.nodes[node];
            stats.calls[f] += tn.calls;
            add_into(&mut stats.own[f], &tn.self_values);
            if active[f] == 0 {
                add_into(&mut stats.total[f], &totals[node]);
            }
            if parent != usize::MAX {
                let e = edges
                    .entry((parent, f))
                    .or_insert_with(|| (0, vec![0; metrics]));
                e.0 += tn.calls;
                if active[f] == 0 {
                    add_into(&mut e.1, &totals[node]);
                }
            }
            active[f] += 1;
            stack.push((node, parent, true));
            for c in tn.children.iter().rev() {
                stack.push((*c, f, false));
            }
        }
        let mut list: Vec<_> = edges.into_iter().collect();
        list.sort_by_key(|(k, _)| *k);
        stats.edges = list;
        stats
    }

    /// The call tree as a pprof profile: one sample per node with its own cost.
    fn profile(&self) -> Profile {
        let frames: Vec<Frame> = self
            .funcs
            .iter()
            .map(|f| Frame {
                function: f.name.clone(),
                file: f.file.clone(),
                line: f.line,
                start_line: f.line,
            })
            .collect();
        let mut samples = Vec::new();
        // Paths are leaf first, as pprof's are.
        let mut stack: Vec<(usize, Vec<usize>)> = self.nodes[ROOT]
            .children
            .iter()
            .map(|c| (*c, Vec::new()))
            .collect();
        while let Some((node, path)) = stack.pop() {
            let tn = &self.nodes[node];
            let mut here = Vec::with_capacity(path.len() + 1);
            here.push(tn.func);
            here.extend_from_slice(&path);
            if tn.self_values.iter().any(|v| *v != 0) {
                samples.push(Sample {
                    stack: here.clone(),
                    values: tn.self_values.clone(),
                });
            }
            for c in &tn.children {
                stack.push((*c, here.clone()));
            }
        }
        Profile {
            sample_types: self.metrics.clone(),
            default_sample_type: 0,
            samples,
            frames,
            period_type: None,
            period: 0,
            time_nanos: 0,
            duration_nanos: 0,
        }
    }

    /// The snapshot for `metric`, statistics derived from the call tree.
    pub fn finish(self, head: Head, metric: Option<usize>) -> Snapshot {
        let stats = self.tree_stats();
        self.finish_with(head, stats, metric)
    }

    /// The snapshot for `metric` with statistics computed by the parser.
    pub fn finish_with(self, head: Head, stats: Stats, metric: Option<usize>) -> Snapshot {
        let metric = metric.filter(|m| *m < self.metrics.len()).unwrap_or(
            head.default_metric
                .min(self.metrics.len().saturating_sub(1)),
        );
        let functions: Vec<FunctionStats> = self
            .funcs
            .iter()
            .enumerate()
            .map(|(i, f)| FunctionStats {
                name: f.name.clone(),
                file: f.file.clone(),
                line: f.line,
                calls: stats.calls.get(i).copied().unwrap_or(0),
                own: stats
                    .own
                    .get(i)
                    .and_then(|v| v.get(metric))
                    .copied()
                    .unwrap_or(0),
                total: stats
                    .total
                    .get(i)
                    .and_then(|v| v.get(metric))
                    .copied()
                    .unwrap_or(0),
            })
            .collect();
        let edges = stats
            .edges
            .iter()
            .map(|((caller, callee), (calls, total))| CallEdge {
                caller: *caller,
                callee: *callee,
                calls: *calls,
                total: total.get(metric).copied().unwrap_or(0),
            })
            .collect();
        let view = goforge_pprof::view(
            &self.profile(),
            Some(metric),
            ViewOptions {
                top_limit: 500,
                ..ViewOptions::default()
            },
        );
        let mut view = view;
        view.duration_nanos = head.duration_nanos;
        let total = head
            .totals
            .as_ref()
            .and_then(|t| t.get(metric).copied())
            .unwrap_or(view.total);
        Snapshot {
            kind: head.kind,
            title: head.title,
            info: head.info,
            metrics: self.metrics,
            metric,
            calls_label: head.calls_label.to_string(),
            total,
            functions,
            edges,
            view: Some(view),
            heap: None,
        }
    }
}

/// What a parser knows about the snapshot besides the graph.
#[derive(Debug, Clone)]
pub(crate) struct Head {
    pub kind: SnapshotKind,
    pub title: String,
    pub info: Vec<InfoItem>,
    pub calls_label: &'static str,
    pub default_metric: usize,
    /// Totals written by the tool (cachegrind `summary:`), per metric.
    pub totals: Option<Vec<i64>>,
    pub duration_nanos: i64,
}

impl Head {
    pub fn new(kind: SnapshotKind, title: impl Into<String>) -> Self {
        Head {
            kind,
            title: title.into(),
            info: Vec::new(),
            calls_label: "Calls",
            default_metric: 0,
            totals: None,
            duration_nanos: 0,
        }
    }

    pub fn info(&mut self, label: &str, value: impl Into<String>) {
        let value = value.into();
        if !value.is_empty() {
            self.info.push(InfoItem {
                label: label.to_string(),
                value,
            });
        }
    }
}

/// A metric (sample type) value.
pub(crate) fn metric(kind: &str, unit: &str) -> ValueType {
    ValueType {
        kind: kind.to_string(),
        unit: unit.to_string(),
    }
}
