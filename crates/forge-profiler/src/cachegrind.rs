//! Xdebug profiler snapshots: the callgrind format (`cachegrind.out.*`, also gzip
//! compressed) with `fl=`/`fn=`/`cfl=`/`cfn=`/`calls=` lines, name compression (`(id)`),
//! relative positions (`+n`, `-n`, `*`) and the `Time_(10ns)` / `Memory_(bytes)` events
//! Xdebug 3 writes (Xdebug 2: `Time` in microseconds, `Memory`).
//!
//! Xdebug writes one `fn=` block per finished call with the function's own cost and the
//! inclusive cost of each call it made, so the statistics are exact while the call tree
//! is spread top-down from the edges (each callee's share is proportional to the edge's
//! cost), as KCachegrind and PHPStorm do.

use std::collections::HashMap;

use crate::model::{add_into, metric, Graph, Head, SnapshotKind, Stats, ROOT};
use crate::Snapshot;

/// The deepest level of the spread call tree.
const MAX_DEPTH: usize = 64;
/// Call tree nodes kept at most.
const MAX_NODES: usize = 50_000;
/// Calls below this fraction of the total are folded into their caller.
const MIN_FRACTION: f64 = 0.0005;

#[derive(Debug, Default)]
struct CgFunc {
    name: String,
    file: String,
    line: u32,
    own: Vec<i64>,
    blocks: u64,
}

/// A name table of the compressed format: `(id) name` defines, `(id)` refers.
#[derive(Default)]
struct Names(HashMap<String, String>);

impl Names {
    fn resolve(&mut self, value: &str) -> String {
        let v = value.trim();
        if let Some(rest) = v.strip_prefix('(') {
            if let Some(end) = rest.find(')') {
                let id = &rest[..end];
                let name = rest[end + 1..].trim();
                if name.is_empty() {
                    return self.0.get(id).cloned().unwrap_or_default();
                }
                self.0.insert(id.to_string(), name.to_string());
                return name.to_string();
            }
        }
        v.to_string()
    }
}

/// How an event's values become the metric's unit.
fn event_metric(name: &str) -> (goforge_pprof::ValueType, i64) {
    let lower = name.to_ascii_lowercase();
    let scale_of = |default: i64| {
        if lower.contains("(10ns)") {
            10
        } else if lower.contains("(100ns)") {
            100
        } else if lower.contains("(ns)") {
            1
        } else if lower.contains("(us)") || lower.contains("(µs)") || lower.contains("(μs)") {
            1000
        } else if lower.contains("(ms)") {
            1_000_000
        } else {
            default
        }
    };
    let pretty = name.split("_(").next().unwrap_or(name).to_string();
    if lower.starts_with("time") {
        // Xdebug 2 wrote plain `Time` in microseconds.
        (metric(&pretty, "nanoseconds"), scale_of(1000))
    } else if lower.starts_with("memory") || lower.contains("(bytes)") {
        (metric(&pretty, "bytes"), 1)
    } else {
        (metric(&pretty, "count"), 1)
    }
}

fn number(s: &str) -> Option<i64> {
    if let Some(hex) = s.strip_prefix("0x") {
        i64::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<i64>().ok()
    }
}

/// Parse a callgrind / cachegrind file (plain or gzip) into a snapshot of `metric`.
pub fn parse(data: &[u8], name: &str, metric_index: Option<usize>) -> Result<Snapshot, String> {
    let text = crate::text(data)?;
    let mut head = Head::new(SnapshotKind::Xdebug, name);
    let mut metrics = Vec::new();
    let mut scales: Vec<i64> = Vec::new();
    let mut positions = 1usize;
    let mut summary: Option<Vec<i64>> = None;
    let mut files = Names::default();
    let mut fn_names = Names::default();
    let mut funcs: Vec<CgFunc> = Vec::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut edges: HashMap<(usize, usize), (u64, Vec<i64>)> = HashMap::new();
    let mut fl = String::new();
    let mut current: Option<usize> = None;
    let mut call_file: Option<String> = None;
    let mut call_fn: Option<String> = None;
    // A `calls=` line: the next cost line is that call's inclusive cost.
    let mut pending_call: Option<(usize, u64)> = None;
    let mut skip_next = false;
    let mut last_pos = [0i64; 2];
    let mut cmd = String::new();
    let mut seen_header = false;

    let mut func_of = |funcs: &mut Vec<CgFunc>, name: &str, file: &str| -> usize {
        if let Some(i) = by_name.get(name) {
            if funcs[*i].file.is_empty() && !file.is_empty() {
                funcs[*i].file = file.to_string();
            }
            return *i;
        }
        funcs.push(CgFunc {
            name: name.to_string(),
            file: file.to_string(),
            ..CgFunc::default()
        });
        by_name.insert(name.to_string(), funcs.len() - 1);
        funcs.len() - 1
    };

    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if skip_next {
            skip_next = false;
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let first = line.as_bytes()[0];
        if first.is_ascii_digit() || first == b'+' || first == b'-' || first == b'*' {
            // A cost line: positions, then one value per event.
            let mut parts = line.split_ascii_whitespace();
            let mut pos0 = 0;
            for p in 0..positions {
                let Some(tok) = parts.next() else { break };
                let v = match tok.as_bytes()[0] {
                    b'*' => last_pos[p.min(1)],
                    b'+' => last_pos[p.min(1)] + number(&tok[1..]).unwrap_or(0),
                    b'-' => last_pos[p.min(1)] - number(&tok[1..]).unwrap_or(0),
                    _ => number(tok).unwrap_or(0),
                };
                last_pos[p.min(1)] = v;
                if p == positions - 1 {
                    pos0 = v;
                }
            }
            let mut values = vec![0i64; metrics.len()];
            for (i, tok) in parts.enumerate() {
                if i < values.len() {
                    values[i] = number(tok).unwrap_or(0) * scales[i];
                }
            }
            let Some(f) = current else { continue };
            if let Some((callee, calls)) = pending_call.take() {
                let e = edges
                    .entry((f, callee))
                    .or_insert_with(|| (0, vec![0; metrics.len()]));
                e.0 += calls;
                for (i, v) in values.iter().enumerate() {
                    e.1[i] += v;
                }
            } else {
                let func = &mut funcs[f];
                if func.line == 0 && pos0 > 0 {
                    func.line = pos0 as u32;
                }
                for (i, v) in values.iter().enumerate() {
                    func.own[i] += v;
                }
            }
            continue;
        }
        let (key, value) = match line.find(['=', ':']) {
            Some(i) => (&line[..i], &line[i + 1..]),
            None => continue,
        };
        match key {
            "version" | "creator" | "cmd" | "part" | "pid" | "desc" | "positions" | "events"
            | "summary" | "totals" | "thread" => seen_header = true,
            _ => {}
        }
        match key {
            "creator" => head.info("Creator", value.trim()),
            "cmd" => {
                cmd = value.trim().to_string();
                head.info("Command", &cmd);
            }
            "pid" => head.info("Process", value.trim()),
            "positions" => {
                positions = value.split_ascii_whitespace().count().clamp(1, 2);
            }
            "events" => {
                metrics.clear();
                scales.clear();
                for e in value.split_ascii_whitespace() {
                    let (m, s) = event_metric(e);
                    metrics.push(m);
                    scales.push(s);
                }
                for f in &mut funcs {
                    f.own.resize(metrics.len(), 0);
                }
            }
            "summary" | "totals" => {
                let v: Vec<i64> = value
                    .split_ascii_whitespace()
                    .enumerate()
                    .map(|(i, t)| number(t).unwrap_or(0) * scales.get(i).copied().unwrap_or(1))
                    .collect();
                if !v.is_empty() {
                    summary = Some(v);
                }
            }
            "fl" | "fi" | "fe" => {
                let name = files.resolve(value);
                if key == "fl" {
                    fl = name;
                }
            }
            "fn" => {
                let name = fn_names.resolve(value);
                let i = func_of(&mut funcs, &name, &fl);
                funcs[i].own.resize(metrics.len(), 0);
                funcs[i].blocks += 1;
                current = Some(i);
                call_file = None;
                call_fn = None;
            }
            "cfl" | "cfi" => call_file = Some(files.resolve(value)),
            "cfn" => call_fn = Some(fn_names.resolve(value)),
            "calls" => {
                let count = value
                    .split_ascii_whitespace()
                    .next()
                    .and_then(number)
                    .unwrap_or(1)
                    .max(0) as u64;
                if let Some(name) = call_fn.take() {
                    let file = call_file.take().unwrap_or_else(|| fl.clone());
                    let callee = func_of(&mut funcs, &name, &file);
                    funcs[callee].own.resize(metrics.len(), 0);
                    pending_call = Some((callee, count));
                } else {
                    // A call without a callee: skip its cost line.
                    skip_next = true;
                }
            }
            "jump" | "jcnd" => skip_next = true,
            _ => {}
        }
    }
    if !seen_header && funcs.is_empty() {
        return Err("not a cachegrind file".into());
    }
    if metrics.is_empty() {
        return Err("the cachegrind file declares no events".into());
    }
    if funcs.is_empty() {
        return Err("the cachegrind file contains no functions".into());
    }
    if !cmd.is_empty() {
        let script = cmd.rsplit(['/', '\\']).next().unwrap_or(&cmd);
        head.title = format!("{name} ({script})");
    }
    let m = metrics.len();
    let n = funcs.len();

    // Statistics: own cost, total = own + non-recursive calls, calls = incoming calls.
    let mut stats = Stats {
        calls: vec![0; n],
        own: funcs.iter().map(|f| f.own.clone()).collect(),
        total: funcs.iter().map(|f| f.own.clone()).collect(),
        edges: Vec::new(),
    };
    let mut out: Vec<Vec<(usize, u64, Vec<i64>)>> = vec![Vec::new(); n];
    let mut has_caller = vec![false; n];
    let mut edge_list: Vec<_> = edges.into_iter().collect();
    edge_list.sort_by_key(|(k, _)| *k);
    for ((caller, callee), (calls, cost)) in &edge_list {
        stats.calls[*callee] += calls;
        if caller != callee {
            has_caller[*callee] = true;
            add_into(&mut stats.total[*caller], cost);
            out[*caller].push((*callee, *calls, cost.clone()));
        }
    }
    for (i, f) in funcs.iter().enumerate() {
        if stats.calls[i] == 0 {
            stats.calls[i] = f.blocks.max(1);
        }
    }
    stats.edges = edge_list;
    let metric_index = metric_index.filter(|i| *i < m).unwrap_or(0);
    head.totals = Some(match summary {
        Some(mut s) => {
            s.resize(m, 0);
            s
        }
        None => (0..m)
            .map(|i| funcs.iter().map(|f| f.own[i]).sum())
            .collect(),
    });

    // The call tree of the shown metric, spread top-down from the roots.
    let mut graph = Graph::new(metrics);
    for f in &funcs {
        graph.func(&f.name, &f.file, f.line);
    }
    let incl: Vec<i64> = (0..n).map(|i| stats.total[i][metric_index]).collect();
    let own: Vec<i64> = (0..n).map(|i| stats.own[i][metric_index]).collect();
    let grand: i64 = (0..n)
        .filter(|i| !has_caller[*i])
        .map(|i| incl[i].abs())
        .sum::<i64>()
        .max(1);
    let mut roots: Vec<usize> = (0..n).filter(|i| !has_caller[*i]).collect();
    if roots.is_empty() {
        roots = (0..n).max_by_key(|i| incl[*i]).into_iter().collect();
    }
    roots.sort_by(|a, b| incl[*b].cmp(&incl[*a]));
    let mut spread = Spread {
        graph: &mut graph,
        out: &out,
        incl: &incl,
        own: &own,
        metric: metric_index,
        min: grand as f64 * MIN_FRACTION,
        path: Vec::new(),
    };
    for r in roots {
        spread.expand(ROOT, r, incl[r] as f64, 1);
    }
    Ok(graph.finish_with(head, stats, Some(metric_index)))
}

struct Spread<'a> {
    graph: &'a mut Graph,
    out: &'a [Vec<(usize, u64, Vec<i64>)>],
    incl: &'a [i64],
    own: &'a [i64],
    metric: usize,
    min: f64,
    path: Vec<usize>,
}

impl Spread<'_> {
    fn expand(&mut self, parent: usize, f: usize, value: f64, calls: u64) {
        let node = self.graph.child(parent, f);
        self.graph.nodes[node].calls += calls;
        let ratio = if self.incl[f] != 0 {
            value / self.incl[f] as f64
        } else {
            0.0
        };
        let mut self_value = if self.incl[f] != 0 {
            self.own[f] as f64 * ratio
        } else {
            value
        };
        self.path.push(f);
        let deep = self.path.len() >= MAX_DEPTH;
        for (g, edge_calls, cost) in &self.out[f] {
            let v = cost[self.metric] as f64 * ratio;
            if v == 0.0 {
                continue;
            }
            if self.path.contains(g) {
                // Recursion: the callee's cost is already in its outer call.
                continue;
            }
            if deep || v.abs() < self.min || self.graph.nodes.len() >= MAX_NODES {
                self_value += v;
                continue;
            }
            let c = ((*edge_calls as f64) * ratio).round().max(1.0) as u64;
            self.expand(node, *g, v, c);
        }
        self.path.pop();
        let mut values = vec![0i64; self.graph.metrics.len()];
        values[self.metric] = self_value.round() as i64;
        self.graph.add_self(node, &values);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const XDEBUG: &str = include_str!("../tests/fixtures/cachegrind.out.4242");

    fn func<'a>(s: &'a Snapshot, name: &str) -> (usize, &'a crate::FunctionStats) {
        s.functions
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == name)
            .unwrap_or_else(|| panic!("{name} missing"))
    }

    #[test]
    fn reads_an_xdebug_3_snapshot() {
        let s = parse(XDEBUG.as_bytes(), "cachegrind.out.4242", None).unwrap();
        assert_eq!(s.kind, SnapshotKind::Xdebug);
        assert_eq!(s.title, "cachegrind.out.4242 (report.php)");
        let kinds: Vec<_> = s
            .metrics
            .iter()
            .map(|m| (m.kind.as_str(), m.unit.as_str()))
            .collect();
        assert_eq!(kinds, [("Time", "nanoseconds"), ("Memory", "bytes")]);
        assert!(s
            .info
            .iter()
            .any(|i| i.label == "Creator" && i.value.starts_with("xdebug 3")));
        // summary: 14700 (10ns) → 147000 ns.
        assert_eq!(s.total, 147_000);

        let (main_i, main) = func(&s, "{main}");
        assert_eq!(main.file, "/home/anna/shop/bin/report.php");
        assert_eq!(main.calls, 1);
        assert_eq!(main.own, 1_000);
        assert_eq!(main.total, 147_000);

        // Two blocks of Report->row (two calls), compressed names resolved.
        let (row_i, row) = func(&s, "Report->row");
        assert_eq!(row.calls, 2);
        assert_eq!(row.line, 9);
        assert_eq!(row.file, "/home/anna/shop/src/Report.php");
        assert_eq!(row.own, (3_000 + 2_000) * 10);
        let (_, repeat) = func(&s, "php::str_repeat");
        assert_eq!(repeat.file, "php:internal");
        assert_eq!(repeat.calls, 2);
        assert_eq!(repeat.own, (500 + 600) * 10);
        assert_eq!(row.total, row.own + repeat.own);

        // Recursion: fib calls itself, its total is not counted twice.
        let (fib_i, fib) = func(&s, "fib");
        assert_eq!(fib.calls, 1 + 2);
        assert_eq!(fib.own, (4_000 + 2_000 + 1_000) * 10);
        assert_eq!(fib.total, fib.own);

        // Callers and callees.
        let (render_i, _) = func(&s, "Report->render");
        let callees: Vec<_> = s
            .edges
            .iter()
            .filter(|e| e.caller == render_i)
            .map(|e| (s.functions[e.callee].name.as_str(), e.calls))
            .collect();
        assert!(callees.contains(&("Report->row", 2)));
        assert!(s
            .edges
            .iter()
            .any(|e| e.caller == main_i && e.callee == render_i));
        assert!(s
            .edges
            .iter()
            .any(|e| e.caller == fib_i && e.callee == fib_i && e.calls == 2));
        assert!(s.edges.iter().any(|e| e.callee == row_i && e.calls == 2));

        // The spread call tree: {main} → Report->render → Report->row → php::str_repeat.
        let v = s.view.as_ref().unwrap();
        assert_eq!(v.tree.children[0].name, "{main}");
        assert_eq!(v.tree.children[0].total, 147_000);
        let render = v.tree.children[0]
            .children
            .iter()
            .find(|c| c.name == "Report->render")
            .unwrap();
        assert_eq!(render.children[0].name, "Report->row");
        assert_eq!(render.children[0].children[0].name, "php::str_repeat");
        // The method list's own times add up to the total.
        assert_eq!(v.top.iter().map(|r| r.flat).sum::<i64>(), v.total);
        assert_eq!(v.total, 147_000);
    }

    #[test]
    fn shows_the_memory_metric_and_gzip_files() {
        use std::io::Write;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(XDEBUG.as_bytes()).unwrap();
        let data = gz.finish().unwrap();
        let s = parse(&data, "cachegrind.out.4242.gz", Some(1)).unwrap();
        assert_eq!(s.metric, 1);
        assert_eq!(s.total, 17_128);
        let (_, row) = func(&s, "Report->row");
        assert_eq!(row.own, 4_096 + 2_048);
        assert_eq!(s.view.as_ref().unwrap().sample_index, 1);
    }

    #[test]
    fn handles_relative_positions_and_xdebug_2_time() {
        let text = "version: 1\ncmd: a.php\npositions: line\nevents: Time\n\nfl=a.php\nfn=main\n3 10\n+2 5\ncfn=f\ncalls=1 +1\n* 7\n\nfn=f\n20 7\n";
        let s = parse(text.as_bytes(), "x", None).unwrap();
        let (_, main) = func(&s, "main");
        assert_eq!(main.line, 3);
        assert_eq!(main.own, 15_000);
        assert_eq!(main.total, 22_000);
        let (_, f) = func(&s, "f");
        assert_eq!(f.file, "a.php");
        assert_eq!(f.line, 20);
        assert!(parse(b"hello world", "x", None).is_err());
    }
}
