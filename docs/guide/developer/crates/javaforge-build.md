---
title: javaforge-build
description: "JavaForge: Maven and Gradle builds (detection from the build files, wrappers, modules, lifecycle goals and tasks, JaCoCo, command lines, sync models, dependency trees, Gradle DSL and pom.xml completion, outdated dependencies; pure, also built to WebAssembly)."
order: 270
tags: [crate, rust, pack]
---

# javaforge-build

`packs/javaforge/build` — JavaForge: Maven and Gradle builds (detection from the build files, wrappers, modules, lifecycle goals and tasks, JaCoCo, command lines, sync models, dependency trees, Gradle DSL and pom.xml completion, outdated dependencies; pure, also built to WebAssembly).

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
  with their JUnit XML report folders and JaCoCo reports; Maven profiles (`-Pdev,!prod`) and
  Skip Tests; Application runs through Gradle (`forgeRun` of `GRADLE_RUN_INIT_SCRIPT`) and the
  sync command lines.
- `model` is the Gradle / Maven tool window's model (`BuildModel`: projects, tasks, Maven
  plugins and profiles, dependency trees, source sets with their `java` / `groovy` / `kotlin` /
  `scala` / `resources` folders: `conventional_source_sets` of the applied plugins, Gradle's
  `sourceSets` after a sync): `static_model` from the build files,
  `GRADLE_INIT_SCRIPT` (`forgeModel` writes projects, tasks and resolved dependencies as JSON)
  with `parse_gradle_model`, `parse_maven_tree` for `mvn dependency:tree` (plain and `[INFO]`
  output) and `merge`.
- `wrapper` reads and updates `gradle-wrapper.properties` and compares
  `distributionSha256Sum` with the published checksum (`Checksum`).
- `dsl` completes Gradle Kotlin / Groovy DSL scripts from a hand-written model of the common
  blocks and in `pom.xml`; `request` says what Maven Central must answer (coordinates,
  versions), `complete` builds the items (`forge_jsonschema::CompletionOut`).
- `deps` finds declared dependencies with the place of their version (Maven properties
  resolved) and the "newer version available" markers; `version` orders Maven versions
  (qualifiers, variants like `-jre` / `-android`).
- `central` has Maven Central's search and `maven-metadata.xml` URLs and parsers; `net`
  (feature `net`, desktop only) downloads them with a cache in `<cache>/forge/maven-central`
  and asks services.gradle.org for releases and SHA-256 sums.

## Modules

| Module | Summary |
| --- | --- |
| `central.rs` | `search_query`, `search_url`, `parse_search` → `Artifact`, `metadata_url`, `parse_metadata` → `Versions`. |
| `command.rs` | `ExecOptions` (wrapper, executable, build folder, offline, profiles, skip tests), `executable`, `goals_command`, `build_project_command`, `TestFilter` and `test_command` → `TestRun`, `GRADLE_RUN_INIT_SCRIPT`, `GradleAppRun` and `gradle_application_command`, `gradle_sync_command`, `maven_sync_command`. |
| `deps.rs` | `FileKind`, `Declared`, `declarations`, `versioned_coordinates`, `OutdatedMarker` and `outdated`. |
| `detect.rs` | `BuildTool`, `Module`, `BuildProject` (`module_of`, `module`, `minimal`) and `detect(folder, files, read, prefer)`. |
| `dsl.rs` | `Script`, `Request`, `CentralData`, `request` and `complete`. |
| `goals.rs` | `Goal { name, group, description }` and `goals(&BuildProject)`. |
| `model.rs` | `BuildModel`, `ProjectModel`, `DependencyScope`, `DependencyNode`, `MavenPlugin`, `Profile`, `GRADLE_INIT_SCRIPT`, `parse_gradle_model`, `parse_maven_tree`, `maven_sync_model`, `maven_plugins`, `maven_profiles`, `profiles_argument`, `static_model`, `merge`, `task_names`, `SourceSet`, `SourceDir`, `conventional_source_sets`, `source_dir_kind`. |
| `net.rs` | `MavenCentral` (`search`, `versions`, `answer`, `versions_of`), `gradle_current`, `gradle_sha256` (feature `net`). |
| `version.rs` | `compare`, `is_prerelease`, `latest`, `is_newer`, `update_for`. |
| `wrapper.rs` | `GradleWrapper`, `parse_properties`, `distribution_url`, `update_properties`, `GradleRelease`, `parse_current`, `parse_sha256`, `Checksum` and `check`. |

## Main public items

```rust
pub use command::{build_project_command, executable, goals_command, test_command, BuildCommand, ExecOptions, TestFilter, TestRun};
pub use detect::{detect, BuildProject, BuildTool, Module, TestFrameworkUse};
pub use goals::{goals, Goal};
```

## Used by

- [`javaforge-lang-java`](javaforge-lang-java.md) (test commands), the app (`app/src-tauri/src/commands/java.rs`: `java_build_info`, Maven / Gradle runs, Build Project; `java_build.rs`: `java_build_model`, `java_sync_project`, the wrapper commands, `java_build_complete`, `java_build_outdated`) and [`forge-wasm`](forge-wasm.md) (`java_build_info`, `java_command_line`, `java_build_model`, `java_build_request`, `java_build_complete`, `java_build_outdated`, `java_wrapper`).

## Dependencies

- Workspace: `forge-jsonschema` (completion types, UTF-16 offsets)
- External: `serde`, `serde_json`, `regex-lite`; with `net`: `reqwest`, `sha2`, `tokio`, `dirs`

## Tests

12 source files; 33 unit tests (Gradle Kotlin and Groovy builds, Maven modules, wrappers, goals, test filters, JaCoCo, profiles, Gradle runs and syncs, recorded Gradle model JSON and `mvn dependency:tree` output in `tests/fixtures`, Maven plugins and profiles, version ordering, the wrapper properties, DSL and pom completion, declared and outdated dependencies, Maven Central and services.gradle.org against a local test server).

```sh
cargo test -p javaforge-build --features net
```
