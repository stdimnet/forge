---
title: forge-project
description: "Forge IDE projects: project type detection and New Project generators."
order: 200
tags: [crate, rust]
---

# forge-project

`crates/forge-project` — Forge IDE projects: project type detection and New Project generators.

## Purpose

Forge projects: detect what a project is made of (languages, frameworks, tools) so the
matching packs activate, and create new projects from generators (File | New Project).

## Modules

| Module | Summary |
| --- | --- |
| `create.rs` | Running a `Plan`: official CLIs with their output streamed line by line, built-in templates and `git init`. Programs run without a shell and with stdin closed, so interactive prompts cannot hang the creation. |
| `detect.rs` | Project type detection: which languages, frameworks and tools a project uses, read from marker files in the project root and one directory level below it (monorepos with a `backend/` and `frontend/` folder). Java: `pom.xml` (tool `maven`), Gradle builds (tool `gradle`), `*.java` or `src/main/java` / `src/test/java` → language `java`, pack `javaforge`. Kotlin: `*.kt`, Kotlin scripts (not `*.gradle.kts`), `src/{main,test}/kotlin` or the Kotlin plugin in a build → language `kotlin`, pack `kotlinforge`; `io.kotest` in a build → framework `kotest`. Groovy: `*.groovy` / `*.gvy` / `*.gy`, `src/{main,test}/groovy` or the `groovy` plugin → language `groovy` (pack `javaforge`), `org.spockframework` → framework `spock`. Scala: `build.sbt` / `project/build.properties` (tool `sbt`), `*.scala`, `src/{main,test}/scala` or Gradle's `scala` plugin → language `scala` (pack `javaforge`), ScalaTest / MUnit in `build.sbt` → frameworks `scalatest` / `munit`. Android: the Android Gradle Plugin in a build (also catalog aliases) or `src/main/AndroidManifest.xml` → framework `android` (language `java`, the AGP version of `gradle/libs.versions.toml`, tool `android`). C/C++: `CMakeLists.txt` (tool `cmake`), `compile_commands.json` in the root or `build/` (tool `compile-commands`), `meson.build` (tool `meson`), a `Makefile` next to C/C++ sources or with them in `src/` (tool `make`), C/C++ files in the root or two in a folder → language `cpp`, pack `cforge`; `platformio.ini` (tool `platformio`, language `cpp`), `west.yml` or `prj.conf` with a `CMakeLists.txt` (tool `zephyr`), ESP-IDF's `sdkconfig` / `idf_component.yml` with a `CMakeLists.txt` (tool `esp-idf`), `vcpkg.json` with a `CMakeLists.txt` (tool `vcpkg`), `conanfile.txt` / `conanfile.py` (tool `conan`). Spring Boot (Gradle plugin, Maven parent / BOM, starters; `spring_boot_version`) → framework `spring-boot`; Jakarta EE / Java EE APIs, JAX-RS, CDI or MicroProfile in a build → framework `jakarta-ee`. |
| `generators.rs` | New Project generators (File \| New Project). |
| `templates.rs` | Built-in offline project templates. Files are embedded with `include_str!` and use `{{placeholder}}` variables in both contents and paths (see `TemplateVars`). |

## Main public items

```rust
pub use create::{create, find_executable, has_tool, plan_for_system, CreateEvent};
pub use detect::{detect, language_name, Framework, ProjectInfo};
pub use generators::{…};
```

## Used by

- IPC: `project_detect`, `project_generators_list`, `project_plan`, `project_create`, `project_create_cancel` (`app/src-tauri/src/commands/project.rs`)
- UI: `app/ui/src/features/new-project`, `app/ui/src/features/project-type`
- Browser mode: not built to WebAssembly (runtime code); browser mode uses mocks

## Dependencies

- Workspace crates: [`forge-core`](forge-core.md)
- External: `serde`, `serde_json`, `tracing`

## Tests

5 source files, about 2847 lines; 40 unit tests.

```sh
cargo test -p forge-project
```
