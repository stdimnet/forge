/**
 * Synchronous access to the Rust keymap engine (`forge-keymap` compiled to WebAssembly), in
 * the desktop app and in browser mode alike. A keydown handler has to decide on
 * `preventDefault` before it returns, which an IPC round trip cannot do, so key resolution
 * runs in-process; the keymap data itself still comes from the backend (`keymap_actions`).
 * Until the module is loaded, `resolveKeySync` returns null and callers use IPC.
 */
import { createSignal } from "solid-js";
import type { KeyInput, KeymapSnapshot, Platform, Resolution } from "@/ipc/keymap";
import { forgeWasm, type ForgeWasm } from "@/wasm";

let wasm: ForgeWasm | null = null;
let pendingSnapshot: KeymapSnapshot | null = null;
const [coreReady, setCoreReady] = createSignal(false);
let loading: Promise<boolean> | null = null;

/** Whether the engine is loaded (reactive). */
export { coreReady };

function call<T>(fn: (json: string) => string, input: unknown): T | null {
  const out = JSON.parse(fn(typeof input === "string" ? input : JSON.stringify(input))) as T | { error: string };
  if (out && typeof out === "object" && "error" in out && Object.keys(out).length === 1) {
    console.warn("keymap:", (out as { error: string }).error);
    return null;
  }
  return out as T;
}

/** Load the engine once; resolves to false when WebAssembly is unavailable. */
export function loadKeymapCore(): Promise<boolean> {
  loading ??= forgeWasm().then(
    (w) => {
      wasm = w;
      if (pendingSnapshot) loadBindings(pendingSnapshot);
      setCoreReady(true);
      return true;
    },
    (e) => {
      console.warn("keymap engine not loaded, using IPC", e);
      return false;
    },
  );
  return loading;
}

/** Give the resolver the bindings of a keymap snapshot. */
export function loadBindings(snapshot: KeymapSnapshot): void {
  pendingSnapshot = snapshot;
  if (wasm) call<number>(wasm.keymap_load_bindings, snapshot);
}

/** Resolve a key event now, or null when the engine is not loaded yet. */
export function resolveKeySync(input: KeyInput): Resolution | null {
  return wasm ? call<Resolution>(wasm.keymap_handle_key, input) : null;
}

/** Canonical keystroke of a key event ("Ctrl+Shift+A"); null for a bare modifier. */
export function strokeOfEvent(e: Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">): string | null {
  if (!wasm) return null;
  const { key, code, ctrlKey, altKey, shiftKey, metaKey } = e;
  return call<{ stroke: string | null }>(wasm.keymap_stroke_of_event, { key, code, ctrlKey, altKey, shiftKey, metaKey })?.stroke ?? null;
}

/** Display text of a shortcut string; unparsable text (or no engine yet) is returned as is. */
export function formatShortcut(text: string, platform: Platform): string {
  if (!coreReady() || !wasm || !text) return text;
  return call<string>(wasm.keymap_format, { text, platform }) ?? text;
}
