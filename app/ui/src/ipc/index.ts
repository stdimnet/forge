/**
 * Thin IPC layer over Tauri. When the UI runs in a plain browser (vite dev server,
 * screenshots, tests) calls are answered by registered mock handlers instead, so every
 * feature stays usable without the Rust backend.
 */
import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";

export const isTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type MockHandler = (args: Record<string, unknown>) => unknown | Promise<unknown>;
type MockListener = (payload: unknown) => void;

const mocks = new Map<string, MockHandler>();
const mockListeners = new Map<string, Set<MockListener>>();

/** Register a browser-mode implementation of a backend command. */
export function registerMock(cmd: string, handler: MockHandler): void {
  mocks.set(cmd, handler);
}

/** Emit an event in browser mode (used by mocks to simulate streaming backends). */
export function emitMock(event: string, payload: unknown): void {
  mockListeners.get(event)?.forEach((l) => l(payload));
}

export async function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  if (isTauri()) return tauriInvoke<T>(cmd, args);
  const handler = mocks.get(cmd);
  if (!handler) throw new Error(`no backend for command "${cmd}" in browser mode`);
  return (await handler(args)) as T;
}

export async function listen<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  if (isTauri()) return tauriListen<T>(event, (e) => cb(e.payload));
  const set = mockListeners.get(event) ?? new Set();
  const l = cb as MockListener;
  set.add(l);
  mockListeners.set(event, set);
  return () => set.delete(l);
}
