//! V8 profiles written by Node.js and Chrome DevTools:
//!
//! - CPU profiles (`.cpuprofile`, `node --cpu-prof`): the nodes of the sampled call tree
//!   with their call frames, and the samples with their time deltas (microseconds), so each
//!   node's own time is the time of the samples that hit it.
//! - Sampling heap profiles (`.heapprofile`, `node --heap-prof`): a nested tree of call
//!   frames with the bytes allocated there (`selfSize`) and the samples (`nodeId`).
//!
//! Locations of bundled or compiled scripts are mapped to their sources through source
//! maps when a [`MapResolver`] finds them.

use std::collections::HashMap;

use serde::Deserialize;

use crate::model::{metric, Graph, Head, SnapshotKind, ROOT};
use crate::sourcemap::SourceMap;
use crate::Snapshot;

/// Finds the source map of a script path (`None`: not mapped).
pub type MapResolver<'a> = &'a dyn Fn(&str) -> Option<SourceMap>;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct CallFrame {
    function_name: String,
    url: String,
    line_number: i64,
    column_number: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CpuNode {
    id: u64,
    #[serde(default)]
    call_frame: CallFrame,
    #[serde(default)]
    hit_count: u64,
    #[serde(default)]
    children: Vec<u64>,
    #[serde(default)]
    parent: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CpuProfile {
    nodes: Vec<CpuNode>,
    #[serde(default)]
    start_time: i64,
    #[serde(default)]
    end_time: i64,
    #[serde(default)]
    samples: Vec<u64>,
    #[serde(default)]
    time_deltas: Vec<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HeapNode {
    #[serde(default)]
    call_frame: CallFrame,
    #[serde(default)]
    self_size: i64,
    #[serde(default)]
    id: u64,
    #[serde(default)]
    children: Vec<HeapNode>,
}

#[derive(Debug, Deserialize)]
struct HeapSample {
    #[serde(default, rename = "nodeId")]
    node_id: u64,
}

#[derive(Debug, Deserialize)]
struct HeapProfile {
    head: HeapNode,
    #[serde(default)]
    samples: Vec<HeapSample>,
}

/// `file:///a/b%20c.js` → `/a/b c.js`; other URLs (`node:fs`, `https://…`) unchanged.
pub fn script_path(url: &str) -> String {
    match url.strip_prefix("file://") {
        Some(p) => crate::url_path(p),
        None => url.to_string(),
    }
}

/// Call frames → functions, through source maps.
struct Frames<'a> {
    resolver: Option<MapResolver<'a>>,
    maps: HashMap<String, Option<SourceMap>>,
}

impl Frames<'_> {
    fn func(&mut self, graph: &mut Graph, f: &CallFrame) -> usize {
        let mut name = if f.function_name.is_empty() {
            "(anonymous)".to_string()
        } else {
            f.function_name.clone()
        };
        let mut file = script_path(&f.url);
        let mut line = if f.line_number >= 0 {
            f.line_number as u32 + 1
        } else {
            0
        };
        if let (Some(resolve), true) = (self.resolver, file.starts_with('/') && line > 0) {
            let map = self
                .maps
                .entry(file.clone())
                .or_insert_with(|| resolve(&file));
            if let Some(o) = map
                .as_ref()
                .and_then(|m| m.lookup(line - 1, f.column_number.max(0) as u32))
            {
                file = o.source;
                line = o.line + 1;
                if f.function_name.is_empty() {
                    if let Some(n) = o.name {
                        name = n;
                    }
                }
            }
        }
        graph.func(&name, &file, line)
    }
}

/// Parse a `.cpuprofile` into a snapshot (metrics: CPU time, samples).
pub fn cpu_profile(
    data: &[u8],
    name: &str,
    metric_index: Option<usize>,
    resolver: Option<MapResolver<'_>>,
) -> Result<Snapshot, String> {
    let p: CpuProfile =
        serde_json::from_slice(data).map_err(|e| format!("not a V8 CPU profile: {e}"))?;
    if p.nodes.is_empty() {
        return Err("the CPU profile has no nodes".into());
    }
    let index: HashMap<u64, usize> = p.nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    // Children from `children`, or from `parent` (older profiles).
    let mut children: Vec<Vec<usize>> = p
        .nodes
        .iter()
        .map(|n| {
            n.children
                .iter()
                .filter_map(|c| index.get(c).copied())
                .collect()
        })
        .collect();
    let mut has_parent = vec![false; p.nodes.len()];
    for (i, n) in p.nodes.iter().enumerate() {
        if let Some(parent) = n.parent.and_then(|x| index.get(&x).copied()) {
            if !children[parent].contains(&i) {
                children[parent].push(i);
            }
        }
    }
    for c in children.iter().flatten() {
        has_parent[*c] = true;
    }
    // Own time and samples per node.
    let mut time = vec![0i64; p.nodes.len()];
    let mut hits = vec![0u64; p.nodes.len()];
    if !p.samples.is_empty() && p.samples.len() == p.time_deltas.len() {
        let mut stamps = Vec::with_capacity(p.samples.len());
        let mut t = p.start_time;
        for d in &p.time_deltas {
            t += d;
            stamps.push(t);
        }
        for (i, s) in p.samples.iter().enumerate() {
            let Some(n) = index.get(s).copied() else {
                continue;
            };
            let next = stamps
                .get(i + 1)
                .copied()
                .unwrap_or(p.end_time.max(stamps[i]));
            time[n] += (next - stamps[i]).max(0);
            hits[n] += 1;
        }
    } else {
        let total: u64 = p.nodes.iter().map(|n| n.hit_count).sum();
        let interval = if total > 0 {
            (p.end_time - p.start_time).max(0) / total as i64
        } else {
            0
        };
        for (i, n) in p.nodes.iter().enumerate() {
            hits[i] = n.hit_count;
            time[i] = n.hit_count as i64 * interval;
        }
    }
    let mut graph = Graph::new(vec![
        metric("CPU time", "nanoseconds"),
        metric("Samples", "count"),
    ]);
    let mut frames = Frames {
        resolver,
        maps: HashMap::new(),
    };
    // The `(root)` node is the tree's root; other parentless nodes hang below it.
    let mut stack: Vec<(usize, usize)> = Vec::new();
    for (i, n) in p.nodes.iter().enumerate() {
        if has_parent[i] {
            continue;
        }
        if n.call_frame.function_name == "(root)" {
            graph.add_self(ROOT, &[time[i] * 1000, hits[i] as i64]);
            for c in children[i].iter().rev() {
                stack.push((*c, ROOT));
            }
        } else {
            stack.push((i, ROOT));
        }
    }
    while let Some((i, parent)) = stack.pop() {
        let f = frames.func(&mut graph, &p.nodes[i].call_frame);
        let node = graph.child(parent, f);
        graph.add_self(node, &[time[i] * 1000, hits[i] as i64]);
        graph.nodes[node].calls += hits[i];
        for c in children[i].iter().rev() {
            stack.push((*c, node));
        }
    }
    let mut head = Head::new(SnapshotKind::V8Cpu, name);
    head.calls_label = "Samples";
    head.duration_nanos = (p.end_time - p.start_time).max(0) * 1000;
    head.info(
        "Samples",
        p.samples
            .len()
            .max(hits.iter().sum::<u64>() as usize)
            .to_string(),
    );
    if let Some(script) = main_script(&graph) {
        head.title = format!("{name} ({script})");
    }
    Ok(graph.finish(head, metric_index))
}

/// The user script with the most cost (for the title).
fn main_script(graph: &Graph) -> Option<String> {
    graph
        .nodes
        .iter()
        .skip(1)
        .filter_map(|n| graph.funcs.get(n.func))
        .find(|f| f.file.starts_with('/') && !f.file.contains("/node_modules/"))
        .map(|f| f.file.rsplit('/').next().unwrap_or(&f.file).to_string())
}

/// Parse a `.heapprofile` into a snapshot (metrics: allocated bytes, samples).
pub fn heap_profile(
    data: &[u8],
    name: &str,
    metric_index: Option<usize>,
    resolver: Option<MapResolver<'_>>,
) -> Result<Snapshot, String> {
    let mut de = serde_json::Deserializer::from_slice(data);
    de.disable_recursion_limit();
    let p: HeapProfile = serde::Deserialize::deserialize(&mut de)
        .map_err(|e| format!("not a V8 heap profile: {e}"))?;
    let mut counts: HashMap<u64, i64> = HashMap::new();
    for s in &p.samples {
        *counts.entry(s.node_id).or_default() += 1;
    }
    let mut graph = Graph::new(vec![
        metric("Allocated", "bytes"),
        metric("Samples", "count"),
    ]);
    let mut frames = Frames {
        resolver,
        maps: HashMap::new(),
    };
    let head_node = p.head;
    graph.add_self(
        ROOT,
        &[
            head_node.self_size,
            counts.get(&head_node.id).copied().unwrap_or(0),
        ],
    );
    let mut stack: Vec<(HeapNode, usize)> = head_node
        .children
        .into_iter()
        .rev()
        .map(|c| (c, ROOT))
        .collect();
    while let Some((n, parent)) = stack.pop() {
        let f = frames.func(&mut graph, &n.call_frame);
        let node = graph.child(parent, f);
        let samples = counts.get(&n.id).copied().unwrap_or(0);
        graph.add_self(node, &[n.self_size, samples]);
        graph.nodes[node].calls += samples as u64;
        for c in n.children.into_iter().rev() {
            stack.push((c, node));
        }
    }
    let mut head = Head::new(SnapshotKind::V8HeapSampling, name);
    head.calls_label = "Samples";
    head.info("Samples", p.samples.len().to_string());
    if let Some(script) = main_script(&graph) {
        head.title = format!("{name} ({script})");
    }
    Ok(graph.finish(head, metric_index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sourcemap::tests::encode;

    const CPU: &[u8] = include_bytes!("../tests/fixtures/app.cpuprofile");
    const HEAP: &[u8] = include_bytes!("../tests/fixtures/app.heapprofile");

    #[test]
    fn reads_a_node_cpu_profile() {
        let s = cpu_profile(CPU, "app.cpuprofile", None, None).unwrap();
        assert_eq!(s.kind, SnapshotKind::V8Cpu);
        assert_eq!(s.title, "app.cpuprofile (app.js)");
        assert_eq!(s.calls_label, "Samples");
        assert_eq!(s.metrics[0].unit, "nanoseconds");
        let v = s.view.as_ref().unwrap();
        // Every sample's time is in the tree; the profile lasted about as long.
        assert!(v.total > 0);
        assert!(v.total <= v.duration_nanos + 1_000_000);
        let fib = s.functions.iter().find(|f| f.name == "fib").expect("fib");
        assert_eq!(fib.file, "/tmp/forge-prof-fixtures/app.js");
        assert_eq!(fib.line, 1);
        assert!(fib.calls > 0 && fib.own > 0);
        let report = s
            .functions
            .iter()
            .find(|f| f.name == "buildReport")
            .unwrap();
        assert!(report.total >= fib.own, "buildReport calls fib");
        assert!(s.functions.iter().any(|f| f.name == "(program)"));
        // fib is reached through main → buildReport.
        let main = s.functions.iter().position(|f| f.name == "main").unwrap();
        let build = s
            .functions
            .iter()
            .position(|f| f.name == "buildReport")
            .unwrap();
        assert!(s
            .edges
            .iter()
            .any(|e| e.caller == main && e.callee == build));
        // The samples metric counts samples.
        let by_samples = cpu_profile(CPU, "app.cpuprofile", Some(1), None).unwrap();
        assert_eq!(by_samples.metrics[by_samples.metric].kind, "Samples");
        assert!(by_samples.total > 10);
    }

    #[test]
    fn maps_frames_through_source_maps() {
        // app.js line 1 (fib) comes from src/math.ts line 3.
        let mappings = format!("{};", encode(&[0, 0, 2, 0]));
        let json = format!(
            r#"{{"version":3,"sources":["../src/math.ts"],"names":[],"mappings":"{mappings}"}}"#
        );
        let resolve = |script: &str| {
            (script == "/tmp/forge-prof-fixtures/app.js").then(|| {
                SourceMap::parse(&json, "/tmp/forge-prof-fixtures/dist/app.js.map").unwrap()
            })
        };
        let s = cpu_profile(CPU, "app.cpuprofile", None, Some(&resolve)).unwrap();
        let fib = s.functions.iter().find(|f| f.name == "fib").unwrap();
        assert_eq!(
            (fib.file.as_str(), fib.line),
            ("/tmp/forge-prof-fixtures/src/math.ts", 3)
        );
    }

    #[test]
    fn reads_older_profiles_with_parents_and_hit_counts() {
        let json = r#"{"nodes":[
            {"id":1,"callFrame":{"functionName":"(root)","url":"","lineNumber":-1,"columnNumber":-1},"hitCount":0},
            {"id":2,"callFrame":{"functionName":"","url":"file:///p/a%20b.js","lineNumber":0,"columnNumber":0},"hitCount":1,"parent":1},
            {"id":3,"callFrame":{"functionName":"work","url":"file:///p/a%20b.js","lineNumber":4,"columnNumber":2},"hitCount":3,"parent":2}
        ],"startTime":0,"endTime":4000}"#;
        let s = cpu_profile(json.as_bytes(), "old", None, None).unwrap();
        let work = s.functions.iter().find(|f| f.name == "work").unwrap();
        assert_eq!(
            (work.file.as_str(), work.line, work.calls),
            ("/p/a b.js", 5, 3)
        );
        assert_eq!(work.own, 3_000_000);
        assert_eq!(s.total, 4_000_000);
        assert!(s.functions.iter().any(|f| f.name == "(anonymous)"));
        assert!(cpu_profile(b"{\"nodes\":[]}", "x", None, None).is_err());
        assert!(cpu_profile(b"[1,2]", "x", None, None).is_err());
    }

    #[test]
    fn reads_a_sampling_heap_profile() {
        let s = heap_profile(HEAP, "app.heapprofile", None, None).unwrap();
        assert_eq!(s.kind, SnapshotKind::V8HeapSampling);
        assert_eq!(s.metrics[0].unit, "bytes");
        let v = s.view.as_ref().unwrap();
        assert!(v.total > 0);
        let build = s
            .functions
            .iter()
            .find(|f| f.name == "buildReport")
            .expect("buildReport");
        assert_eq!(build.file, "/tmp/forge-prof-fixtures/app.js");
        assert!(build.total > 0);
        // Every byte is under the root.
        assert_eq!(v.tree.total, v.total);
        assert!(heap_profile(b"{}", "x", None, None).is_err());
    }
}
