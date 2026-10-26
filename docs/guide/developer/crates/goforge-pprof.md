---
title: goforge-pprof
description: "GoForge: pprof profiles (profile.proto, gzip-compressed protobuf) decoded by a small hand-written protobuf reader, with the Profiler's views - top functions, call tree and flame graph; pure, also built to WebAssembly."
order: 299
tags: [crate, rust, pack, go, profiler, pprof]
---

# goforge-pprof

`packs/goforge/pprof` — the data of the Profiler tool window.

## Purpose

- `proto`: the protobuf wire format (varints, length-delimited fields, packed repeated
  numbers, fixed 32/64-bit fields skipped), reading and writing. No protobuf library is
  needed for `profile.proto`'s few messages.
- `profile`: `decode` reads a profile, gzip compressed (`flate2`) or not: sample types,
  samples (location ids, leaf first, and values), locations with their inlined lines,
  functions, the string table, period and duration; frames are interned.
- `view`: `view(profile, sample_index, options)` for one sample type — the top list (flat and
  cumulative per function, sorted like `pprof -top`), the top-down call tree merged by
  function, with nodes below `min_fraction` pruned. The UI draws the flame graph from the tree.
- `builder`: `ProfileBuilder` writes profiles (tests) and `demo_profile` is the CPU profile of
  the browser-mode demo.

## Used by

- IPC: `pprof_open` (`app/src-tauri/src/commands/go.rs`).
- Browser mode: `crates/forge-wasm/src/go.rs` (`pprof_view`).
- UI: `app/ui/src/features/go/ProfilerView.tsx`.

## Dependencies

- External: `serde`, `flate2` (MIT / Apache-2.0)

## Tests

`tests/fixtures/` holds a CPU and a memory profile written by `go test -cpuprofile` /
`-memprofile`; the views of a known built profile are checked value by value.

```sh
cargo test -p goforge-pprof
```
