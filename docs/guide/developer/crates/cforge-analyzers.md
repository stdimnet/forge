---
title: cforge-analyzers
description: "CForge: native code analyzers - sanitizer reports (ASan, LSan, TSan, MSan, UBSan) from run output, Valgrind Memcheck XML, perf record / perf script command lines and perf script output as Profiler views; pure, also built to WebAssembly."
order: 303
tags: [crate, rust, pack, cpp, sanitizers, valgrind, perf]
---

# cforge-analyzers

`packs/cforge/analyzers` — CForge's dynamic analysis results, parsed without I/O. The desktop
backend collects run output, reads Valgrind's XML file and runs `perf script`; this crate turns
the text into the models the Run tool window and the Profiler show. The WebAssembly build of
browser mode shares it.

## Purpose

- `sanitizer`: `==pid==ERROR: AddressSanitizer: …`, `WARNING: ThreadSanitizer: …`,
  `WARNING: MemorySanitizer: …`, LeakSanitizer's `detected memory leaks` (one report per
  `Direct leak` / `Indirect leak`) and UndefinedBehaviorSanitizer's `file:line:col: runtime
  error: …` lines. Each `SanitizerReport` has the sanitizer, its kind, the headline, titled
  stacks (access, allocation, free, previous access, thread creation), extra lines and the
  `SUMMARY`. Frames are parsed from ASan's `#n 0xaddr in func file:line:col (module+0x…)`
  and TSan's `#n func file:line (module+0x…)` forms (`(BuildId: …)` dropped).
- `valgrind`: Memcheck's XML (protocol 4, `--xml=yes`): errors with `kind`, `what` /
  `xwhat`, the stacks (`auxwhat` titles the stack after it), `errorcounts`, leak records
  sorted by size and the leak summary added up (definitely / indirectly / possibly lost,
  still reachable). Truncated files (a killed run) are read as far as they go.
- `perf`: `perf record -g -o <file> [-F n] [--call-graph mode] -- program` and
  `perf script -F comm,pid,tid,time,period,event,ip,sym,dso,srcline`; the script output (one
  call stack per sample, leaf first, `srcline` lines) becomes a `goforge_pprof::Profile`
  (sample types `samples` and the event, `cpu` in nanoseconds for `cpu-clock` /
  `task-clock`), so `goforge_pprof::view` computes the method list, call tree and flame graph
  shared with GoForge.
- `frames`: `StackFrame` / `Stack` and `project_path` (absolute paths below the root,
  relative ones in the project or the build folder; toolchain, C library and sanitizer runtime
  files never), `resolve_stacks` (the first project frame is the location).
- `xml`: a small tree on quick-xml, also used for CMSIS-SVD files by
  [cforge-embedded](cforge-embedded.md).
- `valgrind_args`: the Memcheck command line (`--tool=memcheck --xml=yes --xml-file=…`,
  `--leak-check=full` unless the options set it).

## Modules

| Module | Summary |
| --- | --- |
| `sanitizer.rs` | `parse`, `resolve`, `SanitizerReport`, `Sanitizer`. |
| `valgrind.rs` | `parse`, `resolve`, `ValgrindReport`, `ValgrindError`, `LeakSummary`, `Leaked`. |
| `perf.rs` | `record_args`, `script_args`, `RecordOptions`, `parse`, `open`. |
| `frames.rs` | `StackFrame`, `Stack`, `project_path`, `resolve_stacks`. |
| `xml.rs` | `parse`, `Element`. |

## Used by

- IPC: `app/src-tauri/src/commands/cpp_tools.rs` (the sanitizer observer of C/C++ runs,
  `RunRequest.nativeTool`, `cpp_sanitizer_reports`, `cpp_valgrind_open`).
- [cforge-embedded](cforge-embedded.md) (`xml`).
- Browser mode: `crates/forge-wasm/src/cpp_tools.rs`.

## Dependencies

- Internal: [goforge-pprof](goforge-pprof.md)
- External: `serde`, `quick-xml`

## Tests

Unit tests on recorded fixtures in `tests/fixtures/`: AddressSanitizer (heap-buffer-overflow,
heap-use-after-free, leaks), ThreadSanitizer and UndefinedBehaviorSanitizer reports of GCC 13,
a MemorySanitizer report, Memcheck XML of Valgrind 3.22 and `perf script` output.
`tests/smoke.rs` (ignored) compiles a program in a temporary folder and runs the real
Valgrind: `cargo test -p cforge-analyzers --test smoke -- --ignored`.
