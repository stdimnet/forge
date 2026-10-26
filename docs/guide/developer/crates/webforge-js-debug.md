---
title: webforge-js-debug
description: "WebForge: the JavaScript debugger — Microsoft's js-debug DAP server (downloaded on consent, SHA-256 pinned) as the pwa-node, pwa-chrome and pwa-msedge debug adapters, browser detection and the launch requests of Node.js, npm, JavaScript Debug, attach and test run configurations."
order: 372
tags: [crate, rust, pack, debug, dap, javascript, js-debug]
---

# webforge-js-debug

`packs/webforge/js-debug` — WebForge: the JavaScript debugger on the shared debugging layer
([forge-dap](forge-dap.md)).

## Purpose

[JavaScript debugging](../../user/javascript-debugging.md) with Microsoft's js-debug
(vscode-js-debug, MIT). Nothing is bundled: the pinned standalone DAP server is downloaded
after the user agreed, checked against its SHA-256 and unpacked into Forge's data folder.

- `release`: the pinned release (`js-debug-dap-v1.140.0.tar.gz`, URL, SHA-256, size),
  `install_archive` (verify, unpack with the shared archive checks, mark complete), `ensure`
  (download with progress through `forge_langservers::managed::Http`), `installed`,
  `user_server` (a folder the user chose) and `default_base_dir`
  (`<data dir>/forge/debug-adapters/js-debug`).
- `adapter`: `JsDebugAdapter` implements `forge_dap::DebugAdapterDescriptor` for the targets
  `pwa-node`, `pwa-chrome` and `pwa-msedge`. Every session starts
  `node dapDebugServer.js ${port} 127.0.0.1` and connects to it (`Transport::Connect`);
  js-debug's child sessions (`startDebugging`) connect to the same server (forge-dap reuses a
  server Forge started). Launch arguments: `program`, `args`, `cwd`, `env`, `runtimeExecutable`,
  `outFiles` below the working directory (TypeScript breakpoints bind before scripts load),
  `outputCapture: "std"`, `autoAttachChildProcesses`, `skipFiles`; `url` and `webRoot` for
  browsers; `processId` (Attach to Process) and `address` / `port`
  (`LaunchMode::AttachPort`) for attaching; the request's `configuration` is merged last.
- `browsers`: Chrome, Edge and Chromium at their install paths (Linux, macOS, Windows) or on
  the `PATH`, and `pick` for a setting value or a path.
- `launch`: `request(configuration, context)`, the `forge_dap::LaunchRequest` of a run
  configuration: Node.js (file, interpreter, options, parameters), npm (`runtimeExecutable`
  = the package manager), JavaScript Debug (URL and the found browser), Attach to
  Node.js/Chrome (`AttachPort`), and the Node.js test runners through
  [webforge-js-testing](webforge-js-testing.md)'s `debug_program`.
- `locate` and `JsDebugStatus`: which DAP server Forge uses (the user's folder or the
  download) for the UI.

## Main public items

```rust
pub struct JsDebugAdapter { pub target: Target, pub config: JsDebugConfig }   // DebugAdapterDescriptor
pub fn request(cfg: &JsConfiguration, ctx: &LaunchContext) -> Result<LaunchRequest, String>;
pub fn pinned_release() -> Release;
pub fn release::ensure(http: &Http, base: &Path, release: &Release, on: &mut dyn FnMut(Event)) -> Result<PathBuf, String>;
pub fn system_browsers() -> Vec<Browser>;
```

## Used by

- IPC: `js_debug_status`, `js_debug_install` (events `js-debug://install`),
  `js_debug_browsers`, `js_debug_request`; the adapters are registered with the DAP manager
  whenever `dap_adapters` / `dap_start` sync it (`app/src-tauri/src/commands/js_debug.rs`)
- UI: `app/ui/src/features/webforge/jsDebug.ts` (run configuration types, the debug
  launcher, the download consent dialog), `app/ui/src/ipc/jsDebug.ts`
- Browser mode: not built to WebAssembly; `app/ui/src/mock/jsDebugMock.ts` simulates a
  js-debug session of the Node.js demo (`?project=node`)

## Dependencies

- Workspace crates: [`forge-dap`](forge-dap.md), [`forge-langservers`](forge-langservers.md),
  [`forge-runtools`](forge-runtools.md), [`forge-testing`](forge-testing.md),
  [`webforge-js-testing`](webforge-js-testing.md)
- External: `serde`, `serde_json`

## Tests

Unit tests for the archive check and unpacking (built in the test, no network), browser
detection with fake file systems, the adapter's launch / attach arguments and transports, and
the launch requests of every configuration type. An ignored smoke test debugs a TypeScript
file through its source map with a real js-debug and Node.js:

```sh
cargo test -p webforge-js-debug
JS_DEBUG_DIR=/path/to/js-debug cargo test -p webforge-js-debug -- --ignored
```
