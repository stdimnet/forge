---
title: pyforge-testing
description: "PyForge: pytest, unittest and tox on the shared test runner (discovery from source, parametrize ids, command lines, Rerun Failed Tests, tox environments), debugpy and cProfile command lines, .pstats profiles and the Python Console protocol."
order: 295
tags: [crate, rust, pack, python, pytest, unittest, tox, debugpy, profiler]
---

# pyforge-testing

`packs/pyforge/testing` — PyForge's tests, debugger, console and profiler support (the
`forge.python-tools` plugin, see [PyForge](../../user/python.md)). Pure, so the desktop app
and the WebAssembly build share it.

## Purpose

- `source` scans Python source without a parser: logical lines (brackets, continuations and
  triple-quoted strings joined, comments removed) and the classes and functions they declare
  with decorators, bases and nesting.
- `discover` finds pytest tests (`test_*.py` / `*_test.py`, `test*` functions, `Test*`
  classes without `__init__`), `unittest.TestCase` cases in any module and the ids of
  `@pytest.mark.parametrize` cases, and turns them into gutter targets with temporary
  `pytest` / `unittest` configurations (`config`). `detect` tells whether the project uses
  pytest or tox from its files and picks a file's runner (`python.testing.runner`).
- `command` builds the interpreter arguments: `-m pytest` with a scope, `--junitxml` (or
  `--report-log`) and pytest-cov's Cobertura report; Forge's unittest runner
  (`helpers/forge_unittest.py`, TeamCity messages, sent inline as base64 with `-c`);
  `-m tox -e …`; `-m debugpy --listen … --wait-for-client`; `-m cProfile -o …`; the console
  (`helpers/forge_console.py`). Report paths are made relative to the working directory, so
  they stay valid where interpreters and run targets map the project.
- `rerun` turns failed tests into pytest node ids or unittest ids, with the classes read from
  the files. `tox` reads `tox.ini`, `setup.cfg`, `tox.toml` and `pyproject.toml` environments
  (generative names expanded).
- `pstats` reads cProfile's `.pstats` files (the `marshal` subset they use) into hot spots and
  a call tree; `pstats::write` writes them (tests, the browser demo).
- `console` separates the console helper's replies (`\x1e` + JSON lines) from the user's
  output and builds its requests.
- `provider` registers the three runners as a `forge_testing::TestFrameworkProvider`.

## Modules

| Module | Summary |
| --- | --- |
| `source.rs` | `logical_lines`, `declarations` (`Def`, `Decorator`), `split_top_level`, `bracket_contents`. |
| `discover.rs` | `scan` (`Scan`, `Item`, `Case`), `test_file`, `parametrize_cases`, `Runner`. |
| `config.rs` | The `pytest`, `unittest` and `tox` types, their options and builders. |
| `detect.rs` | `detect` (`ProjectTesting`), `choose_runner`. |
| `command.rs` | `pytest`, `unittest`, `tox`, `build`, `debugpy_args`, `profile_args`, `console`, `inline_script`, `relative_to`. |
| `rerun.rs` | `rerun_failed`. |
| `tox.rs` | `environments`, `from_ini`, `from_toml`, `expand` (`ToxEnv`). |
| `pstats.rs` | `parse`, `unmarshal`, `Profile` (`hot_spots`, `call_tree`, `view`, `map_files`), `write`. |
| `console.rs` | `Splitter`, `Piece`, `request`. |
| `provider.rs` | `PythonTestFrameworks`. |

## Used by

- IPC: `run_start` / `run_command_line` for the `python`, `pytest`, `unittest` and `tox`
  types, `test_discover` and `test_rerun_failed` for Python files, `python_testing_detect`,
  `python_tox_environments`, `python_debugpy_status`, `python_debug_plan`,
  `python_console_*`, `python_profiles`, `python_profile_view`, `python_profile_open`,
  `python_profile_delete` (`app/src-tauri/src/commands/python_tools.rs`)
- Browser mode: `python_test_discover`, `python_testing_detect`, `python_rerun_failed`,
  `python_test_command`, `python_tox_environments`, `python_demo_profile` and the test
  registry of `testing_discover` in `forge-wasm`
- Output: the tox parser lives in [`forge-output`](forge-output.md) (`TestReport::Tox`)

## Dependencies

- Workspace crates: [`forge-testing`](forge-testing.md), [`forge-output`](forge-output.md), [`forge-runtools`](forge-runtools.md)
- External: `serde`, `serde_json`, `toml`

## Tests

Unit tests for discovery (functions, classes, nesting, parametrize ids), detection, command
lines, Rerun Failed Tests, tox configurations, `.pstats` parsing (a real CPython snapshot in
`fixtures/`) and the console protocol. Two ignored smoke tests run a local `python3`
(Forge's unittest runner and cProfile).

```sh
cargo test -p pyforge-testing
cargo test -p pyforge-testing --test smoke -- --ignored
```
