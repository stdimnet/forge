---
title: pyforge-lang-python
description: "PyForge: Python language support (PEP 440 versions and specifiers, PEP 508 requirements, requirements.txt and pyproject.toml dependencies, imports, requirements inspections and completion, run configurations; pure, also built to WebAssembly)."
order: 290
tags: [crate, rust, pack, python]
---

# pyforge-lang-python

`packs/pyforge/lang-python` — PyForge: Python language support that does not need a language
server. Pure Rust (no file system or processes), so the same code backs the desktop app and the
browser mode (WebAssembly, `crates/forge-wasm/src/python.rs`).

## Purpose

- `pep440` parses and normalizes versions (`1.0RC1` = `1.0rc1`, `1.0-1` = `1.0.post1`), orders
  them like pip and matches specifier sets (`~=`, `==1.4.*`, `!=`, `<`, `>` with the pre- and
  post-release exclusions, `===`).
- `pep508` parses dependency specifications (name, extras, specifiers, `@ url`, markers) with
  byte ranges, and normalizes names (PEP 503).
- `deps` reads the dependencies of `requirements*.txt` (options, `-r` includes, line
  continuations, comments, `--hash`) and `pyproject.toml` (PEP 621, PEP 735 groups, Poetry
  tables with `^` / `~` converted to PEP 440) with the position of each name, and edits the
  files for "Add requirement".
- `imports` finds the import statements of a file (strings and comments skipped, `try:`
  blocks marked) and knows the standard library and module → distribution names.
- `inspections` reports unsatisfied requirements and undeclared imports with quick fixes.
- `completion` finds the name / version context in dependency files; `run` builds the
  arguments of the Python run configuration and finds `__main__` guards.

## Modules

| Module | Summary |
| --- | --- |
| `completion.rs` | Completion contexts in dependency files: a package name or a version after an operator, in `requirements.txt` lines and in the dependency strings of pyproject.toml; final releases sorted before pre-releases. |
| `deps.rs` | Declared dependencies of `requirements*.txt` and `pyproject.toml` with their positions (0-based lines, UTF-16 columns), Poetry constraint conversion and the "Add requirement" edits. |
| `imports.rs` | Import statements, `STDLIB` (Python 3.8–3.13) and `KNOWN_DISTRIBUTIONS` (`yaml` → `PyYAML`, `PIL` → `Pillow`, …). |
| `inspections.rs` | `check_dep_file` ("Package requirement 'X' is not satisfied") and `check_imports` ("Package containing module 'x' is not listed in the project requirements") with `Fix`es. |
| `pep440.rs` | `Version`, `Specifier`, `SpecifierSet` (`contains`, `best`). |
| `pep508.rs` | `Requirement`, `parse`, `normalize`, `is_name`. |
| `run.rs` | `arguments` of a script / module run, `run_markers`, `module_of`. |

## Used by

- IPC: `python_inspect`, `python_add_requirement`, `python_complete`, `python_run_markers`, `python_dependencies`, the Python run configuration (`app/src-tauri/src/commands/python.rs`)
- [`pyforge-interpreters`](pyforge-interpreters.md), [`pyforge-packages`](pyforge-packages.md)
- UI: `app/ui/src/features/python`
- Browser mode: built into `forge-wasm`

## Dependencies

- External: `serde`, `serde_json`, `toml`

## Tests

8 source files, about 2970 lines; 24 unit tests.

```sh
cargo test -p pyforge-lang-python
```
