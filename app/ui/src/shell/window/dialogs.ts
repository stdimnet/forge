/**
 * Dialogs that open as their own OS windows, like PHPStorm's Settings or Run/Debug
 * Configurations.
 *
 *   registerDialog({ id: "settings", title: "Settings", width: 900, height: 640, component: SettingsContent });
 *   const result = await openDialogWindow("settings", { page: "keymap" });
 *
 * - Desktop app: the backend creates a window `dialog-<id>` owned by the calling window
 *   (centered on it, remembered geometry, the owner is disabled while it is open). The window
 *   loads the same UI with `?dialog=<id>` and renders only that dialog (`DialogWindowApp`).
 *   Dialogs keep their data in the Rust backend, so whatever they save is visible to every
 *   window; `onClosed` / `onMessage` run in the opener to refresh what it shows.
 * - Browser mode: the dialog renders in-page over a backdrop (`DialogHost`), inside the same
 *   `WindowFrame` as the window, so it looks the same.
 *
 * Small popups (Find Action, completion, context menus, balloons) stay in-page and do not use
 * this API.
 */
import { createSignal, type Component } from "solid-js";
import { isTauri } from "@/ipc";
import { windowApi } from "@/ipc/window";
import { dialogBoot, windowLabel } from "./mode";

export interface DialogComponentProps<P> {
  props: P;
}

export interface DialogDef<P = Record<string, unknown>, R = unknown> {
  id: string;
  title: string | ((props: P) => string);
  /** Default size in CSS pixels (the window remembers the user's size per dialog). */
  width: number;
  height: number;
  minWidth?: number;
  minHeight?: number;
  /** The dialog's content: everything below the title bar (body and OK / Cancel buttons). */
  component: Component<DialogComponentProps<P>>;
  /** Runs where the dialog renders, before it mounts: load the data it shows. */
  prepare?: (props: P) => Promise<unknown> | unknown;
  /** Runs in the opening window after the dialog closed; `result` is `null` when cancelled. */
  onClosed?: (result: R | null, props: P) => void;
  /** Runs in the opening window for `notifyDialogOpener` messages (e.g. after Apply). */
  onMessage?: (message: unknown) => void;
}

// Definitions are heterogeneous; callers get typed access through `registerDialog`.
type AnyDef = DialogDef<any, any>;

const defs = new Map<string, AnyDef>();

export function registerDialog<P, R = unknown>(def: DialogDef<P, R>): void {
  defs.set(def.id, def as AnyDef);
}

export const dialogDef = (id: string): AnyDef | undefined => defs.get(id);

export function dialogTitle(def: AnyDef, props: unknown): string {
  return typeof def.title === "function" ? def.title(props) : def.title;
}

export interface InPageDialog {
  id: string;
  props: unknown;
}

const [inPage, setInPage] = createSignal<InPageDialog[]>([]);
const [openIds, setOpenIds] = createSignal<string[]>([]);

/** Dialogs rendered in-page in this window (browser mode), in stacking order. */
export const inPageDialogs = inPage;

interface Pending {
  props: unknown;
  promise: Promise<unknown>;
  resolve: (result: unknown) => void;
}

/** Dialogs opened from this window that have not closed yet. */
const pending = new Map<string, Pending>();

let windowMode = isTauri();

/** Open dialogs as OS windows (desktop) or in-page (browser mode, tests). */
export function setDialogWindowMode(on: boolean): void {
  windowMode = on;
}

export const usesDialogWindows = (): boolean => windowMode;

let listening: Promise<void> | null = null;

function ensureListeners(): Promise<void> {
  listening ??= (async () => {
    const me = await windowLabel();
    await windowApi.onDialogClosed((e) => e.window === me && finish(e.id, e.result ?? null));
    await windowApi.onDialogMessage((e) => {
      if (e.window === me) defs.get(e.id)?.onMessage?.(e.message);
    });
  })();
  return listening;
}

function finish(id: string, result: unknown): void {
  const p = pending.get(id);
  if (!p) return;
  pending.delete(id);
  setOpenIds((ids) => ids.filter((x) => x !== id));
  setInPage((list) => list.filter((d) => d.id !== id));
  try {
    defs.get(id)?.onClosed?.(result, p.props);
  } finally {
    p.resolve(result);
  }
}

/**
 * Open a registered dialog; resolves with its result when it closes (`null` = cancelled).
 * Opening a dialog that is already open focuses it and returns the same promise.
 */
export function openDialogWindow<R = unknown>(id: string, props: object = {}): Promise<R | null> {
  const def = defs.get(id);
  if (!def) return Promise.reject(new Error(`unknown dialog "${id}"`));
  // This window already is that dialog.
  if (dialogBoot()?.id === id) return Promise.resolve(null);
  const request = {
    id,
    title: dialogTitle(def, props),
    width: def.width,
    height: def.height,
    minWidth: def.minWidth,
    minHeight: def.minHeight,
    props,
  };
  const existing = pending.get(id);
  if (existing) {
    if (windowMode) void windowApi.openDialog(request).catch(() => undefined);
    return existing.promise as Promise<R | null>;
  }
  let resolve!: (result: unknown) => void;
  const promise = new Promise<unknown>((r) => (resolve = r));
  pending.set(id, { props, promise, resolve });
  setOpenIds((ids) => [...ids, id]);
  if (windowMode) {
    void ensureListeners()
      .then(() => windowApi.openDialog(request))
      .catch((e) => {
        console.error(`dialog "${id}" could not be opened`, e);
        finish(id, null);
      });
  } else {
    setInPage((list) => [...list, { id, props }]);
  }
  return promise as Promise<R | null>;
}

/** Close a dialog with a result (OK) or `null` (Cancel). Call it from the dialog itself. */
export function closeDialog(id: string, result: unknown = null): void {
  if (dialogBoot()?.id === id) {
    disposeHandlers.get(id)?.();
    if (isTauri()) void windowApi.closeDialog(result);
    return;
  }
  // Dialog windows close themselves; in-page dialogs close here.
  if (!windowMode) finish(id, result);
}

const cancelHandlers = new Map<string, () => void>();
const disposeHandlers = new Map<string, () => void>();

/**
 * What the title bar close button, Escape, a click on the backdrop and the OS close request
 * (Alt+F4) do for a dialog. Without a handler they cancel it with `closeDialog(id, null)`.
 * A dialog that cannot close while it is busy registers a handler that ignores the request.
 */
export function onDialogCancel(id: string, handler: () => void): () => void {
  cancelHandlers.set(id, handler);
  return () => {
    if (cancelHandlers.get(id) === handler) cancelHandlers.delete(id);
  };
}

/** Run before a dialog window closes (its Solid cleanups do not run when a window closes). */
export function onDialogDispose(id: string, handler: () => void): () => void {
  disposeHandlers.set(id, handler);
  return () => {
    if (disposeHandlers.get(id) === handler) disposeHandlers.delete(id);
  };
}

/** Cancel a dialog the way its Cancel button would. */
export function requestCloseDialog(id: string): void {
  const handler = cancelHandlers.get(id);
  if (handler) handler();
  else closeDialog(id, null);
}

/** Tell the window that opened the dialog about a change (its `onMessage` runs there). */
export function notifyDialogOpener(id: string, message: unknown): void {
  if (dialogBoot()?.id === id) {
    if (isTauri()) void windowApi.messageParent(message).catch(() => undefined);
    return;
  }
  defs.get(id)?.onMessage?.(message);
}

/** The dialog is open: this window shows it, or this window opened it. */
export const isDialogOpen = (id: string): boolean => dialogBoot()?.id === id || openIds().includes(id);
