/**
 * Loader for the WebAssembly build of Forge's Rust logic (crates/forge-wasm). Browser-mode
 * mocks call Rust through it instead of re-implementing logic in TypeScript. The desktop app
 * loads it too, for work that must be synchronous: key resolution (`@/keymap`) and message
 * formatting (`@/i18n`).
 */
import init, { initSync } from "./pkg/forge_wasm.js";
import * as bindings from "./pkg/forge_wasm.js";

export type ForgeWasm = typeof bindings;

let ready: Promise<ForgeWasm> | null = null;

/** Load the module once; later calls share the same instance. */
export function forgeWasm(): Promise<ForgeWasm> {
  ready ??= (async () => {
    if (import.meta.env.MODE === "test") {
      // vitest runs in Node (cwd = app/ui): read the file instead of fetching a URL.
      const fsModule = "node:fs";
      const { readFileSync } = (await import(/* @vite-ignore */ fsModule)) as {
        readFileSync: (path: string) => Uint8Array;
      };
      const cwd = (globalThis as unknown as { process: { cwd(): string } }).process.cwd();
      initSync({ module: readFileSync(`${cwd}/src/wasm/pkg/forge_wasm_bg.wasm`) });
    } else {
      await init();
    }
    return bindings;
  })();
  return ready;
}

/**
 * vitest only: load the module synchronously (Node can compile it on the spot), for code that
 * needs Rust before any `await`, like `t()` in a component under test. Null elsewhere.
 */
export function forgeWasmSyncForTests(): ForgeWasm | null {
  if (import.meta.env.MODE !== "test") return null;
  const proc = (globalThis as unknown as {
    process?: { cwd(): string; getBuiltinModule?(id: string): { readFileSync(path: string): Uint8Array } };
  }).process;
  const fs = proc?.getBuiltinModule?.("node:fs");
  if (!proc || !fs) return null;
  initSync({ module: fs.readFileSync(`${proc.cwd()}/src/wasm/pkg/forge_wasm_bg.wasm`) });
  ready ??= Promise.resolve(bindings);
  return bindings;
}

/** Call a JSON-in/JSON-out bridge function and parse the result. */
export async function callWasm<T>(fn: (json: string) => string, input: unknown): Promise<T> {
  await forgeWasm();
  return callWasmSync(fn, input);
}

/**
 * Synchronous variant for code that cannot wait (e.g. editor setup); only valid once
 * `forgeWasm()` has resolved, which browser-mode mocks await at startup.
 */
export function callWasmSync<T>(fn: (json: string) => string, input: unknown): T {
  const out = JSON.parse(fn(JSON.stringify(input))) as T | { error: string };
  if (out && typeof out === "object" && "error" in (out as object) && Object.keys(out as object).length === 1) {
    throw new Error((out as { error: string }).error);
  }
  return out as T;
}
