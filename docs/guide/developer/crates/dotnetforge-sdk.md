---
title: dotnetforge-sdk
description: "DotNetForge on the user's machine - the dotnet host and its SDKs and runtimes (settings, DOTNET_ROOT, PATH, install folders, dotnet --list-sdks), netcoredbg and Roslyn server locations, and the NuGet v3 feed client with a version cache in the user cache dir."
order: 311
tags: [crate, dotnet, pack, sdk, nuget, debug]
---

# dotnetforge-sdk

`packs/dotnetforge/sdk` — DotNetForge on the user's machine.

## Purpose

- `detect`: `DotnetStatus` — the `dotnet` hosts (the setting, `DOTNET_ROOT`, the `PATH`,
  `/usr/share/dotnet`, `/usr/lib/dotnet`, Homebrew, `C:\Program Files\dotnet`, `~/.dotnet`),
  SDKs and runtimes from `dotnet --list-sdks` / `--list-runtimes` (run in the temp folder
  through a `Runner`, so tests use recorded output) or the host's `sdk/` and `shared/`
  folders, the SDK `global.json` selects, problems and install hints.
- `tools`: netcoredbg (setting, Forge's download through
  `forge_langservers::managed::netcoredbg`, `PATH`) and the Roslyn server (setting, Forge's
  download through `forge_langservers::managed::roslyn`).
- `nuget`: `NuGetClient` — the service index of a feed (`SearchQueryService`,
  `PackageBaseAddress`), search, versions and the upgrades of installed packages; versions
  are cached in `<cache dir>/forge/nuget/<feed>/<id>.json` for six hours and used when the
  feed is unreachable.

## Modules

| Module | Summary |
| --- | --- |
| `detect.rs` | `detect`, `find_hosts`, `sdks_in`, `runtimes_in`, `DetectEnv`, `DotnetStatus`, `system_runner`. |
| `tools.rs` | `find_netcoredbg`, `find_roslyn`, `ToolLocation`. |
| `nuget.rs` | `NuGetClient`, `PackageUpdate`, `default_cache_dir`. |

## Used by

- IPC: `app/src-tauri/src/commands/dotnet.rs` (`dotnet_status`, `nuget_search`,
  `nuget_versions`, `nuget_updates`, `dotnet_debug_request`, the Roslyn server).

## Dependencies

- Workspace crates: [`dotnetforge-lang-dotnet`](dotnetforge-lang-dotnet.md),
  [`forge-langservers`](forge-langservers.md)
- External: `serde`, `serde_json`, `dirs`, `tokio`

## Tests

Detection runs against fake hosts in temporary folders and a fake runner; the NuGet client
against a local HTTP server (`forge_langservers::managed::testserver`) with recorded
answers, including the offline cache fallback. Nothing needs a .NET SDK or the network.

```sh
cargo test -p dotnetforge-sdk
```
