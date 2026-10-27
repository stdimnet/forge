---
title: forge-inspections
description: "Forge's inspection framework: registry, severities, profiles (PHPStorm XML), suppressions, duplicates, the batch runner, Code Cleanup and report export (pure, also built to WebAssembly)."
order: 27
tags: [crate, rust, inspections, profiles, suppressions, duplicates]
---

# forge-inspections

`crates/forge-inspections` — Forge's inspection framework: registry, severities, profiles (PHPStorm XML), suppressions, duplicates, the batch runner, Code Cleanup and report export (pure, also built to WebAssembly).

## Purpose

The common framework every problem in Forge goes through (see the
[user guide](../../user/inspections.md)). It does not inspect code itself except for
duplicated code; packs contribute inspections and providers.

- `spec`: what a pack contributes: `InspectionSpec` (`[[contributions.inspections]]`: id,
  name, group path, language, default severity, enabled, description, options, `cleanup`,
  `external`, `batch`) and `ToolMapping` (`[[contributions.inspectionMappings]]`: problems
  published with a diagnostics `source` and optionally a `code` or code prefix belong to an
  inspection).
- `registry`: the inspections of the enabled plugins plus the framework's own
  (`DuplicatedCode`), the Settings tree grouped by language and group with search,
  `inspection_for(source, code)` and Run Inspection by Name's `find_by_name`.
- `severity`: Error, Warning, Weak Warning, Server Problem, Typo, Info and custom severities
  (`SeverityDef`), PHPStorm's level names and how the editor highlights each.
- `profile`: `Profile` stores only what differs from the defaults (`ToolSettings`: enabled,
  severity, scope-specific settings, options); `ToolOp` / `ProfileOp` are the edits of the
  Settings page (copy, rename, delete, reset, import, custom severities), `view` the Settings
  tree with the profile's state, `ResolvedProfile::effective(id, path)` what an inspection
  does for a file (the first matching scope wins).
- `store`: where profiles live: IDE profiles in the user settings (`inspections.profiles`,
  `inspections.severities`), project profiles as PHPStorm XML in
  `.forge/inspectionProfiles/` (listed in `inspections.projectProfiles`), the selection in
  `inspections.profile`; `load` and `plan_save` (settings writes and file writes).
- `io`: PHPStorm inspection profile XML and Forge JSON, both ways.
- `lang`, `stmts`: comment and string syntax per file type and a parser-free statement and
  block finder (braces, semicolons, line breaks, indentation).
- `suppress`: `@noinspection` / `noinspection` comments (statement, function, class, file and
  line regions, `ALL`, ids with or without the `Inspection` suffix) and the Suppress actions
  with the comment each one inserts (PHP doc comments get a line in an existing doc block).
- `duplicates`: the token-based duplicate detector (rolling hashes over windows of the
  minimum size, maximal extension, grouping of copies; anonymized identifiers and literals).
- `engine`: `InspectionProvider` (per file and project-wide checks), `ProblemSource` (an
  external tool that yields problems by position and code, mapped through the registry) and
  `run`, which applies the profile, the scopes and the suppressions, with progress and
  cancel.
- `report`: `Report` / `ReportProblem`, the Inspection Results tree (`tree` grouped by
  severity, group, inspection, directory and file), export as HTML, one XML document, one XML
  file per inspection (PHPStorm's command-line format) and JSON.
- `cleanup`: applying edits, and Code Cleanup's plan of safe fixes per file.

## Main public items

```rust
pub struct InspectionSpec { id, name, group, language, severity, enabled, description, options, cleanup, external, batch }
pub struct ToolMapping { source, code, inspection }
pub struct Registry { inspections, mappings } // new, get, inspection_for, tree, find_by_name
pub struct Profile { name, level, tools: BTreeMap<String, ToolSettings> }
pub struct ProfilesState { profiles, selected, has_project, severities } // apply(ProfileOp)
pub struct ResolvedProfile { … } // effective(id, path) -> Option<Effective>
pub struct Suppressions { regions } // parse(path, text), is_suppressed(id, offset)
pub fn suppress::actions(path, text, offset, id) -> Vec<SuppressAction>
pub fn duplicates::find(files, &DuplicateOptions, cancelled) -> Vec<DuplicateGroup>
pub trait InspectionProvider { fn inspections(); fn inspect_file(); fn inspect_project(); }
pub trait ProblemSource { fn name(); fn problems(files, ctx) -> Result<Vec<ExternalProblem>, String>; }
pub fn run(files, &ResolvedProfile, providers, sources, &RunOptions, progress, cancelled) -> Report
pub fn report::tree(&Report, &[Grouping]) -> Vec<ResultNode>
pub fn cleanup::plan(problems, texts, &Registry) -> CleanupPlan
```

## Used by

- `forge-plugin-api` (`InspectionContribution`, `InspectionMappingContribution`,
  `PluginRegistry::inspection_registry`)
- `phpforge-lang-php` (`inspections::to_problem`; its problem codes are inspection ids)
- `app/src-tauri`: `inspections.rs` (scope files, the PHP provider, duplicates), the IPC
  commands in `commands/inspections.rs` and the command-line inspector `cli.rs`
  (`forge inspect`)
- `forge-wasm` (`inspections_*`): the UI's Settings tree, the diagnostics filter of the
  editor, Suppress actions, Run Inspection by Name, the results tree, and browser mode's runs

## Dependencies

- Workspace crates: `forge-scopes`
- External: `serde`, `serde_json`, `quick-xml`

## Tests

16 source files; unit tests for severities, specs and mappings, the registry tree and search,
profile edits and scope resolution, store round trips, PHPStorm XML, the lexer and statement
finder, suppression parsing and actions in PHP, JavaScript, Python, Rust and Go, duplicates,
the runner (providers, sources, suppressions, cancel), the results tree and exports, and Code
Cleanup.
