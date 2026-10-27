---
title: cforge-lang-cpp
description: "CForge: C/C++ language support - build problems of GCC, Clang, MSVC and linkers, compilation databases, GoogleTest and Catch2 discovery, the GoogleTest / Catch2 / CTest providers with gcov and llvm-cov coverage, clangd and clang-format command lines and debugger setup; pure, also built to WebAssembly."
order: 302
tags: [crate, rust, pack, cpp]
---

# cforge-lang-cpp

`packs/cforge/lang-cpp` — CForge: everything about C and C++ that needs no I/O. The desktop
backend and the WebAssembly build of browser mode share it.

## Purpose

- `diagnostics`: compiler and linker messages of a build — GCC and Clang
  (`file:line:col: error: …` with `[-Wflag]` codes; source excerpts and `In function`
  context skipped; notes attached to their message), MSVC (`file(line,col): error C2065: …`,
  `link.exe` `LNK` errors), GNU `ld` (`file.cpp:(.text+0x9): undefined reference …`), `lld`
  and Apple's `ld` — and `project_path`, which makes paths project-relative (relative paths
  resolve against the build folder).
- `compdb`: `compile_commands.json` (`command` strings split like a shell, or `arguments`),
  standards, include folders, a summary and `--query-driver` globs.
- `discovery`: GoogleTest (`TEST`, `TEST_F`, `TEST_P`, `TYPED_TEST`, `TYPED_TEST_P`) and
  Catch2 (`TEST_CASE`, `TEST_CASE_METHOD`, `SCENARIO`, `TEMPLATE_TEST_CASE`, nested `SECTION`,
  `GIVEN` / `WHEN` / `THEN` with Catch2's prefixes, tags) with a tokenizer that skips comments
  and reads string and raw string literals; the included header decides the framework.
- `provider`: `CppTests`, the `forge-testing` providers of `gtest`, `catch2` and `ctest`
  (replacing the manifest's declarations): gutter targets keyed like the test tree,
  command lines per scope (`--gtest_filter` for tests, suites, parameterized and typed tests;
  Catch2 test specs with escaping and `-c` sections; `ctest -R/-L/-j`), JUnit reports, Rerun
  Failed Tests and Run with Coverage (`LLVM_PROFILE_FILE` and a `CoverageCollect` for the
  backend). The executable comes from a `CppContext` (build folder, CTest, executable targets
  with their sources) in `CommandContext.settings["cpp.context"]`; `debug_command` gives the
  debugger a test's program and filter.
- `coverage`: gcov's JSON (`gcov --json-format --stdout`) to LCOV (project files only, counts
  of several objects added, branches and functions), the `llvm-profdata merge` and `llvm-cov
  export -format=lcov` command lines, and the tools that belong to a compiler
  (`g++-14` → `gcov-14`).
- `tools`: clangd's arguments (`--compile-commands-dir`, `--background-index`,
  `--clang-tidy`, `--header-insertion`, `--completion-style`, `--query-driver`), its version,
  clang-format's arguments (`--assume-filename`, `--style=file`, `--fallback-style`) and the
  debug setup with STL renderers (GDB `-iex` commands that register libstdc++'s printers,
  LLDB `initCommands`), with the folders where GCC keeps its printers.

## Modules

| Module | Summary |
| --- | --- |
| `diagnostics.rs` | `parse_build_output`, `project_path`, `BuildProblem`, `Severity`. |
| `compdb.rs` | `parse`, `summary`, `query_drivers`, `entry_for`, `CompileCommand`. |
| `discovery.rs` | `discover`, `included_framework`, `CppTest`, `Discovered`, `Framework`, `Kind`. |
| `provider.rs` | `CppTests`, `register`, `CppContext`, `ContextTarget`, `debug_command`, `gtest_test_filter`, `catch2_escape`, `is_cpp_source`. |
| `coverage.rs` | `gcov_json_to_lcov`, `gcov_command`, `llvm_merge_command`, `llvm_export_command`, `llvm_profile_env`, `tool_next_to`. |
| `tools.rs` | `clangd_args`, `ClangdOptions`, `clangd_version`, `clang_format_args`, `debug_setup`, `printer_dir_candidates`. |

## Used by

- IPC: `app/src-tauri/src/commands/cpp.rs`, `commands/run.rs` (the providers in
  `RunState.tests`, CMake Application command lines), `commands/testing.rs` (coverage).
- Browser mode: `crates/forge-wasm/src/cpp.rs` and `testing.rs`.

## Dependencies

- Workspace crates: [`forge-output`](forge-output.md), [`forge-runtools`](forge-runtools.md),
  [`forge-testing`](forge-testing.md), [`cforge-cmake`](cforge-cmake.md)
- External: `serde`, `serde_json`

## Tests

Unit tests per module with recorded fixtures in `tests/fixtures/`: GCC 13 and Clang 18 errors,
a GNU `ld` error, a failed Ninja build, CTest's `--output-junit` report and console output,
gcov JSON of a test run; documented-format samples of MSVC errors, a GoogleTest XML report
and a Catch2 JUnit report.

```sh
cargo test -p cforge-lang-cpp
```
