//! Snapshots of the browser-mode demo projects (the Laravel shop and the Node.js orders
//! API): the files a profiler would write, generated in their real formats and read by
//! the same parsers, so browser mode shows what the desktop app shows.

use std::collections::HashMap;
use std::fmt::Write;

use serde_json::{json, Value};

use crate::{heapsnapshot, spx, v8, Snapshot, SnapshotKind};

/// A call in a demo call tree: function, file (relative to the project unless absolute
/// or `php:internal`), line, own time (µs), own memory (bytes), repetitions, callees.
struct Call {
    name: &'static str,
    file: &'static str,
    line: u32,
    own_us: u64,
    mem: i64,
    times: u32,
    children: Vec<Call>,
}

fn c(
    name: &'static str,
    file: &'static str,
    line: u32,
    own_us: u64,
    mem: i64,
    times: u32,
    children: Vec<Call>,
) -> Call {
    Call {
        name,
        file,
        line,
        own_us,
        mem,
        times,
        children,
    }
}

const KERNEL: &str = "vendor/laravel/framework/src/Illuminate/Foundation/Http/Kernel.php";
const BUILDER: &str = "vendor/laravel/framework/src/Illuminate/Database/Eloquent/Builder.php";
const INTERNAL: &str = "php:internal";

/// `GET /orders/42` of the Laravel demo shop.
fn php_request() -> Call {
    let total = c(
        "App\\Services\\PriceCalculator->total",
        "app/Services/PriceCalculator.php",
        41,
        1_150,
        2_048,
        3,
        vec![
            c(
                "App\\Services\\PriceCalculator->loyaltyDiscount",
                "app/Services/PriceCalculator.php",
                73,
                380,
                512,
                1,
                vec![],
            ),
            c(
                "App\\Support\\Money->__construct",
                "app/Support/Money.php",
                7,
                45,
                96,
                4,
                vec![],
            ),
            c("php::round", INTERNAL, 0, 18, 0, 4, vec![]),
        ],
    );
    c(
        "{main}",
        "public/index.php",
        1,
        120,
        4_096,
        1,
        vec![
            c(
                "require_once::/vendor/autoload.php",
                "vendor/autoload.php",
                1,
                2_100,
                310_000,
                1,
                vec![c(
                    "Composer\\Autoload\\ClassLoader->loadClass",
                    "vendor/composer/ClassLoader.php",
                    427,
                    160,
                    8_192,
                    6,
                    vec![],
                )],
            ),
            c(
                "Illuminate\\Foundation\\Http\\Kernel->handle",
                KERNEL,
                140,
                900,
                64_000,
                1,
                vec![
                    c(
                        "App\\Http\\Controllers\\OrderController->show",
                        "app/Http/Controllers/OrderController.php",
                        18,
                        210,
                        1_024,
                        1,
                        vec![
                            c(
                                "Illuminate\\Database\\Eloquent\\Builder->first",
                                BUILDER,
                                300,
                                480,
                                12_288,
                                1,
                                vec![c(
                                    "php::PDOStatement->execute",
                                    INTERNAL,
                                    0,
                                    6_500,
                                    2_048,
                                    1,
                                    vec![],
                                )],
                            ),
                            c(
                                "App\\Services\\InvoiceService->settle",
                                "app/Services/InvoiceService.php",
                                18,
                                320,
                                4_096,
                                1,
                                vec![
                                    total,
                                    c(
                                        "App\\Services\\PriceCalculator->format",
                                        "app/Services/PriceCalculator.php",
                                        83,
                                        290,
                                        256,
                                        2,
                                        vec![c(
                                            "php::number_format",
                                            INTERNAL,
                                            0,
                                            22,
                                            64,
                                            1,
                                            vec![],
                                        )],
                                    ),
                                ],
                            ),
                            c(
                                "App\\Reports\\SalesReport->topCustomers",
                                "app/Reports/SalesReport.php",
                                21,
                                410,
                                16_384,
                                1,
                                vec![
                                    c(
                                        "php::PDOStatement->execute",
                                        INTERNAL,
                                        0,
                                        9_100,
                                        4_096,
                                        1,
                                        vec![],
                                    ),
                                    c(
                                        "php::array_map",
                                        INTERNAL,
                                        0,
                                        60,
                                        8_192,
                                        1,
                                        vec![c(
                                            "{closure:app/Reports/SalesReport.php:25-27}",
                                            "app/Reports/SalesReport.php",
                                            25,
                                            14,
                                            128,
                                            30,
                                            vec![],
                                        )],
                                    ),
                                ],
                            ),
                        ],
                    ),
                    c(
                        "Illuminate\\Foundation\\Http\\Kernel->terminate",
                        KERNEL,
                        210,
                        300,
                        0,
                        1,
                        vec![],
                    ),
                ],
            ),
        ],
    )
}

fn path(root: &str, file: &str) -> String {
    if file == INTERNAL || file.starts_with('/') || file.is_empty() {
        file.to_string()
    } else {
        format!("{}/{}", root.trim_end_matches('/'), file)
    }
}

/// The Xdebug profiler file of the demo request (one `fn=` block per call, as Xdebug
/// writes it).
pub fn cachegrind_text(root: &str) -> String {
    struct Cg<'a> {
        root: &'a str,
        out: String,
        files: HashMap<String, usize>,
        fns: HashMap<String, usize>,
    }
    fn reference(map: &mut HashMap<String, usize>, value: &str) -> String {
        match map.get(value) {
            Some(id) => format!("({id})"),
            None => {
                let id = map.len() + 1;
                map.insert(value.to_string(), id);
                format!("({id}) {value}")
            }
        }
    }
    impl Cg<'_> {
        fn emit(&mut self, call: &Call) -> (u64, i64) {
            let mut costs = Vec::new();
            for ch in &call.children {
                for _ in 0..ch.times {
                    costs.push((ch, self.emit(ch)));
                }
            }
            let own = call.own_us * 100;
            let file = path(self.root, call.file);
            let name = call
                .name
                .replace("/vendor/", &format!("{}/vendor/", self.root));
            let fl = reference(&mut self.files, &file);
            let fname = reference(&mut self.fns, &name);
            let _ = write!(
                self.out,
                "fl={fl}\nfn={fname}\n{} {own} {}\n",
                call.line, call.mem
            );
            let (mut t, mut m) = (own, call.mem);
            for (i, (ch, (ct, cm))) in costs.iter().enumerate() {
                let cf = path(self.root, ch.file);
                let cname = ch
                    .name
                    .replace("/vendor/", &format!("{}/vendor/", self.root));
                let cfl = reference(&mut self.files, &cf);
                let cfn = reference(&mut self.fns, &cname);
                let _ = write!(
                    self.out,
                    "cfl={cfl}\ncfn={cfn}\ncalls=1 0 0\n{} {ct} {cm}\n",
                    call.line + 1 + i as u32
                );
                t += ct;
                m += cm;
            }
            self.out.push('\n');
            (t, m)
        }
    }
    let mut cg = Cg {
        root,
        out: String::new(),
        files: HashMap::new(),
        fns: HashMap::new(),
    };
    let (t, m) = cg.emit(&php_request());
    format!(
        "version: 1\ncreator: xdebug 3.3.2 (PHP 8.3.6)\ncmd: {}\npart: 1\npositions: line\n\nevents: Time_(10ns) Memory_(bytes)\n\n{}summary: {t} {m}\n",
        path(root, "public/index.php"),
        cg.out
    )
}

/// The SPX report of `php artisan shop:report` (metadata, trace).
pub fn spx_files(root: &str) -> (String, Vec<u8>) {
    fn walk(
        call: &Call,
        names: &mut Vec<String>,
        index: &mut HashMap<String, usize>,
        clock: &mut (u64, i64),
        out: &mut String,
    ) {
        let name = call.name.replace("->", "::").replace("php::", "");
        let i = *index.entry(name.clone()).or_insert_with(|| {
            names.push(name);
            names.len() - 1
        });
        let _ = writeln!(out, "{i} 1 {:.4} {:.4}", clock.0 as f64, clock.1 as f64);
        clock.0 += call.own_us * 1000 / 2;
        clock.1 += call.mem / 2;
        for ch in &call.children {
            for _ in 0..ch.times {
                walk(ch, names, index, clock, out);
            }
        }
        clock.0 += call.own_us * 1000 - call.own_us * 1000 / 2;
        clock.1 += call.mem - call.mem / 2;
        let _ = writeln!(out, "{i} 0 {:.4} {:.4}", clock.0 as f64, clock.1 as f64);
    }
    let mut tree = php_request();
    tree.name = "artisan";
    tree.children.remove(1);
    let mut kernel = php_request().children.remove(1);
    kernel.name = "Illuminate\\Foundation\\Console\\Kernel->handle";
    tree.children.push(kernel);
    let mut names = Vec::new();
    let mut events = String::from("[events]\n");
    walk(
        &tree,
        &mut names,
        &mut HashMap::new(),
        &mut (0, 2_097_152),
        &mut events,
    );
    for n in &mut names {
        if n == "artisan" {
            *n = path(root, "artisan");
        }
    }
    let trace = format!("{events}[functions]\n{}\n", names.join("\n"));
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let _ = std::io::Write::write_all(&mut gz, trace.as_bytes());
    let meta = json!({
        "key": "spx-full-20261028_101500-acme-4242-1804289383",
        "exec_ts": 1761646500,
        "host_name": "acme",
        "process_pid": 4242,
        "process_pwd": root,
        "cli": 1,
        "cli_command_line": "php artisan shop:report",
        "http_request_uri": "n/a",
        "http_method": "n/a",
        "http_host": "n/a",
        "wall_time_ms": 24,
        "peak_memory_usage": 6_291_456,
        "called_function_count": names.len(),
        "call_count": 64,
        "enabled_metrics": ["wt", "zm"],
    });
    (meta.to_string(), gz.finish().unwrap_or_default())
}

/// A V8 call frame JSON value of the Node.js demo (`src/…` files of the orders API).
fn frame(root: &str, name: &str, file: &str, line: i64) -> Value {
    let url = if file.is_empty() || file.starts_with("node:") {
        file.to_string()
    } else {
        format!("file://{}", path(root, file))
    };
    json!({ "functionName": name, "scriptId": "0", "url": url, "lineNumber": line - 1, "columnNumber": 0 })
}

/// (name, file, line, own samples / bytes, children)
type Node = (&'static str, &'static str, i64, u64, Vec<NodeSpec>);
struct NodeSpec(Node);

fn n(
    name: &'static str,
    file: &'static str,
    line: i64,
    own: u64,
    children: Vec<NodeSpec>,
) -> NodeSpec {
    NodeSpec((name, file, line, own, children))
}

fn node_tree() -> NodeSpec {
    n(
        "(root)",
        "",
        0,
        0,
        vec![
            n("(program)", "", 0, 31, vec![]),
            n("(garbage collector)", "", 0, 22, vec![]),
            n(
                "",
                "node:internal/main/run_main_module",
                1,
                0,
                vec![n(
                    "executeUserEntryPoint",
                    "node:internal/modules/run_main",
                    157,
                    2,
                    vec![n(
                        "",
                        "src/server.ts",
                        1,
                        4,
                        vec![n(
                            "seed",
                            "src/orders/store.ts",
                            12,
                            18,
                            vec![n(
                                "structuredClone",
                                "node:internal/structured_clone",
                                1,
                                9,
                                vec![],
                            )],
                        )],
                    )],
                )],
            ),
            n(
                "processTicksAndRejections",
                "node:internal/process/task_queues",
                72,
                3,
                vec![n(
                    "handle",
                    "src/server.ts",
                    6,
                    12,
                    vec![
                        n(
                            "findOrder",
                            "src/orders/store.ts",
                            3,
                            26,
                            vec![n("filter", "", 0, 14, vec![])],
                        ),
                        n(
                            "orderTotal",
                            "src/orders/pricing.ts",
                            9,
                            21,
                            vec![
                                n(
                                    "lineTotal",
                                    "src/orders/pricing.ts",
                                    5,
                                    64,
                                    vec![n("round", "src/orders/pricing.ts", 18, 23, vec![])],
                                ),
                                n("applyVoucher", "src/orders/vouchers.ts", 3, 17, vec![]),
                            ],
                        ),
                        n("stringify", "", 0, 38, vec![]),
                        n("writeHead", "node:_http_server", 352, 7, vec![]),
                    ],
                )],
            ),
        ],
    )
}

/// The `.cpuprofile` of a demo run of the orders API.
pub fn cpuprofile_text(root: &str) -> String {
    let mut nodes = Vec::new();
    fn add(root: &str, spec: &NodeSpec, nodes: &mut Vec<Value>) -> u64 {
        let id = nodes.len() as u64 + 1;
        nodes.push(Value::Null);
        let (name, file, line, own, children) = &spec.0;
        let ids: Vec<u64> = children.iter().map(|c| add(root, c, nodes)).collect();
        nodes[id as usize - 1] = json!({
            "id": id,
            "callFrame": frame(root, name, file, if file.is_empty() { 0 } else { *line }),
            "hitCount": own,
            "children": ids,
        });
        id
    }
    add(root, &node_tree(), &mut nodes);
    let hits: u64 = nodes
        .iter()
        .map(|n| n["hitCount"].as_u64().unwrap_or(0))
        .sum();
    json!({ "nodes": nodes, "startTime": 0, "endTime": hits * 1000, "samples": [], "timeDeltas": [] })
        .to_string()
}

/// The `.heapprofile` of a demo run (bytes allocated per call path).
pub fn heapprofile_text(root: &str) -> String {
    fn add(root: &str, spec: &NodeSpec, id: &mut u64) -> Value {
        *id += 1;
        let (name, file, line, own, children) = &spec.0;
        let me = *id;
        let kids: Vec<Value> = children.iter().map(|c| add(root, c, id)).collect();
        json!({
            "callFrame": frame(root, name, file, if file.is_empty() { 0 } else { *line }),
            "selfSize": own * 4_096,
            "id": me,
            "children": kids,
        })
    }
    let head = add(root, &node_tree(), &mut 0);
    json!({ "head": head, "samples": [] }).to_string()
}

/// A heap snapshot of the orders API: a store with orders, line items and strings, the
/// HTTP server, closures, compiled code and system objects.
pub fn heapsnapshot_text() -> String {
    /// type, name, id, size, edges (type, name or index, target)
    type Node = (u32, u32, u32, u32, Vec<(u32, u32, usize)>);
    struct B {
        nodes: Vec<Node>,
        strings: Vec<String>,
        index: HashMap<String, u32>,
    }
    const T: &[&str] = &[
        "hidden",
        "array",
        "string",
        "object",
        "code",
        "closure",
        "regexp",
        "number",
        "native",
        "synthetic",
        "concatenated string",
        "sliced string",
        "symbol",
        "bigint",
        "object shape",
    ];
    const E: &[&str] = &[
        "context", "element", "property", "internal", "hidden", "shortcut", "weak",
    ];
    impl B {
        fn s(&mut self, v: &str) -> u32 {
            if let Some(i) = self.index.get(v) {
                return *i;
            }
            self.strings.push(v.to_string());
            let i = self.strings.len() as u32 - 1;
            self.index.insert(v.to_string(), i);
            i
        }
        fn node(&mut self, kind: &str, name: &str, size: u32) -> usize {
            let t = T.iter().position(|x| *x == kind).unwrap_or(0) as u32;
            let name = self.s(name);
            let id = self.nodes.len() as u32 * 2 + 1;
            self.nodes.push((t, name, id, size, Vec::new()));
            self.nodes.len() - 1
        }
        fn edge(&mut self, from: usize, kind: &str, name: &str, to: usize) {
            let t = E.iter().position(|x| *x == kind).unwrap_or(2) as u32;
            let n = if kind == "element" {
                name.parse().unwrap_or(0)
            } else {
                self.s(name)
            };
            self.nodes[from].4.push((t, n, to));
        }
    }
    let mut b = B {
        nodes: Vec::new(),
        strings: Vec::new(),
        index: HashMap::new(),
    };
    b.s("");
    let root = b.node("synthetic", "", 0);
    let gc = b.node("synthetic", "(GC roots)", 0);
    b.edge(root, "element", "1", gc);
    let global = b.node("object", "global", 864);
    b.edge(gc, "element", "1", global);
    let store = b.node("object", "OrderStore", 48);
    b.edge(global, "property", "store", store);
    let map = b.node("object", "Map", 32);
    b.edge(store, "property", "orders", map);
    let table = b.node("array", "", 98_336);
    b.edge(map, "internal", "table", table);
    let shape = b.node("object shape", "system / Map", 120);
    b.edge(store, "internal", "map", shape);
    for i in 0..400u32 {
        let order = b.node("object", "Order", 72);
        b.edge(table, "element", &i.to_string(), order);
        let number = b.node("string", &format!("ORD-2026-{:05}", 1000 + i), 32);
        b.edge(order, "property", "number", number);
        let items = b.node("array", "", 16 + 8 * (1 + i % 4));
        b.edge(order, "property", "items", items);
        for k in 0..(1 + i % 4) {
            let item = b.node("object", "LineItem", 56);
            b.edge(items, "element", &k.to_string(), item);
            let sku = b.node(
                "string",
                ["KB-1", "USB-C", "MON-27", "HUB-4"][k as usize],
                24,
            );
            b.edge(item, "property", "sku", sku);
        }
        if i % 25 == 0 {
            let note = b.node(
                "concatenated string",
                &format!("Customer note for order {i}: please deliver to the back door"),
                8_192,
            );
            b.edge(order, "property", "note", note);
        }
    }
    let server = b.node("object", "Server", 312);
    b.edge(global, "property", "server", server);
    let handle = b.node("closure", "handle", 64);
    b.edge(server, "property", "_events", handle);
    let code = b.node("code", "handle", 4_608);
    b.edge(handle, "internal", "code", code);
    let ctx = b.node("object", "system / Context", 96);
    b.edge(handle, "context", "context", ctx);
    b.edge(ctx, "context", "store", store);
    let sockets = b.node("array", "", 1_048);
    b.edge(server, "property", "_connections", sockets);
    for i in 0..12 {
        let s = b.node("object", "Socket", 1_284);
        b.edge(sockets, "element", &i.to_string(), s);
        let buf = b.node("native", "Buffer", 16_384);
        b.edge(s, "property", "buffer", buf);
    }
    for i in 0..60 {
        let f = b.node(
            "closure",
            ["lineTotal", "orderTotal", "applyVoucher", "findOrder"][i % 4],
            64,
        );
        b.edge(global, "property", &format!("fn{i}"), f);
        let code = b.node("code", "(code)", 1_200 + (i as u32 % 7) * 300);
        b.edge(f, "internal", "code", code);
    }
    let hidden = b.node("hidden", "system / PropertyCell", 30_000);
    b.edge(gc, "element", "2", hidden);
    let leaked = b.node("object", "Order", 72);
    b.edge(global, "weak", "cache", leaked);
    let meta = json!({
        "node_fields": ["type", "name", "id", "self_size", "edge_count", "trace_node_id", "detachedness"],
        "node_types": [T, "string", "number", "number", "number", "number", "number"],
        "edge_fields": ["type", "name_or_index", "to_node"],
        "edge_types": [E, "string_or_number", "node"],
    });
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for (t, name, id, size, es) in &b.nodes {
        nodes.extend([*t, *name, *id, *size, es.len() as u32, 0, 0]);
        for (et, en, to) in es {
            edges.extend([*et, *en, *to as u32 * 7]);
        }
    }
    json!({
        "snapshot": { "meta": meta, "node_count": b.nodes.len(), "edge_count": edges.len() / 3 },
        "nodes": nodes,
        "edges": edges,
        "strings": b.strings,
    })
    .to_string()
}

/// A demo snapshot of `kind` for the project at `root`.
pub fn snapshot(kind: SnapshotKind, root: &str, metric: Option<usize>) -> Snapshot {
    let result = match kind {
        SnapshotKind::Xdebug => crate::cachegrind::parse(
            cachegrind_text(root).as_bytes(),
            "cachegrind.out.4242",
            metric,
        ),
        SnapshotKind::Spx => {
            let (meta, trace) = spx_files(root);
            spx::metadata(meta.as_bytes())
                .and_then(|m| spx::parse(Some(&m), &trace, &m.key, metric))
        }
        SnapshotKind::V8Cpu => v8::cpu_profile(
            cpuprofile_text(root).as_bytes(),
            "CPU.20261028.101500.4242.0.001.cpuprofile",
            metric,
            None,
        ),
        SnapshotKind::V8HeapSampling => v8::heap_profile(
            heapprofile_text(root).as_bytes(),
            "Heap.20261028.101500.4242.0.001.heapprofile",
            metric,
            None,
        ),
        SnapshotKind::V8Heap => heapsnapshot::parse(
            heapsnapshot_text().as_bytes(),
            "Heap.20261028.101500.4242.heapsnapshot",
        ),
    };
    result.expect("the demo snapshots parse")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_demo_snapshot_parses() {
        let root = "~/Projects/acme-shop";
        let x = snapshot(SnapshotKind::Xdebug, root, None);
        let execute = x
            .functions
            .iter()
            .find(|f| f.name == "php::PDOStatement->execute")
            .unwrap();
        assert_eq!(execute.calls, 2);
        assert_eq!(execute.own, (6_500 + 9_100) * 1000);
        let total = x
            .functions
            .iter()
            .find(|f| f.name.ends_with("PriceCalculator->total"))
            .unwrap();
        assert_eq!(total.calls, 3);
        assert_eq!(
            total.file,
            "~/Projects/acme-shop/app/Services/PriceCalculator.php"
        );
        let main = x.functions.iter().find(|f| f.name == "{main}").unwrap();
        assert_eq!(main.total, x.total);
        let spx = snapshot(SnapshotKind::Spx, root, None);
        assert_eq!(spx.title, "php artisan shop:report");
        assert!(spx
            .functions
            .iter()
            .any(|f| f.name == "PDOStatement::execute"));
        let cpu = snapshot(SnapshotKind::V8Cpu, "~/Projects/orders-api", None);
        let line = cpu
            .functions
            .iter()
            .find(|f| f.name == "lineTotal")
            .unwrap();
        assert_eq!(line.file, "~/Projects/orders-api/src/orders/pricing.ts");
        assert_eq!(line.own, 64 * 1_000_000);
        let heap = snapshot(SnapshotKind::V8HeapSampling, "~/Projects/orders-api", None);
        assert!(heap.total > 0);
        let snap = snapshot(SnapshotKind::V8Heap, "", None);
        let h = snap.heap.unwrap();
        let order = h.classes.iter().find(|c| c.name == "Order").unwrap();
        assert_eq!(order.count, 400);
        assert!(h.unreachable_size >= 72);
    }
}
