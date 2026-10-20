/**
 * Loader for the WebAssembly build of Forge's Rust logic (crates/forge-wasm). Browser-mode
 * mocks call Rust through it instead of re-implementing logic in TypeScript; the desktop app
 * uses Tauri IPC and never loads it.
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

/** Call a JSON-in/JSON-out bridge function and parse the result. */
export async function callWasm<T>(fn: (json: string) => string, input: unknown): Promise<T> {
  await forgeWasm();
  const out = JSON.parse(fn(JSON.stringify(input))) as T | { error: string };
  if (out && typeof out === "object" && "error" in (out as object) && Object.keys(out as object).length === 1) {
    throw new Error((out as { error: string }).error);
  }
  return out as T;
}
