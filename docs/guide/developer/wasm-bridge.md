---
title: The WebAssembly bridge
description: How crates/forge-wasm compiles pure Rust crates to WebAssembly for browser mode, how mocks call it, and how to add a module.
order: 50
tags: [wasm, webassembly, browser mode, mocks]
---

# The WebAssembly bridge

In browser mode (`pnpm dev`, screenshots, vitest) there is no Tauri backend. Instead of
re-implementing logic in TypeScript mocks, Forge compiles its **pure** crates to WebAssembly with
`crates/forge-wasm` and the mocks call them.

## How it works

- `crates/forge-wasm` depends on the pure crates (`forge-fuzzy`, `forge-filetypes`,
  `forge-keymap`, `forge-markdown`, `forge-output`, `forge-sql`, `forge-window`,
  `forge-plugin-api`, `forge-scratches` without `client`, `forge-remote` without `client`,
  `forge-containers` without `runtime`, `forge-agent-cli` without `runtime`, `luaforge-lang-lua`, `javaforge-build`, `javaforge-lang-java`,
  `phpforge-dql`, the WebForge crates).
- Each area is a module `crates/forge-wasm/src/<area>.rs` with `#[wasm_bindgen]` functions that
  take and return **JSON strings**, mirroring the matching IPC command:

  ```rust
  #[wasm_bindgen]
  pub fn keymap_resolve(request_json: &str) -> String { … }
  ```

- Stateful models are exposed through numeric handles (`run_output_new` / `run_output_handle`,
  `chat_new` / `chat_apply`); parser bridges live in `crates/forge-wasm/src/parsers/`.
- The generated package `app/ui/src/wasm/pkg` is **committed**, so `pnpm dev` works without a Rust
  toolchain.

## Calling it from the UI

```ts
import { callWasm, forgeWasm } from "@/wasm";
import { registerMock } from "@/ipc";

registerMock("search_rank", async ({ request }) => {
  const wasm = await forgeWasm();
  return callWasm(wasm.search_rank, request);
});
```

- `forgeWasm()` loads the module once.
- `callWasm(fn, input)` serializes the input, calls the function and parses the result.
- `callWasmSync(fn, input)` is for code that cannot wait (editor setup); it is valid once
  `forgeWasm()` has resolved. `mock/index.ts` waits for the module before the UI starts.

The keymap resolver is special: it runs from WebAssembly in **both** desktop and browser mode,
because a key handler must decide synchronously whether to call `preventDefault`.

## Rebuilding

After changing Rust code that the bundle uses:

```sh
pnpm --dir app/ui wasm
```

This needs the `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`) and
`wasm-bindgen-cli` at the version pinned in `crates/forge-wasm/Cargo.toml`. Commit the updated
`app/ui/src/wasm/pkg`.

## Adding a module

1. Make sure the crate (or its pure part) builds without file system, processes, network and
   tokio.
2. Add the dependency to `crates/forge-wasm/Cargo.toml` (with the runtime feature off).
3. Add `src/<area>.rs` with JSON-in/JSON-out functions and declare it in `lib.rs`.
4. Rebuild with `pnpm --dir app/ui wasm` and call it from the mock.
5. Add a test in `forge-wasm` (they run natively with `cargo test -p forge-wasm`) and, if useful,
   a vitest that loads the bundle (`app/ui/src/wasm/wasm.test.ts`).
