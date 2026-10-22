/**
 * Synchronous access to the Vim engine (`crates/forge-vim` compiled to WebAssembly). A key
 * press must be answered before the keydown handler returns, so the engine runs in-process
 * in the desktop app and in browser mode alike. Until the module is loaded, `vimReady()`
 * is false and editors type normally.
 */
import { createSignal } from "solid-js";
import type {
  VimHandlerOverride,
  VimHandlerRow,
  VimKeyEvent,
  VimRcReport,
  VimResponse,
  VimSync,
} from "@/ipc/vim";
import { forgeWasm, type ForgeWasm } from "@/wasm";

let wasm: ForgeWasm | null = null;
const [ready, setReady] = createSignal(false);

/** Whether the engine is loaded (reactive). */
export const vimReady = ready;

let loading: Promise<boolean> | null = null;

export function loadVimEngine(): Promise<boolean> {
  loading ??= forgeWasm().then(
    (w) => {
      wasm = w;
      setReady(true);
      return true;
    },
    (e) => {
      console.warn("Vim emulation not available", e);
      return false;
    },
  );
  return loading;
}

function call<T>(fn: (json: string) => string, input: unknown): T {
  const out = JSON.parse(fn(JSON.stringify(input))) as T | { error: string };
  if (out && typeof out === "object" && "error" in out && Object.keys(out).length === 1) {
    throw new Error((out as { error: string }).error);
  }
  return out as T;
}

const engine = (): ForgeWasm => {
  if (!wasm) throw new Error("Vim engine not loaded");
  return wasm;
};

export const vimEngine = {
  attach: (text: string): number => call<{ handle: number }>(engine().vim_attach, { text }).handle,
  detach: (handle: number) => wasm && call<boolean>(wasm.vim_detach, { handle }),
  /** The focused editor (null on blur): its keys may be taken from the IDE keymap. */
  focus: (handle: number | null) => wasm && call<boolean>(wasm.vim_focus, { handle }),
  key: (handle: number, event: VimKeyEvent | null, sync: VimSync): VimResponse =>
    call<VimResponse>(engine().vim_key, { handle, event, sync }),
  timeout: (handle: number, sync: VimSync): VimResponse => call<VimResponse>(engine().vim_timeout, { handle, sync }),
  state: (handle: number): VimResponse => call<VimResponse>(engine().vim_state, { handle }),
  configure: (enabled: boolean, handlers: VimHandlerOverride[]) =>
    wasm && call<boolean>(wasm.vim_configure, { enabled, handlers }),
  loadRc: (text: string): VimRcReport => call<VimRcReport>(engine().vim_load_rc, { text }),
  handlers: (handlers: VimHandlerOverride[]): VimHandlerRow[] =>
    call<VimHandlerRow[]>(engine().vim_handlers, { handlers }),
  setClipboard: (text: string) => wasm && call<boolean>(wasm.vim_set_clipboard, { text }),
};
