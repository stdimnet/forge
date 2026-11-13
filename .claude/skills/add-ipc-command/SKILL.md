---
name: add-ipc-command
description: Add a backend command that the Forge UI can call (Rust Tauri command + typed TypeScript client + browser-mode mock). Use whenever a UI feature needs data or actions from the Rust side, or when adding events streamed from Rust to the UI.
---

# Add an IPC command

Every command has three parts: the Rust handler, a typed UI client and a browser mock.

## 1. Rust

- Put logic in a library crate (`crates/forge-<area>` or `packs/<pack>/<plugin>`) with unit
  tests; keep `app/src-tauri` thin.
- Add the handler to `app/src-tauri/src/commands/<area>.rs` (create the module and add
  `pub mod <area>;` to `commands/mod.rs`):

  ```rust
  #[tauri::command]
  pub async fn file_text(path: String, state: State<'_, AppState>) -> Result<String> {
      let backend = state.backend()?;
      blocking(move || backend.read_file(&path)).await
  }
  ```

  - Return `forge_core::Result<T>`; errors serialize to strings.
  - Reach the project only through `state.backend()` (files, processes, git, executables,
    tunnels), so the command works in remote projects; never `std::fs` / `std::process` on
    the project. Analysis over many files gets its own backend call that runs next to the
    files (see `forge_backend::web`, `frameworks`, `Backend::collect_files`), with a protocol
    call in `forge_backend::protocol`, a handler in `forge-remote-agent` and an agent test.
  - The backend resolves workspace-relative paths and rejects paths outside the project.
  - Long work: make the command `async` or spawn a task; stream progress with
    `app.emit("<area>://<event>", payload)`.
  - Feature state: a separate struct managed with `.manage(...)` instead of growing `AppState`.
- Register it in `app/src-tauri/src/lib.rs` inside `tauri::generate_handler![...]`.
- Arguments arrive camelCased from JS (`fooBar` → `foo_bar`).

## 2. TypeScript client

`app/ui/src/ipc/<area>.ts`:

```ts
import { invoke, listen } from "./index";
export interface Route { method: string; uri: string; name?: string }
export const routesApi = {
  list: () => invoke<Route[]>("routes_list"),
  onChanged: (cb: (r: Route[]) => void) => listen<Route[]>("routes://changed", cb),
};
```

## 3. Browser mock

If the command is pure computation (no file system, processes or network), do not re-implement it
in TypeScript: expose the same Rust function from `crates/forge-wasm/src/<area>.rs` with
`#[wasm_bindgen]` (JSON string in, JSON string out), rebuild with `pnpm --dir app/ui wasm`, and let
the mock call it via `callWasm(wasm.<fn>, args)` from `@/wasm`. Otherwise write a mock:

`app/ui/src/mock/<area>Mock.ts`, imported from `app/ui/src/mock/index.ts`:

```ts
import { registerMock, emitMock } from "@/ipc";
registerMock("routes_list", () => [{ method: "GET", uri: "/orders/{order}", name: "orders.show" }]);
```

Mocks only load in browser mode and must never write to a real project. Use the demo files in
`mock/demoProject.ts` / `mock/demoConfigFiles.ts` so screenshots look realistic.

## 4. Document

Shared or cross-feature contracts go into `docs/ARCHITECTURE.md` → "Shared IPC contracts".
