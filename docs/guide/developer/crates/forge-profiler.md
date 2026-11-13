---
title: forge-profiler
description: "Profiler snapshots for the shared Profiler tool window: Xdebug cachegrind files, php-spx full reports, V8 CPU profiles with source maps, V8 sampling heap profiles and heap snapshots (summary by constructor with retained sizes) as execution statistics, callers / callees and goforge-pprof's views; pure, also built to WebAssembly."
order: 195
tags: [crate, rust, core, profiler, xdebug, cachegrind, spx, v8, cpuprofile, heapsnapshot, sourcemap]
---

# forge-profiler

`crates/forge-profiler` — the data of the Profiler's Xdebug, SPX, V8 CPU and V8 Heap sources.
See [Profiling PHP and Node.js](../../user/profiling.md) for the user side.

## Purpose

- `model`: the `Snapshot` every format becomes, for one metric: the metrics (sample types),
  `FunctionStats` (calls, own and total cost; recursion counted once), `CallEdge`s
  (caller → callee with calls and inclusive cost), and `goforge_pprof::ProfileView` for the
  Call Tree and Flame Graph. Parsers fill a `Graph` (functions plus a call tree); formats
  with real stacks derive the statistics from the tree, cachegrind brings its own.
  `Snapshot::map_files` rewrites container paths to local ones.
- `cachegrind`: the callgrind format Xdebug writes (gzip or plain): `fl=`/`fi=`/`fe=`,
  `fn=`, `cfl=`/`cfn=`, `calls=` and the inclusive cost line after it, name compression
  (`(id) name` / `(id)`), relative positions (`+n`, `-n`, `*`), `summary:`/`totals:`, and the
  events `Time_(10ns)`, `Time` (Xdebug 2, µs) and `Memory_(bytes)`. Xdebug writes one block
  per call; blocks of the same function are merged. The call tree is spread top-down from
  the edges (a callee's share is proportional to the edge's cost, recursion skipped,
  nodes below 0.05% folded, at most 64 levels and 50 000 nodes).
- `spx`: php-spx full reports — the metadata (`enabled_metrics`, command line, request) and
  the trace's `[events]` (`<function> <1|0> <cumulative metric values…>`) and `[functions]`
  sections; exact call tree, own cost = inclusive minus the children's.
- `v8`: `.cpuprofile` (nodes with `children` or `parent`, `samples` + `timeDeltas`, or
  `hitCount` × interval for older profiles; metrics CPU time and Samples) and `.heapprofile`
  (nested `head`, `selfSize`, samples; parsed without serde's recursion limit). Call frames
  become functions by name, file (`file://` URLs decoded) and 1-based line, mapped through
  source maps when a `MapResolver` finds one.
- `sourcemap`: source map v3 (Base64 VLQ `mappings`, `sources`, `sourceRoot`, `names`,
  `webpack://` sources), position lookup, and `for_script` to find a script's map
  (`sourceMappingURL` file or inline `data:` URL, else `<script>.map`).
- `heapsnapshot`: `.heapsnapshot` read from a stream straight into `u32` arrays (no JSON
  tree), dominators over the strong edges (Cooper–Harvey–Kennedy on the DFS postorder),
  retained sizes, the Summary by constructor (DevTools' class names: `(string)`, `(array)`,
  `(closure)`, `(system)`, …; same-constructor nesting counted once), the 20 largest objects
  per constructor and the 50 biggest objects; weak-only objects are reported as unreachable.
- `demo`: the browser-mode demo snapshots, generated in the real file formats (cachegrind
  text, SPX report, `.cpuprofile`, `.heapprofile`, `.heapsnapshot`) and read by the parsers.
- `detect` and `open`: the format of a file from its name and first bytes, and one entry
  point (`OpenOptions`: metric, SPX companion file, source maps).

## Used by

- IPC: `profiler_open`, `profiler_recent`, and `RunRequest.profiler` (Profile with Xdebug,
  SPX and V8; the `profiler` run event) in `app/src-tauri/src/commands/profiler.rs`.
- Browser mode: `crates/forge-wasm/src/profiler.rs` (`profiler_demo`, `profiler_parse`) for
  `app/ui/src/mock/profilerMock.ts`.
- UI: `app/ui/src/features/profiling/` (Execution Statistics, callers / callees, the Memory
  view) with the shared views of `app/ui/src/features/profiler/callTree.tsx`.

## Dependencies

- Internal: [goforge-pprof](goforge-pprof.md) (`ProfileView` and its call tree)
- External: `serde`, `serde_json` (`unbounded_depth`), `flate2`, `base64` (MIT / Apache-2.0)

## Tests

`tests/fixtures/` holds small files of every format: an Xdebug 3 cachegrind file with name
compression, two-call merging and recursion, an SPX report (metadata and `.txt.gz` trace),
a `.cpuprofile` and a `.heapprofile` recorded with Node.js 22, and a hand-made
`.heapsnapshot` with Node.js 22's `meta` whose retained sizes are checked object by object.

```sh
cargo test -p forge-profiler
```
