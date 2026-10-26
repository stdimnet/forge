---
title: pyforge-interpreters
description: "PyForge: Python interpreters (system, pyenv, venv/virtualenv, uv, Poetry, conda, Pipenv, Hatch, Docker, Docker Compose, SSH, WSL): model, detection, metadata parsing and new environments."
order: 292
tags: [crate, rust, pack, python]
---

# pyforge-interpreters

`packs/pyforge/interpreters` — PyForge: Python interpreters (Settings | Project | Python
Interpreter, the status bar widget and its popup).

## Purpose

- `model` describes an interpreter (`EnvKind`: system, pyenv, venv, virtualenv, uv, Poetry,
  conda, Pipenv, Hatch, Docker, Docker Compose, SSH, WSL) and how it is stored in the
  `objects` settings `python.interpreters` / `python.projectInterpreters`;
  `python.interpreter` selects one by name or is a local path. `PythonInterpreter::label` is
  the status bar text (`Python 3.12 (shop-api)`). Remote kinds run through the PHP
  interpreters' command lines and path mappings: `wrap_target` turns the interpreter into a
  `phpforge_interpreters::PhpInterpreter` whose program is `python`.
- `probe` parses `python --version`, the probe script's JSON, `pyvenv.cfg`, `conda-meta` file
  names, `py -0p`, `conda env list --json` and the one-path answers of `poetry env info -p`,
  `pipenv --venv` and friends.
- `detect` (not in WebAssembly) finds the project's environments and the machine's Pythons
  (see the [user guide](../../user/python.md#python-interpreter)); versions come from metadata,
  only system Pythons are run. `SearchEnv` and the `version_of` / `run_tool` callbacks make it
  testable with fixture folders and fake executables.
- `create` plans the commands of a new environment (venv, uv, Poetry, conda) and the
  dependency install.

## Modules

| Module | Summary |
| --- | --- |
| `create.rs` | `NewEnvironment`, `ProjectFiles`, `plan` → `Step`s, `python_in`. |
| `detect.rs` | `SearchEnv::system`, `detect`, `version_of`, `run_with_timeout`. |
| `model.rs` | `EnvKind`, `PythonInterpreter`, `InterpreterSet` (`resolve`: names, ids and local paths). |
| `probe.rs` | Parsers and `PROBE_SCRIPT`. |

## Used by

- IPC: `python_interpreter_current`, `python_interpreters_detect`, `python_interpreter_info`, `python_environment_plan` (`app/src-tauri/src/commands/python.rs`)
- [`pyforge-packages`](pyforge-packages.md)
- Browser mode: `python_interpreter_label`, `python_environment_plan` in `forge-wasm`

## Dependencies

- Workspace crates: `phpforge-interpreters` (`packs/phpforge/interpreters`), [`pyforge-lang-python`](pyforge-lang-python.md)
- External: `serde`, `serde_json`, `dirs`

## Tests

5 source files, about 1600 lines; 10 unit tests (fixture environments, fake `python`
executables, a fake tool runner).

```sh
cargo test -p pyforge-interpreters
```
