---
title: dotnetforge-lang-dotnet
description: "DotNetForge: .NET support in the style of Rider - .sln / .slnx solutions, MSBuild projects with Directory.Build.props and central package management, launchSettings.json, global.json and SDK selection, build output as problems, xUnit / NUnit / MSTest discovery and the .NET test provider with TRX reports, dotnet command lines, dotnet new list, NuGet answers and versions, dotnet format reports and the Roslyn server setup; pure, also built to WebAssembly."
order: 310
tags: [crate, dotnet, csharp, fsharp, pack, msbuild, nuget, testing, wasm]
---

# dotnetforge-lang-dotnet

`packs/dotnetforge/lang-dotnet` — DotNetForge's .NET logic. Pure Rust (no I/O), shared by the
desktop backend and the WebAssembly build of browser mode.

## Purpose

- Read solutions and projects the way Rider shows them before a design-time build: `.sln`
  and `.slnx` (projects, solution folders, configurations), MSBuild project files with the
  nearest `Directory.Build.props` / `Directory.Packages.props` (properties with `$(…)`
  references, simple conditions, target frameworks, output type, references, package
  references with central versions and `VersionOverride`, F# compile order), and
  `launchSettings.json` profiles.
- `global.json` and the SDK the dotnet host picks (`rollForward` policies), `dotnet
  --list-sdks` / `--list-runtimes`, install hints per platform.
- MSBuild / compiler output as problems (C#, F#, VB, NuGet, MSBuild, `[project]` suffixes,
  each problem once) with the built outputs and the summary.
- A tolerant C# / F# scanner for xUnit, NUnit and MSTest tests and `Main` methods, and the
  `dotnet-test` provider of the shared test runner (`DotnetTests`): discovery, `dotnet test`
  with `FullyQualifiedName` filters and a TRX logger, Rerun Failed Tests, location rules from
  test classes to files.
- Command lines: build / rebuild / clean / restore / test / pack / publish / watch,
  ".NET Project" runs (`dotnet run` / `dotnet watch` with launch profiles), the debug build,
  `dotnet add|remove package`, `dotnet new` with `dotnet sln add`, `dotnet format`.
- `dotnet new list` tables, NuGet v3 service index / search / versions, NuGet SemVer
  comparison and upgrades, `dotnet format` JSON reports and EditorConfig lookup, the Roslyn
  server's arguments, `solution/open` and settings.

## Modules

| Module | Summary |
| --- | --- |
| `solution.rs` | `parse_sln`, `parse_slnx`, `Solution`, `SolutionProject`, `ProjectLanguage`. |
| `msbuild.rs` | `parse_msbuild`, `evaluate_project`, `eval_condition`, `DotnetProject`, `PackageReference`, `TestFramework`. |
| `workspace.rs` | `load_workspace`, `DotnetWorkspace`, `choose_solution`, `affects_workspace`. |
| `launch.rs` | `parse_launch_settings`, `LaunchProfile`, `strip_jsonc`. |
| `sdk.rs` | `parse_list_sdks`, `parse_list_runtimes`, `parse_global_json`, `select_sdk`, `install_options`. |
| `build.rs` | `parse_build_output`, `BuildReport`, `BuildProblem`, `output_of`. |
| `scan.rs` | `scan_csharp`, `scan_fsharp`, `SourceScan`, `TestClass`, `TestMethod`. |
| `context.rs` | `DotnetContext` (`dotnet.context` of test runs), location rules. |
| `provider.rs` | `DotnetTests`, `discover_file`, `test_args`, filters, `DOTNET_TEST`. |
| `command.rs` | `action_command`, `run_command`, `debug_build_command`, `package_command`, `new_project_commands`, `format_command`. |
| `templates.rs` | `parse_template_list`, `common_templates`. |
| `nuget.rs`, `version.rs` | `parse_service_index`, `parse_search`, `parse_versions`, URLs; `NuGetVersion`, `compare`, `upgrade`. |
| `format.rs` | `parse_format_report`, `editorconfig_candidates`. |
| `roslyn.rs` | `RoslynSetup`, `file_uri`. |

## Used by

- IPC: `app/src-tauri/src/commands/dotnet.rs`; `run.rs` registers `DotnetTests` and builds
  ".NET Project" runs, `testing.rs` hands C# / F# files to `discover_file`.
- WebAssembly: `crates/forge-wasm/src/dotnet.rs` and the test registry (browser mode).

## Dependencies

- Workspace crates: [`forge-output`](forge-output.md), [`forge-runtools`](forge-runtools.md),
  [`forge-testing`](forge-testing.md)
- External: `serde`, `serde_json`, `quick-xml`, `regex-lite`

## Tests

Unit tests on the `Bookstore` fixture solution (`tests/fixtures/bookstore`: `.sln` with CRLF
and BOM, `.slnx`, `Directory.Build.props`, `Directory.Packages.props`, `global.json`
with comments, a web project with launch profiles, a multi-target library, an xUnit
project, an F# tool) and recorded `dotnet build`, `dotnet --list-sdks`, `dotnet new list`
and NuGet search output. No SDK or network is needed.

```sh
cargo test -p dotnetforge-lang-dotnet
```
