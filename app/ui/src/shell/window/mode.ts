/**
 * Which kind of window this UI instance runs in: the main IDE window, or a dialog window
 * opened with `openDialogWindow` (`index.html?dialog=<id>`, see `dialogs.ts`).
 */
import { isTauri } from "@/ipc";

/** What a dialog window boots with; the backend injects it as `window.__FORGE_DIALOG__`. */
export interface DialogBoot {
  id: string;
  /** Label of the window that opened the dialog. */
  parent?: string;
  props?: unknown;
}

declare global {
  interface Window {
    __FORGE_DIALOG__?: DialogBoot;
  }
}

/** The dialog this window shows, or `null` in the main window. */
export function dialogBoot(): DialogBoot | null {
  if (typeof window === "undefined" || typeof location === "undefined") return null;
  const params = new URLSearchParams(location.search);
  const id = params.get("dialog");
  if (!id) return null;
  const injected = window.__FORGE_DIALOG__;
  if (injected?.id === id) return injected;
  // Browser mode (`?dialog=settings&props={…}`): a preview of the dialog window.
  let props: unknown = {};
  try {
    props = JSON.parse(params.get("props") ?? "{}");
  } catch {
    props = {};
  }
  return { id, props };
}

/**
 * This window shows a single dialog. Background work that belongs to the IDE window
 * (indexing, polling, notifications) must not start here.
 */
export const isDialogWindow = (): boolean => dialogBoot() !== null;

let label: Promise<string> | null = null;

/** Tauri label of this window (`main`, `dialog-settings`); `main` in browser mode. */
export function windowLabel(): Promise<string> {
  label ??= isTauri()
    ? import("@tauri-apps/api/window").then((m) => m.getCurrentWindow().label)
    : Promise.resolve(dialogBoot() ? `dialog-${dialogBoot()!.id}` : "main");
  return label;
}
