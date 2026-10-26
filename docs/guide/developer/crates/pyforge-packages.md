---
title: pyforge-packages
description: "PyForge: Python packages (installed packages of an environment, pip/uv/Poetry/Pipenv commands, PyPI metadata and search cached in the user cache dir)."
order: 294
tags: [crate, rust, pack, python]
---

# pyforge-packages

`packs/pyforge/packages` — PyForge: the Python Packages tool window, "Install requirement"
quick fixes and completion in dependency files.

## Purpose

- `listing` holds the `importlib.metadata` script that lists an environment's distributions
  with their top-level modules (it works without pip), and parses its JSON (and `pip list
  --format=json`).
- `manager` chooses pip, uv, Poetry or Pipenv for an environment and builds the install,
  uninstall and upgrade command lines.
- `pypi` parses PyPI's JSON API (latest version, summary, licence, required Python, home page,
  releases without yanked ones) and its search page.
- `net` (feature `net`, desktop only) is the PyPI client: requests through the system proxy,
  answers cached in `<cache dir>/forge/pypi` (12 hours for packages, 1 hour for searches) with
  the stale copy used when PyPI cannot be reached.

## Modules

| Module | Summary |
| --- | --- |
| `listing.rs` | `LIST_SCRIPT`, `parse`, `parse_pip_list`. |
| `manager.rs` | `Manager`, `Action`, `choose`, `command`. |
| `net.rs` | `Pypi` (`package`, `search`, `cached`). |
| `pypi.rs` | `PackageInfo`, `SearchHit`, `parse_json`, `parse_search`, URLs. |
| `testserver.rs` | A tiny HTTP server on localhost for the tests (no network). |

## Used by

- IPC: `python_packages_list`, `python_packages_command`, `python_packages_latest`, `python_packages_search`, `python_package_info`, `python_complete` (`app/src-tauri/src/commands/python.rs`)
- Browser mode: `python_packages_command` in `forge-wasm`

## Dependencies

- Workspace crates: [`pyforge-lang-python`](pyforge-lang-python.md), [`pyforge-interpreters`](pyforge-interpreters.md)
- External: `serde`, `serde_json`; with `net`: `reqwest` (rustls), `tokio`, `dirs`

## Tests

6 source files, about 790 lines; 6 unit tests (PyPI answers from a local mock server).

```sh
cargo test -p pyforge-packages --features net
```
