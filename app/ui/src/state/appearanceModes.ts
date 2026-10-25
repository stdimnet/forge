/**
 * View | Appearance modes, like PHPStorm:
 *
 * - Presentation Mode: tool windows, toolbar and status bar hidden, full screen, large editor
 *   font (`appearance.presentationFontSize`).
 * - Distraction Free Mode: only the editor (no tool windows, bars or editor tabs), its text
 *   centered.
 * - Zen Mode: Distraction Free Mode in full screen.
 * - Full Screen: the window covers the screen.
 *
 * Entering a mode remembers the open tool windows and whether the window was full screen;
 * leaving it restores both. One mode is active at a time; entering another first leaves the
 * current one.
 */
import { createSignal } from "solid-js";
import { windowApi } from "@/ipc/window";
import { findToolWindow } from "@/features/registry";
import { layout, setLayout } from "./layout";
import { SLOTS, type ActiveWindows } from "./toolWindowLayout";

export type AppearanceMode = "presentation" | "distractionFree" | "zen";

/** PHPStorm's names, as the View | Appearance entries show them. */
export const APPEARANCE_MODE_TITLES: Record<AppearanceMode, string> = {
  presentation: "Presentation Mode",
  distractionFree: "Distraction-Free Mode",
  zen: "Zen Mode",
};

const [mode, setMode] = createSignal<AppearanceMode | null>(null);
const [fullScreen, setFullScreenState] = createSignal(false);

/** The active mode, if any. */
export const appearanceMode = mode;
export const isFullScreen = fullScreen;

/** What leaving the mode restores. */
let saved: { active: ActiveWindows; fullScreen: boolean } | null = null;

/** Modes that also make the window full screen. */
const FULL_SCREEN: Record<AppearanceMode, boolean> = { presentation: true, distractionFree: false, zen: true };

async function applyFullScreen(on: boolean): Promise<void> {
  setFullScreenState(on);
  await windowApi.setFullscreen(on).catch((e) => console.warn("full screen is not available", e));
}

/** Enter a mode, or leave it when it is the active one. */
export async function toggleAppearanceMode(next: AppearanceMode): Promise<void> {
  if (mode() === next) return exitAppearanceMode();
  if (mode()) await exitAppearanceMode();
  saved = { active: { ...layout.active }, fullScreen: fullScreen() };
  setLayout("active", Object.fromEntries(SLOTS.map((s) => [s, null])) as ActiveWindows);
  setMode(next);
  if (FULL_SCREEN[next] && !fullScreen()) await applyFullScreen(true);
}

/** Leave the active mode: the tool windows and the full screen state come back. */
export async function exitAppearanceMode(): Promise<void> {
  if (!mode()) return;
  const before = saved;
  saved = null;
  setMode(null);
  if (!before) return;
  for (const slot of SLOTS) {
    const id = before.active[slot];
    // Windows that are gone (plugin disabled) stay closed.
    setLayout("active", slot, id && findToolWindow(id) ? id : null);
  }
  if (fullScreen() !== before.fullScreen) await applyFullScreen(before.fullScreen);
}

/** View | Appearance | Enter Full Screen / Exit Full Screen. */
export async function toggleFullScreen(): Promise<void> {
  // Presentation and Zen Mode need full screen: leaving it leaves the mode.
  const current = mode();
  if (fullScreen() && current && FULL_SCREEN[current]) {
    if (saved) saved.fullScreen = false;
    return exitAppearanceMode();
  }
  await applyFullScreen(!fullScreen());
}

/** Read the window's real state (e.g. the OS left full screen with its own shortcut). */
export async function syncFullScreen(): Promise<void> {
  const on = await windowApi.isFullscreen().catch(() => fullScreen());
  setFullScreenState(on);
}

/** For tests: leave every mode without touching the window. */
export function resetAppearanceModes(): void {
  saved = null;
  setMode(null);
  setFullScreenState(false);
}
