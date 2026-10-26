---
title: javaforge-build
description: "JavaForge: Maven and Gradle builds (detection from the build files, wrappers, modules, lifecycle goals and tasks, JaCoCo, command lines for goals, tasks, Build Project and tests; pure, also built to WebAssembly)."
order: 270
tags: [crate, rust, pack]
---

# javaforge-build

`packs/javaforge/build` — JavaForge: Maven and Gradle builds (detection from the build files, wrappers, modules, lifecycle goals and tasks, JaCoCo, command lines for goals, tasks, Build Project and tests; pure, also built to WebAssembly).

## Purpose

JavaForge's Maven and Gradle support, like IntelliJ IDEA's build tool integration. Pure Rust:
the caller hands in file names and texts, so the desktop backend, the remote agent and the
browser mode (WebAssembly) share it.

- `detect` reads a build (`BuildProject`): the tool, its wrapper (`mvnw`, `gradlew`), the
  Kotlin or Groovy DSL, modules (`settings.gradle` `include`, `pom.xml` `<modules>`), JaCoCo,
  the test frameworks the build depends on, the Java release, Gradle's `application` main class,
  Spring Boot and custom tasks.
- `goals` lists Maven lifecycle phases and plugin goals, or Gradle tasks by group, for the
  Gradle / Maven tool windows.
- `command` builds command lines: goals and tasks (Gradle `:module:task`, Maven `-pl module
  -am`), Build Project (`classes testClasses`, `test-compile`) and test runs (Gradle `cleanTest
  test --tests`, Maven Surefire `test -Dtest=` with `-Dsurefire.failIfNoSpecifiedTests=false`)
  with their JUnit XML report folders and JaCoCo reports.

## Modules

| Module | Summary |
| --- | --- |
| `command.rs` | `ExecOptions` (wrapper, executable, build folder, offline), `executable`, `goals_command`, `build_project_command`, `TestFilter` (all, package, class, method, a list) and `test_command` → `TestRun { command, report_dirs, coverage_report }`. |
| `detect.rs` | `BuildTool`, `Module`, `BuildProject` (`module_of`, `module`, `minimal`) and `detect(folder, files, read, prefer)`. |
| `goals.rs` | `Goal { name, group, description }` and `goals(&BuildProject)`. |

## Main public items

```rust
pub use command::{build_project_command, executable, goals_command, test_command, BuildCommand, ExecOptions, TestFilter, TestRun};
pub use detect::{detect, BuildProject, BuildTool, Module, TestFrameworkUse};
pub use goals::{goals, Goal};
```

## Used by

- [`javaforge-lang-java`](javaforge-lang-java.md) (test commands), the app (`app/src-tauri/src/commands/java.rs`: `java_build_info`, Maven / Gradle runs, Build Project) and [`forge-wasm`](forge-wasm.md) (`java_build_info`, `java_command_line`).

## Dependencies

- External: `serde`, `regex-lite`

## Tests

4 source files, about 1190 lines; 9 unit tests (Gradle Kotlin and Groovy builds, Maven modules, wrappers, goals, test filters, JaCoCo).

```sh
cargo test -p javaforge-build
```
