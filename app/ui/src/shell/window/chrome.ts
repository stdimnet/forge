/**
 * Title bar state of this window: the chrome resolved in Rust for `appearance.windowControls`
 * (or `?windowControls=<style>` for previews and screenshots), whether the window is
 * maximized, and the window actions behind the controls.
 */
import { createResource, createRoot, createSignal } from "solid-js";
import { isTauri } from "@/ipc";
import { windowApi, type WindowChrome } from "@/ipc/window";
import { getSetting } from "@/state/settings";

/** `?windowControls=gnome` forces a style (browser mode, screenshots). */
export const forcedControls = (): string | undefined => {
  if (typeof location === "undefined") return undefined;
  return new URLSearchParams(location.search).get("windowControls") ?? undefined;
};

// Created on first use, after the mocks (browser mode) and settings are available.
let state: ReturnType<typeof createState> | null = null;

function createState() {
  return createRoot(() => {
    const [chrome] = createResource(
      () => forcedControls() ?? getSetting<string>("appearance.windowControls", "auto") ?? "auto",
      (style) => windowApi.chrome(forcedControls() ? style : undefined).catch(() => undefined),
    );
    const [maximized, setMaximized] = createSignal(false);
    return { chrome, maximized, setMaximized };
  });
}

const use = () => (state ??= createState());

/** The resolved chrome of this window (`undefined` while loading). */
export const chrome = (): WindowChrome | undefined => use().chrome();
export const maximized = (): boolean => use().maximized();

/** Forge draws this window's title bar controls. */
export const drawsControls = (): boolean => {
  const c = chrome();
  return !!c && !c.native;
};


async function current() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

let tracking = false;

/** Follow the maximized state of this window (the maximize glyph shows "restore"). */
export function trackMaximized(): void {
  if (tracking || !isTauri()) return;
  tracking = true;
  void current().then(async (w) => {
    const update = () => void w.isMaximized().then(use().setMaximized, () => undefined);
    update();
    await w.onResized(update);
  });
}

export const windowActions = {
  minimize: () => isTauri() && void current().then((w) => w.minimize()),
  toggleMaximize: () => isTauri() && void current().then((w) => w.toggleMaximize()),
  close: () => isTauri() && void current().then((w) => w.close()),
};

/** Elements in a title bar that keep their own mouse handling. */
const INTERACTIVE = "button, a, input, select, textarea, [role='button'], [data-no-drag]";

/**
 * Mouse handlers that make a title bar area move the window (drag) and maximize it on double
 * click, like the native title bar. Buttons and inputs inside keep working.
 */
export const dragRegion = {
  onMouseDown(e: MouseEvent): void {
    if (e.button !== 0 || !isTauri() || !drawsControls()) return;
    if ((e.target as Element | null)?.closest(INTERACTIVE)) return;
    // Like Tauri's `data-tauri-drag-region`: the second click of a double click maximizes
    // (a started drag would swallow the `dblclick` event on some platforms).
    if (e.detail === 2) windowActions.toggleMaximize();
    else void current().then((w) => w.startDragging());
  },
};

export type ResizeDirection = "North" | "South" | "East" | "West" | "NorthEast" | "NorthWest" | "SouthEast" | "SouthWest";

export function startResize(direction: ResizeDirection): void {
  if (!isTauri()) return;
  void current().then((w) => w.startResizeDragging(direction));
}
