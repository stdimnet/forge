---
title: goforge-sdk
description: "GoForge: Go SDKs and tools on the user's machine - SDK detection, consented SDK downloads from go.dev/dl with SHA-256 checks, gopls / Delve / goimports locations and pinned go install commands, module proxy lookups cached in the user cache dir."
order: 297
tags: [crate, rust, pack, go, download]
---

# goforge-sdk

`packs/goforge/sdk` — GoForge on the user's machine: SDKs, tools and the module proxy.

## Purpose

- `detect`: Go SDKs found without running anything — the `go` on the `PATH` (symlinks
  resolved), `GOROOT`, `/usr/local/go`, `/usr/lib/go`, `/usr/lib/golang`, `/opt/go`, Snap,
  Homebrew, `Program Files\Go`, `~/sdk/go*`, `~/go/go*` and `<data dir>/forge/go-sdks` —
  newest first, the version from the SDK's `VERSION` file.
- `download`: the release list of `go.dev/dl/?mode=json`, the archive of a stable release for
  the platform, and the install: download, SHA-256 check against the list, unpack
  (`forge_langservers::managed::archive`, `go/` stripped) into a staging folder renamed to
  `<data dir>/forge/go-sdks/go<version>`. Only called after the user agreed.
- `tools`: gopls, Delve, goimports and golangci-lint — the setting with the user's path, the
  lookup order (Forge's `GOBIN`, `GOBIN`, `GOPATH/bin`, the SDK's `bin`, `PATH`), and the
  pinned `go install <module>@<version>` per SDK version (Delve 1.27.2 for Go 1.25+, 1.25.2
  before; gopls 0.23.0 / 0.20.0; goimports 0.30.0). golangci-lint (GPL-3.0) is never installed.
- `proxy`: `ModuleProxy` (module versions from `<proxy>/<escaped>/@v/list`, `@latest` for
  untagged modules), cached 12 hours in `<cache dir>/forge/go-proxy` with the stale copy used
  offline; `proxy_from_goproxy`; `cached_modules` (module paths in the module cache).

## Modules

| Module | Summary |
| --- | --- |
| `detect.rs` | `detect`, `DetectEnv`, `GoSdk`, `sdk_version`, `sdk_from_folder`, `default_sdks_dir`. |
| `download.rs` | `releases`, `downloadable`, `install`, `platform`. |
| `tools.rs` | `GoTool`, `ToolDirs`, `install_command`, `status`, `default_tools_dir`. |
| `proxy.rs` | `ModuleProxy`, `cached_modules`, `proxy_from_goproxy`. |
| `testserver.rs` | A local HTTP server for the tests. |

## Used by

- IPC: `app/src-tauri/src/commands/go.rs` (`go_status`, `go_sdk_releases`,
  `go_sdk_download`, `go_tool_install_command`, `go_mod_complete`, `go_module_versions`).
- Browser mode: not built to WebAssembly (file system and network); `mock/goMock.ts`.

## Dependencies

- Workspace crates: [`goforge-lang-go`](goforge-lang-go.md),
  [`forge-langservers`](forge-langservers.md) (HTTP client, archives, SHA-256)
- External: `serde`, `serde_json`, `dirs`, `tokio`

## Tests

Fake SDK folders for detection, a local HTTP server with a generated SDK tarball for the
download (checksum mismatch and unsafe names refused) and the module proxy (cache, offline
fallback). No network.

```sh
cargo test -p goforge-sdk
```
