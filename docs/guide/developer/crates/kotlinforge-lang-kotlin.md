---
title: kotlinforge-lang-kotlin
description: "KotlinForge: Kotlin language support (structural source scanner, main functions, JUnit 5 and Kotest discovery and commands for the shared test runner, Kotlin/JVM and script command lines, kotlin-lsp launch, ktlint and detekt reports, coroutine dumps; pure, also built to WebAssembly)."
order: 275
tags: [crate, rust, pack]
---

# kotlinforge-lang-kotlin

`packs/kotlinforge/lang-kotlin` — KotlinForge: Kotlin language support (structural source scanner, main functions, JUnit 5 and Kotest discovery and commands for the shared test runner, Kotlin/JVM and script command lines, kotlin-lsp launch, ktlint and detekt reports, coroutine dumps; pure, also built to WebAssembly).

## Purpose

KotlinForge's Kotlin language support. Pure Rust (no file system or processes); code
intelligence itself comes from kotlin-lsp (or kotlin-language-server) through `forge-lsp`. See
[KotlinForge](../../user/kotlin.md).

- `source` tokenizes Kotlin (nested block comments, string templates, raw strings, backtick
  names) and scans package, imports, `@file:` annotations, classes, interfaces, objects,
  companion objects and enums (with JVM binary names such as `com.acme.Cart$Companion`),
  functions with annotations, modifiers, parameters and receivers, `init` blocks and the
  argument ranges of supertype constructors. It tolerates half-typed code.
- `discovery` finds `main` functions (top-level → the file facade `MainKt` or `@file:JvmName`;
  `@JvmStatic` mains in objects and companions) and JUnit 5 / `kotlin.test`, JUnit 4 and TestNG
  tests with backtick names and `@Nested inner class`es.
- `kotest` reads Kotest specs (FunSpec, StringSpec, DescribeSpec, BehaviorSpec) from the
  constructor lambda and `init` blocks into tests and containers with their paths
  (`totals -- applies the discount`).
- `tests` is the shared test runner's provider `KotlinTests` (type `kotest`): gutter targets,
  Rerun Failed Tests (the failed specs) and commands through JavaForge's
  `javaforge_lang_java::tests::build_command` with Kotest's filter variables and
  `JUnitFlavor::Kotlin`; JUnit tests in `.kt` files become `junit` / `testng` configurations
  with `language = kotlin`.
- `launch` builds Kotlin/JVM runs (Gradle with an init script adding `forgeRunKotlin`, Maven
  `exec:java`, `kotlinc` then `java -cp`) and Kotlin scripts (`kotlin` / `kotlinc -script`;
  Gradle build scripts are refused).
- `lsp` describes how kotlin-lsp (`bin/intellij-server --stdio`) and kotlin-language-server
  start: arguments, `JAVA_HOME`, JVM options, initialization options.
- `lint` parses ktlint's JSON and Checkstyle reports and detekt's SARIF and Checkstyle reports
  into `LintProblem`s, and builds their command lines (ktlint on stdin, detekt with the project's
  configuration and baseline).
- `coroutines` holds the debugger expressions of the Coroutines view and parses what they return
  (the `DebugProbes` text dump or the coroutine info list) into `CoroutineDump`.

## Modules

| Module | Summary |
| --- | --- |
| `source.rs` | `tokenize`, `parse(text) -> KotlinFile { package, imports, file_annotations, classes, functions }`, `facade_name`, `skip_balanced`. |
| `discovery.rs` | `main_functions`, `MainFunction`, `discover_tests`, `KotlinTestDiscovery`. |
| `kotest.rs` | `discover_specs`, `KotestSpec`, `KotestTest`, `SpecStyle`. |
| `tests.rs` | `KotlinTests` (`TestFrameworkProvider`), `KOTEST`, `FILTER_SPECS_ENV`, `FILTER_TESTS_ENV`. |
| `launch.rs` | `KotlinApp`, `Runner`, `application_command`, `ScriptRun`, `script_command`, `is_runnable_script`, `GRADLE_INIT_SCRIPT`. |
| `lsp.rs` | `Server`, `KotlinServerSetup`, `launcher_path`. |
| `lint.rs` | `LintProblem`, `parse_ktlint_json`, `parse_sarif`, `parse_checkstyle`, `ktlint_args`, `detekt_args`, `detekt_config_files`, `gradle_plugin`. |
| `coroutines.rs` | `DUMP_EXPRESSION`, `INFO_EXPRESSION`, `parse_dump`, `CoroutineDump`, `Coroutine`. |

## Main public items

```rust
pub use discovery::{discover_tests, main_functions, MainFunction};
pub use kotest::{discover_specs, KotestSpec};
pub use tests::KotlinTests;
```

## Used by

- The app: `RunState` registers `KotlinTests` (`commands/run.rs`); `commands/kotlin.rs` builds
  the language server config, Kotlin runs, ktlint / detekt runs and the debugger plan with it.
  [`forge-wasm`](forge-wasm.md): `kotlin_main_functions`, `kotlin_command_line`,
  `kotlin_lint_report`, `kotlin_coroutines` and the test registry of the browser mode.

## Dependencies

- Workspace crates: [`forge-output`](forge-output.md), [`forge-runtools`](forge-runtools.md), [`forge-testing`](forge-testing.md), [`javaforge-build`](javaforge-build.md), [`javaforge-lang-java`](javaforge-lang-java.md)
- External: `serde`, `serde_json`

## Tests

9 source files, about 3300 lines; 21 unit tests on fixtures (no network).

```sh
cargo test -p kotlinforge-lang-kotlin
```
