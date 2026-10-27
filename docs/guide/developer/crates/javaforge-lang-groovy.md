---
title: javaforge-lang-groovy
description: "JavaForge: Groovy basics (tolerant source scanner, scripts and main classes, Spock specification discovery and commands for the shared test runner, Groovy script command lines with groovy or through Gradle, Groovy SDK lookup; pure, also built to WebAssembly)."
order: 273
tags: [crate, rust, pack]
---

# javaforge-lang-groovy

`packs/javaforge/lang-groovy` — JavaForge: Groovy basics (tolerant source scanner, scripts and main classes, Spock specification discovery and commands for the shared test runner, Groovy script command lines with groovy or through Gradle, Groovy SDK lookup; pure, also built to WebAssembly).

## Purpose

The Groovy plugin's logic (`forge.groovy`). Pure Rust (no file system or processes). See
[Groovy and Scala](../../user/groovy-scala.md).

- `source` tokenizes Groovy (comments, `'…'`, `"…"`, triple-quoted strings with their text,
  shebang lines) with brace depths and scans package, imports, classes / interfaces / traits /
  enums with binary names and supertypes, methods (also string-named Spock features, with the
  Spock block labels of their bodies, annotations and `static`) and whether the file has
  top-level statements (a script). Half-typed code and slashy strings are tolerated.
- `spock` finds specifications (`extends Specification`, `spock.lang.*`, or a base spec in a file
  importing Spock) and their features (string names or methods with blocks; fixtures excluded;
  `@Ignore` / `@PendingFeature`, data-driven `where:`).
- `tests` is the shared test runner's provider `SpockTests` (type `spock`): gutter targets,
  Rerun Failed Tests (`Class#feature`) and commands through JavaForge's
  `javaforge_lang_java::tests::build_command` with `language = groovy` (Gradle `--tests`,
  Surefire `-Dtest=`, `src/test/groovy` location rules, JaCoCo).
- `launch` lists what runs in a file (`runnable`: script, `main` methods), builds Groovy Script
  command lines (`groovy` with `JAVA_OPTS`, or Gradle with the `GRADLE_INIT_SCRIPT` task
  `forgeRunGroovy` running `groovy.ui.GroovyMain` on the main source set), finds Groovy SDK
  candidates (GROOVY_HOME, SDKMAN!) and parses `groovy --version`.

## Modules

| Module | Summary |
| --- | --- |
| `source.rs` | `parse(text) -> GroovyFile { package, imports, types, script }`, `TypeDecl`, `Method`, `SPOCK_LABELS`. |
| `spock.rs` | `specs`, `specs_of`, `Spec`, `Feature`. |
| `tests.rs` | `SpockTests` (`TestFrameworkProvider`), `SPOCK`. |
| `launch.rs` | `runnable`, `Runnable`, `MainMethod`, `Runner`, `ScriptRun`, `script_command`, `GRADLE_INIT_SCRIPT`, `gradle_applies_groovy`, `groovy_candidates`, `launcher_in`, `parse_version`. |

## Main public items

```rust
pub use launch::{runnable, Runnable};
pub use spock::{specs, Spec};
pub use tests::SpockTests;
```

## Used by

- The app: `RunState` registers `SpockTests` (`commands/run.rs`); `commands/groovy.rs`
  (`groovy_status`, `groovy_runnable`, Groovy Script command lines) and `commands/java.rs`
  (Spock runs are prepared like JUnit). [`forge-wasm`](forge-wasm.md): `groovy_runnable`,
  `groovy_script_command` and the test registry of the browser mode.

## Dependencies

- Workspace crates: [`forge-output`](forge-output.md), [`forge-runtools`](forge-runtools.md), [`forge-testing`](forge-testing.md), [`javaforge-lang-java`](javaforge-lang-java.md)
- External: `serde`, `serde_json`

## Tests

5 source files, about 1300 lines; 9 unit tests on fixtures (`tests/fixtures`: a Spock
specification, a release script; no network).
