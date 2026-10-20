---
title: forge-project
description: "Forge IDE projects: project type detection and New Project generators."
order: 190
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
| `detect.rs` | Project type detection: which languages, frameworks and tools a project uses, read from marker files in the project root and one directory level below it (monorepos with a `backend/` and `frontend/` folder). |
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
