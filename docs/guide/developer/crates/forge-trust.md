---
title: forge-trust
description: Project trust and Safe Mode - trusted projects and locations, path keys for local and remote projects, the Safe Mode flag and which programs count as project code.
order: 242
tags: [crate, rust, trust, safe mode]
---

# forge-trust

`crates/forge-trust` — Project trust and Safe Mode: trusted projects and locations, and what
may run.

## Purpose

The logic behind [Project trust and Safe Mode](../../user/project-trust.md):

- `TrustStore { projects, locations }` is `<config dir>/forge/trusted-projects.toml`
  (`TrustStore::default_path`, `load`, `save`). `reason(project)` says why a project is
  trusted (`TrustReason::Project` or `Location { location }`), `check(project)` builds the
  `TrustCheck` the dialog and banner show (key, display text, parent folder for "Trust
  projects in …"). `trust`, `distrust`, `add_location` (expands `~`, refuses relative paths
  and file system roots) and `remove_location` edit it.
- Keys: `local_key(path)` is the canonical local path (symlinks resolved, no Windows `\\?\`
  prefix); `remote_key(user, host, port, path)` is `ssh://user@host:port/path`.
  `normalize` cleans separators, `.` and `..`; `contains(location, project)` matches whole path
  components on the same machine (case-insensitive for local paths on Windows); `display`
  shows remote keys as `user@host:/path`.
- `SafeMode` is the shared flag of the open project; `check(what)` returns the error
  `refusal(what)` while it is on.
- `runs_project_program(root, program, args)` tells whether a command starts a program from
  the project (the program or a path argument lies inside `root`), lexically so it works for
  remote roots; `server_runs_project_code(id)` lists catalog language servers that execute
  project files (`SERVERS_RUNNING_PROJECT_CODE`).

## IPC

`app/src-tauri/src/commands/trust.rs`: `trust_check({ path })`, `trust_status()`,
`trust_project({ path?, location? })`, `trust_locations()`, `trust_location_add({ path })`,
`trust_location_remove({ key })`; `trust://changed` carries the open project's status.
`AppState::safe_mode` is set when a project opens (`refresh_safe_mode`) and every command that
starts project programs calls `AppState::require_trusted`; the LSP manager's launcher is
wrapped by `SafeModeLauncher`.

## Dependencies

`forge-core` (errors), `serde`, `toml`, `dirs`.

## Tests

Unit tests in `src/lib.rs`: normalization, remote keys, locations and their hosts, parent
locations, the store round trip, roots and relative paths, Safe Mode errors and project
program detection. `commands::trust` tests the app state following the store.
