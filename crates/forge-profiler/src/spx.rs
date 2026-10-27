//! php-spx full reports: the metadata (`spx-full-<date>-<host>-<pid>-<n>.json` with the
//! command line, host and `enabled_metrics`) and the trace next to it (`<key>.txt.gz`).
//!
//! The trace has an `[events]` section with one line per call start / end
//! (`<function index> <1|0> <metric values…>`, the metrics' cumulative counters at that
//! moment) and a `[functions]` section with one function name per line, so the call tree,
//! the calls and the own / inclusive costs are exact.

use serde::Deserialize;

use crate::model::{metric, Graph, Head, SnapshotKind, ROOT};
use crate::Snapshot;

/// SPX's metrics: key, label, unit.
const METRICS: &[(&str, &str, &str)] = &[
    ("wt", "Wall time", "nanoseconds"),
    ("ct", "CPU time", "nanoseconds"),
    ("it", "Idle time", "nanoseconds"),
    ("zm", "ZE memory usage", "bytes"),
    ("zmac", "ZE alloc count", "count"),
    ("zmab", "ZE alloc bytes", "bytes"),
    ("zmfc", "ZE free count", "count"),
    ("zmfb", "ZE free bytes", "bytes"),
    ("zgr", "ZE GC runs", "count"),
    ("zgb", "ZE GC root buffer", "count"),
    ("zgc", "ZE GC collected", "count"),
    ("zif", "ZE file count", "count"),
    ("zil", "ZE line count", "count"),
    ("zuc", "ZE class count", "count"),
    ("zuf", "ZE func. count", "count"),
    ("zuo", "ZE opcodes count", "count"),
    ("zo", "ZE object count", "count"),
    ("ze", "ZE error count", "count"),
    ("mor", "Own RSS", "bytes"),
    ("io", "I/O Bytes", "bytes"),
    ("ior", "I/O Read Bytes", "bytes"),
    ("iow", "I/O Written Bytes", "bytes"),
];

/// The metadata file (`<key>.json`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Metadata {
    pub key: String,
    pub exec_ts: u64,
    pub host_name: String,
    pub process_pid: i64,
    pub process_pwd: String,
    pub cli: i64,
    pub cli_command_line: String,
    pub http_request_uri: String,
    pub http_method: String,
    pub http_host: String,
    pub wall_time_ms: u64,
    pub peak_memory_usage: u64,
    pub called_function_count: u64,
    pub call_count: u64,
    pub enabled_metrics: Vec<String>,
}

/// Read a metadata file.
pub fn metadata(json: &[u8]) -> Result<Metadata, String> {
    serde_json::from_slice(json).map_err(|e| format!("not an SPX metadata file: {e}"))
}

/// The trace file of a metadata key (`<key>.txt.gz`).
pub fn trace_name(key: &str) -> String {
    format!("{key}.txt.gz")
}

/// Whether `text` is a trace (starts with `[events]`).
pub fn is_trace(text: &str) -> bool {
    text.trim_start().starts_with("[events]")
}

fn known(n: Option<&str>) -> Option<&str> {
    n.filter(|v| !v.is_empty() && *v != "n/a")
}

/// Parse a full report (the trace, plain or gzip, and its metadata when found).
pub fn parse(
    meta: Option<&Metadata>,
    trace: &[u8],
    name: &str,
    metric_index: Option<usize>,
) -> Result<Snapshot, String> {
    let text = crate::text(trace)?;
    if !is_trace(&text) {
        return Err("not an SPX trace (no [events] section)".into());
    }
    let (events, functions) = match text.find("\n[functions]") {
        Some(i) => (&text[..i], &text[i + "\n[functions]".len()..]),
        None => return Err("the SPX trace has no [functions] section (incomplete run?)".into()),
    };
    let names: Vec<&str> = functions
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.is_empty())
        .collect();
    // Metrics: from the metadata, else SPX's default (`wt,zm`) for two columns.
    let columns = events
        .lines()
        .nth(1)
        .map(|l| l.split_ascii_whitespace().count().saturating_sub(2))
        .unwrap_or(0);
    let keys: Vec<String> = match meta {
        Some(m) if !m.enabled_metrics.is_empty() => m.enabled_metrics.clone(),
        _ if columns == 2 => vec!["wt".into(), "zm".into()],
        _ => (0..columns).map(|i| format!("m{}", i + 1)).collect(),
    };
    let metrics: Vec<_> = keys
        .iter()
        .map(|k| match METRICS.iter().find(|(key, _, _)| key == k) {
            Some((_, label, unit)) => metric(label, unit),
            None => metric(k, "count"),
        })
        .collect();
    let m = metrics.len();
    let mut head = Head::new(SnapshotKind::Spx, name);
    if let Some(meta) = meta {
        if let Some(cmd) = known(Some(&meta.cli_command_line)) {
            head.title = cmd.to_string();
            head.info("Command", cmd);
        }
        if let Some(uri) = known(Some(&meta.http_request_uri)) {
            let method = known(Some(&meta.http_method)).unwrap_or("GET");
            head.title = format!("{method} {uri}");
            head.info("Request", format!("{method} {uri}"));
        }
        head.info("Host", &meta.host_name);
        if meta.process_pid > 0 {
            head.info("Process", meta.process_pid.to_string());
        }
        if meta.call_count > 0 {
            head.info("Calls", meta.call_count.to_string());
        }
        if meta.peak_memory_usage > 0 {
            head.info("Peak memory", format!("{} bytes", meta.peak_memory_usage));
        }
    }

    let mut graph = Graph::new(metrics);
    let mut func_ids: Vec<Option<usize>> = vec![None; names.len()];
    // (node, function index, values at the start, inclusive values of finished children)
    let mut stack: Vec<(usize, usize, Vec<f64>, Vec<f64>)> = Vec::new();
    let mut last = vec![0f64; m];
    let mut first: Option<Vec<f64>> = None;
    let mut values = vec![0f64; m];
    for line in events.lines().skip(1) {
        let mut parts = line.split_ascii_whitespace();
        let (Some(idx), Some(start)) = (parts.next(), parts.next()) else {
            continue;
        };
        let Ok(idx) = idx.parse::<usize>() else {
            continue;
        };
        for (i, v) in values.iter_mut().enumerate() {
            *v = parts
                .next()
                .and_then(|t| t.parse::<f64>().ok())
                .unwrap_or(last.get(i).copied().unwrap_or(0.0));
        }
        if first.is_none() {
            first = Some(values.clone());
        }
        last.clone_from(&values);
        if start == "1" {
            let func = match func_ids.get(idx).copied().flatten() {
                Some(f) => f,
                None => {
                    let name = names.get(idx).copied().unwrap_or("(unknown)");
                    let f = graph.func(name, "", 0);
                    if idx < func_ids.len() {
                        func_ids[idx] = Some(f);
                    }
                    f
                }
            };
            let parent = stack.last().map(|s| s.0).unwrap_or(ROOT);
            let node = graph.child(parent, func);
            stack.push((node, idx, values.clone(), vec![0.0; m]));
        } else {
            close(&mut graph, &mut stack, &values);
        }
    }
    // Calls still open when the trace ends (exit() inside a function).
    while !stack.is_empty() {
        let end = last.clone();
        close(&mut graph, &mut stack, &end);
    }
    if graph.funcs.is_empty() {
        return Err("the SPX trace contains no calls".into());
    }
    let wall = keys.iter().position(|k| k == "wt");
    if let (Some(w), Some(first)) = (wall, &first) {
        head.duration_nanos = (last[w] - first[w]).max(0.0) as i64;
    }
    Ok(graph.finish(head, metric_index))
}

fn close(graph: &mut Graph, stack: &mut Vec<(usize, usize, Vec<f64>, Vec<f64>)>, end: &[f64]) {
    let Some((node, _, start, children)) = stack.pop() else {
        return;
    };
    let incl: Vec<f64> = end.iter().zip(&start).map(|(e, s)| e - s).collect();
    let own: Vec<i64> = incl
        .iter()
        .zip(&children)
        .map(|(i, c)| (i - c).round() as i64)
        .collect();
    graph.add_self(node, &own);
    graph.nodes[node].calls += 1;
    if let Some(parent) = stack.last_mut() {
        for (c, i) in parent.3.iter_mut().zip(&incl) {
            *c += i;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const META: &[u8] =
        include_bytes!("../tests/fixtures/spx-full-20261028_101500-devbox-4242-1804289383.json");
    const TRACE: &[u8] =
        include_bytes!("../tests/fixtures/spx-full-20261028_101500-devbox-4242-1804289383.txt.gz");

    fn func<'a>(s: &'a Snapshot, name: &str) -> (usize, &'a crate::FunctionStats) {
        s.functions
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == name)
            .unwrap_or_else(|| panic!("{name} missing"))
    }

    #[test]
    fn reads_a_full_report() {
        let meta = metadata(META).unwrap();
        assert_eq!(
            trace_name(&meta.key),
            "spx-full-20261028_101500-devbox-4242-1804289383.txt.gz"
        );
        let s = parse(Some(&meta), TRACE, "report", None).unwrap();
        assert_eq!(s.kind, SnapshotKind::Spx);
        assert_eq!(s.title, "php bin/report.php");
        let kinds: Vec<_> = s
            .metrics
            .iter()
            .map(|m| (m.kind.as_str(), m.unit.as_str()))
            .collect();
        assert_eq!(
            kinds,
            [("Wall time", "nanoseconds"), ("ZE memory usage", "bytes")]
        );
        assert_eq!(s.total, 8_100);
        assert_eq!(s.view.as_ref().unwrap().duration_nanos, 8_100);

        let (script_i, script) = func(&s, "/home/anna/shop/bin/report.php");
        assert_eq!((script.calls, script.own, script.total), (1, 300, 8_100));
        let (render_i, render) = func(&s, "Report::render");
        assert_eq!((render.own, render.total), (1_600, 6_900));
        let (_, row) = func(&s, "Report::row");
        assert_eq!((row.calls, row.own, row.total), (2, 4_200, 5_300));
        let (_, repeat) = func(&s, "str_repeat");
        assert_eq!((repeat.calls, repeat.own), (2, 1_100));
        // fib recursion: the inner call's time is not counted twice.
        let (fib_i, fib) = func(&s, "fib");
        assert_eq!((fib.calls, fib.own, fib.total), (2, 900, 900));
        assert!(s
            .edges
            .iter()
            .any(|e| e.caller == fib_i && e.callee == fib_i && e.calls == 1));
        assert!(s
            .edges
            .iter()
            .any(|e| e.caller == script_i && e.callee == render_i && e.total == 6_900));

        // Exact call tree.
        let v = s.view.as_ref().unwrap();
        let top = &v.tree.children[0];
        assert_eq!(top.name, "/home/anna/shop/bin/report.php");
        assert_eq!(top.total, 8_100);
        assert_eq!(top.children[0].name, "Report::render");
        assert_eq!(top.children[0].children[0].total, 5_300);

        // The memory metric.
        let mem = parse(Some(&meta), TRACE, "report", Some(1)).unwrap();
        assert_eq!(mem.total, 13_000);
        assert_eq!(func(&mem, "Report::row").1.own, 3_072 + 1_536);
    }

    #[test]
    fn works_without_metadata_and_rejects_other_files() {
        let s = parse(None, TRACE, "trace", None).unwrap();
        assert_eq!(s.metrics[0].kind, "Wall time");
        assert_eq!(s.title, "trace");
        assert!(parse(None, b"[events]\n0 1 0 0\n", "x", None).is_err());
        assert!(parse(None, b"{}", "x", None).is_err());
    }
}
