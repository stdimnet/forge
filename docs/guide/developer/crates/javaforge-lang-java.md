---
title: javaforge-lang-java
description: "JavaForge: Java language support (structural source scanner, main methods, JUnit 5/4/3 and TestNG discovery and commands for the shared test runner, application command lines, Eclipse JDT LS launch configuration; pure, also built to WebAssembly)."
order: 271
tags: [crate, rust, pack]
---

# javaforge-lang-java

`packs/javaforge/lang-java` — JavaForge: Java language support (structural source scanner, main methods, JUnit 5/4/3 and TestNG discovery and commands for the shared test runner, application command lines, Eclipse JDT LS launch configuration; pure, also built to WebAssembly).

## Purpose

JavaForge's Java language support. Pure Rust (no file system or processes); code intelligence
itself comes from Eclipse JDT LS through `forge-lsp`.

- `source` scans a Java file: package, imports, types (classes, interfaces, enums, records,
  annotation types, nested types with binary names such as `com.acme.Outer$Inner`) and their
  methods with annotations, modifiers, return types and parameters. It skips comments, strings,
  text blocks and method bodies and tolerates half-typed code.
- `discovery` finds `main` methods for the Run gutter (`public static void main(String[])`,
  Java 25 instance mains and compact source files) and tests: JUnit 5 (`@Test`,
  `@ParameterizedTest`, `@RepeatedTest`, `@TestFactory`, `@TestTemplate`, `@Nested`), JUnit 4,
  JUnit 3 `TestCase` subclasses and TestNG (method and class level `@Test`), chosen by imports.
- `tests` is the shared test runner's provider (`JavaTests::junit()`, `JavaTests::testng()`):
  gutter targets with temporary configurations, Rerun Failed Tests (`Class#method` lists) and
  commands through `javaforge-build` with `TestReport::JUnitFile`, `JUnitFlavor::Java`, the
  build's report folders (`TestCommand::report_dirs`), location rules from the source root and
  JaCoCo coverage with its own rules (main sources). With the option `runner = junit` ("Run
  tests using: IntelliJ IDEA") JUnit runs on the JUnit Platform Console Launcher instead, on
  the class path the backend resolved (`resolvedClasspath`, `launcherJar`, `reportsDir`).
  `build_command` also serves KotlinForge (`language = kotlin`) and Spock
  (`language = groovy`: `src/<set>/groovy/` roots, `JUnitFlavor::Kotlin` names with spaces,
  locations renamed to `.groovy`).
- `launch` builds Application command lines (`java` with VM options, `-p` / `-cp`, `-m`, the
  JDWP agent of debug runs), the debug arguments of Gradle (`--debug-jvm`) and Surefire, and
  `junit_console_command` (`java -jar junit-platform-console-standalone.jar execute
  --select-… --reports-dir …`).
- `jdtls` describes how JDT LS starts: the command line (read-only installation, OSGi
  configuration and `-data` in the cache dir), `initializationOptions` (java-debug bundle,
  extended client capabilities) and the `java.*` settings (JDKs as runtimes, Gradle / Maven
  import, hot code replace), plus the JDT LS and java-debug commands Forge sends.

## Modules

| Module | Summary |
| --- | --- |
| `discovery.rs` | `main_methods`, `discover_tests`, `Framework`, `TestDiscovery`, `source_root`. |
| `jdtls.rs` | `JdtlsSetup` (`args`, `settings`, `initialization_options`), `Runtime`, `execution_environment`, `config_folder`, `commands`. |
| `launch.rs` | `AppLaunch`, `application_command`, `jdwp_listen`, `remote_jvm_arguments`, `build_output_classpath`, `build_tool_debug_args`, `JUnitSelector`, `junit_console_command`, `DEBUG_PORT_ENV`. |
| `source.rs` | The tolerant scanner: `parse(text) -> JavaFile { package, imports, types, top_level_methods }`. |
| `tests.rs` | `JavaTests` (`TestFrameworkProvider`) and `module_of_source_root`. |

## Main public items

```rust
pub use discovery::{discover_tests, main_methods, Framework, MainMethod, TestDiscovery};
pub use tests::JavaTests;
```

## Used by

- The app: `RunState` registers `JavaTests` (`commands/run.rs`); `commands/java.rs` builds
  JDT LS, Application and debug runs with it. [`forge-wasm`](forge-wasm.md): `java_main_methods`,
  `java_command_line` and the test registry of the browser mode.

## Dependencies

- Workspace crates: [`forge-output`](forge-output.md), [`forge-runtools`](forge-runtools.md), [`forge-testing`](forge-testing.md), [`javaforge-build`](javaforge-build.md)
- External: `serde`, `serde_json`

## Tests

6 source files; 16 unit tests.

```sh
cargo test -p javaforge-lang-java
```
