---
title: forge-maintenance
description: "Repairing and resetting the IDE: Invalidate Caches, Restore Default Settings, Export / Import Settings."
order: 145
tags: [crate, rust, invalidate caches, settings, export, import]
---

# forge-maintenance

`crates/forge-maintenance` — Repairing and resetting the IDE: Invalidate Caches, Restore Default Settings, Export / Import Settings.

## Purpose

The logic behind PHPStorm's File | Invalidate Caches… and File | Manage IDE Settings
([user guide](../../user/repair-ide.md), [settings](../../user/settings-and-themes.md#export-and-import-settings)).

- **Invalidate Caches** (`caches`): `invalidate_model` is the dialog (texts and the four
  `CacheOption`s); `cleanup_plan(dirs, options)` lists the folders that exist and would be
  deleted (always the PHP index, search index, command line tools cache, JSON schemas,
  Packagist and shell integration; per option the Local History of all projects, the PHP
  stubs, the webview cache and language server data); `run_cleanup` deletes them and reports
  failures.
- **Work for the next start** (`pending`): `schedule` merges work into
  `<data dir>/forge/pending-maintenance.json`; `run_pending` runs it once at startup (the
  app calls it before the settings load), removing the marker first.
- **Settings groups** (`groups`): PHPStorm's export components as setting key prefixes
  (the longest prefix wins, the rest is "Other settings") or whole configuration files;
  `count_groups` lists the non-empty ones for the dialogs.
- **Settings archive** (`archive`): `export_settings` writes a zip with `forge-settings.json`
  (format, version, groups), the chosen keys as `settings.toml` and the chosen files;
  `read_archive` reads one back (unknown entries ignored, size-limited); `SettingsArchive::plan`
  replaces the chosen groups of the current user settings (keys the archive does not set are
  removed).
- **Restore Default Settings** (`restore`): `backup_settings` copies `<config dir>/forge` to
  `forge-backup-<UTC timestamp>`; `reset_settings` removes `settings.toml`,
  `new-project-settings.toml` and `dialog-windows.json` at the next start.

`ForgeDirs` names the user directories, so tests run entirely in temporary folders.

## Main public items

```rust
pub enum CacheOption { FileSystemAndLocalHistory, SharedIndexes, BrowserEngine, LanguageServerData }
pub fn invalidate_model() -> InvalidateModel { … }
pub fn cleanup_plan(dirs: &ForgeDirs, options: &[CacheOption]) -> Vec<CleanupItem> { … }
pub fn run_cleanup(plan: Vec<CleanupItem>) -> CleanupReport { … }
pub fn schedule(dirs: &ForgeDirs, change: impl FnOnce(&mut PendingWork)) -> Result<(), String> { … }
pub fn run_pending(dirs: &ForgeDirs) -> Option<PendingReport> { … }
pub fn settings_groups() -> &'static [SettingsGroup] { … }
pub fn group_of_key(key: &str) -> &'static str { … }
pub fn export_settings(values, files, groups, version) -> Result<Vec<u8>, String> { … }
pub fn read_archive(bytes: &[u8]) -> Result<SettingsArchive, String> { … }
pub fn backup_settings(dirs: &ForgeDirs, now_secs: u64) -> Result<PathBuf, String> { … }
```

## Used by

- IPC: `maintenance_invalidate_model`, `maintenance_invalidate_caches`, `maintenance_restart`,
  `maintenance_rescan_indexes`, `settings_export_info`, `settings_export`,
  `settings_import_preview`, `settings_import`, `settings_restore_defaults`
  (`app/src-tauri/src/commands/maintenance.rs`).
- UI: `app/ui/src/features/maintenance` (Invalidate Caches dialog, Repair IDE balloons,
  Export / Import Settings chooser, Restore Default Settings).
- Browser mode: the pure models (`caches`, `groups`) are built into `forge-wasm`
  (`maintenance_invalidate_model`, `maintenance_settings_groups`).

## Dependencies

- Workspace crates: `forge-core` (feature `disk`)
- External: `serde`, `serde_json`, `dirs`, `zip` (MIT, deflate only; feature `disk`)

## Tests

7 source files; 9 unit tests (dialog model, cleanup plan with every option in a temporary
folder, the marker running once, settings groups, archive round trip and plans, foreign
archives, UTC timestamps, backups and reset).
