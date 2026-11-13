---
title: javaforge-lang-scala
description: "JavaForge: Scala basics (tolerant source scanner, main objects, sbt builds read from build.sbt and from sbt's output, sbt command lines, ScalaTest and MUnit discovery and commands for the shared test runner, the Metals launch and its Coursier install; pure, also built to WebAssembly)."
order: 274
tags: [crate, rust, pack]
---

# javaforge-lang-scala

`packs/javaforge/lang-scala` — JavaForge: Scala basics (tolerant source scanner, main objects, sbt builds read from build.sbt and from sbt's output, sbt command lines, ScalaTest and MUnit discovery and commands for the shared test runner, the Metals launch and its Coursier install; pure, also built to WebAssembly).

## Purpose

The Scala plugin's logic (`forge.scala`). Pure Rust (no file system or processes); code
intelligence comes from Metals through `forge-lsp`. See [Groovy and Scala](../../user/groovy-scala.md).

- `source` tokenizes Scala (nested comments, strings, triple-quoted strings, character
  literals, backtick identifiers) and scans packages (also chained clauses), imports, objects /
  classes / traits / enums with fully qualified names and parents, `def main(args:
  Array[String])`, `extends App` and Scala 3 `@main` methods (`main_objects`), for braces and
  Scala 3's indentation syntax.
- `suites` finds ScalaTest suites (FunSuite, FlatSpec with `it should`, WordSpec clauses,
  FunSpec `describe` / `it`, FreeSpec `-`, PropSpec, async styles) and MUnit suites
  (`munit.FunSuite`, `CatsEffectSuite`, `ScalaCheckSuite`) with the full test names the
  frameworks report (`A Cart when empty should have no items`).
- `tests` is the shared test runner's providers `ScalaTests::scalatest()` / `munit()` (types
  `scalatest`, `munit`): gutter targets, Rerun Failed Tests (suites) and `sbt testOnly`
  commands (`-z` for ScalaTest test names, a glob for MUnit) reading the JUnit XML of
  `target/test-reports`.
- `sbt` reads builds statically (`static_build`: projects, folders, `dependsOn`, `aggregate`,
  names, Scala versions, `libraryDependencies` with `%%` / configurations, string `val`s and
  setting sequences, `project/build.properties`, `project/plugins.sbt`), parses an import's
  output (`parse_sync` of `projects` and `show scalaVersion baseDirectory
  libraryDependencies`), merges both and builds command lines (`task_command`, `scoped_task`:
  `core/test`, `~core/compile`, `-J` VM options).
- `metals` holds the pinned Metals version, the Coursier `bootstrap` arguments, the
  initialization options and workspace settings.

## Modules

| Module | Summary |
| --- | --- |
| `source.rs` | `parse(text) -> ScalaFile`, `Template`, `main_objects`, `MainObject`. |
| `suites.rs` | `suites`, `suites_of`, `Suite`, `SuiteTest`, `Framework`. |
| `tests.rs` | `ScalaTests` (`TestFrameworkProvider`), `SCALATEST`, `MUNIT`, `project_dir_of`. |
| `sbt.rs` | `SbtBuild`, `SbtProject`, `SbtDependency`, `TASKS`, `static_build`, `parse_build_properties`, `parse_sync`, `merge`, `SYNC_COMMANDS`, `sync_args`, `task_command`, `scoped_task`, `split_tasks`. |
| `metals.rs` | `METALS_VERSION`, `SERVER_ID`, `coordinates`, `bootstrap_args`, `initialization_options`, `settings`, `version_of_install`. |

## Main public items

```rust
pub use sbt::{SbtBuild, SbtProject};
pub use source::{main_objects, MainObject};
pub use tests::ScalaTests;
```

## Used by

- The app: `RunState` registers both `ScalaTests` (`commands/run.rs`); `commands/scala.rs`
  (Metals registration and install, the sbt model and import, sbt Task command lines, test
  preparation). [`forge-wasm`](forge-wasm.md): `scala_main_objects`, `scala_sbt_build`,
  `scala_command_line` and the test registry of the browser mode.

## Dependencies

- Workspace crates: [`forge-output`](forge-output.md), [`forge-runtools`](forge-runtools.md), [`forge-testing`](forge-testing.md)
- External: `serde`, `serde_json`, `regex-lite`

## Tests

6 source files, about 2100 lines; 11 unit tests on fixtures (`tests/fixtures`: a multi-project
`build.sbt`, `build.properties`, recorded sbt output, ScalaTest and MUnit suites; no network,
no sbt).
