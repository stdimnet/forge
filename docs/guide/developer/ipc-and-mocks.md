---
title: IPC commands and browser mocks
description: Adding a backend command - the Rust handler in app/src-tauri, the typed TypeScript client, events, and the browser-mode mock or WebAssembly bridge.
order: 70
tags: [ipc, tauri, mocks, commands]
---

# IPC commands and browser mocks

Every backend call has three parts: a Rust command, a typed client and a browser mock. Recipe:
[`add-ipc-command`](../../../.claude/skills/add-ipc-command/SKILL.md). The list of existing
contracts is in [`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md) and, per crate, in the
[crate pages](crates/index.md).

## 1. Rust

Put the logic in a crate with tests, then add a thin handler to
`app/src-tauri/src/commands/<area>.rs` (new modules go into `commands/mod.rs`):

```rust
#[tauri::command]
pub fn routes_list(state: State<'_, AppState>) -> Result<Vec<Route>> {
    state.with_workspace(|ws| forge_laravel::routes(ws.root()))
}
```

- Return `forge_core::Result<T>`; errors serialize to strings for the UI.
- Resolve user paths with `Workspace::resolve`, which rejects paths outside the project.
- Project operations that must also work remotely go through `AppState::backend()` (the
  `forge_backend::Backend` of the open project) and run on a blocking thread
  (`commands::workspace::blocking`). `with_workspace` is for local-only features; it fails for
  remote projects with "not available in remote projects yet".
- Long work: make the command `async` or spawn a task; stream progress with
  `app.emit("<area>://<event>", payload)`.
- Feature state: a struct registered with `.manage(...)` in `lib.rs`, not more fields in
  `AppState`.
- Register the command in `tauri::generate_handler![...]` in `app/src-tauri/src/lib.rs`.
- Arguments arrive camelCased from JS (`fooBar` → `foo_bar`).

## 2. Typed client

`app/ui/src/ipc/<area>.ts`:

```ts
import { invoke, listen } from "./index";

export interface Route { methods: string[]; uri: string; name?: string }

export const routesApi = {
  list: () => invoke<Route[]>("routes_list"),
  onChanged: (cb: (r: Route[]) => void) => listen<Route[]>("routes://changed", cb),
};
```

Features never call `invoke` with string literals directly; they use the client.

## 3. Browser mock

If the logic is pure (no file system, processes, network), expose the same Rust function through
`crates/forge-wasm` and call it from the mock ([The WebAssembly bridge](wasm-bridge.md)).
Otherwise write a mock in `app/ui/src/mock/<area>Mock.ts` and import it from `mock/index.ts`:

```ts
import { registerMock, emitMock } from "@/ipc";

registerMock("routes_list", () => [{ methods: ["GET"], uri: "/orders/{order}", name: "orders.show" }]);
```

- Mocks load only in browser mode and must **never write to a real project**; keep changes in
  memory.
- Use the demo projects (`mock/demoProject.ts`, `demoLaravel.ts`, `demoSymfonyProject.ts`,
  `demoWebProjects.ts`, `demoLuaProject.ts`, `demoConfigFiles.ts`) so screenshots look real.
- `emitMock("<area>://<event>", payload)` simulates events.

## Events

Events are named `<area>://<event>` (`settings://changed`, `fs://changed`, `lsp://diagnostics`,
`run://<runId>`, `agent://<sessionId>`, `services://event`, `remote://status`, …). Subscribe
before you start the work that emits them (pass your own id where the API allows it, e.g.
`terminal_open({ options: { id } })`).

## Conventions

- Paths across IPC are workspace-relative; positions are 0-based lines and UTF-16 columns
  (JavaScript string offsets) unless documented otherwise.
- Shared or cross-feature contracts are documented in `docs/ARCHITECTURE.md` → "Shared IPC
  contracts".
- Secrets never travel back to the UI: commands return masked values (`maskedKey`) or booleans.
