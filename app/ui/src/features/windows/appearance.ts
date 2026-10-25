/**
 * View | Appearance: Enter Presentation / Distraction-Free / Zen Mode, Enter Full Screen and
 * Zoom IDE In / Out / Reset, plus the auto-hiding of tool windows in the Dock Unpinned and
 * Undock view modes. The mode state lives in `state/appearanceModes.ts`; the modes show
 * through `data-appearance-mode` / `data-full-screen` on <html> (`windows.css`).
 */
import { createEffect, createRoot, on } from "solid-js";
import { isTauri } from "@/ipc";
import { windowApi } from "@/ipc/window";
import { registerCommand } from "@/state/commands";
import { getSetting, saveSetting, settingsState } from "@/state/settings";
import { hideToolWindow, layout, viewModeOf } from "@/state/layout";
import { SLOTS, hidesOnFocusLoss } from "@/state/toolWindowLayout";
import {
  APPEARANCE_MODE_TITLES,
  appearanceMode,
  isFullScreen,
  syncFullScreen,
  toggleAppearanceMode,
  toggleFullScreen,
  type AppearanceMode,
} from "@/state/appearanceModes";
import { registerMainMenuLabel } from "@/features/mainMenu/model";
import { isDialogWindow } from "@/shell/window/mode";
import { DEFAULT_ZOOM, nextZoom, zoomPercent } from "./zoom";

const MODES: [string, AppearanceMode, string][] = [
  ["TogglePresentationMode", "presentation", "_Presentation Mode"],
  ["ToggleDistractionFreeMode", "distractionFree", "_Distraction-Free Mode"],
  ["ToggleZenMode", "zen", "_Zen Mode"],
];

for (const [id, mode, label] of MODES) {
  registerCommand({
    id,
    title: `Toggle ${APPEARANCE_MODE_TITLES[mode]}`,
    category: "View",
    run: () => toggleAppearanceMode(mode),
  });
  registerMainMenuLabel(id, () => (appearanceMode() === mode ? `Exit ${label}` : undefined));
}

registerCommand({ id: "ToggleFullScreen", title: "Toggle Full Screen Mode", category: "View", run: toggleFullScreen });
registerMainMenuLabel("ToggleFullScreen", () => (isFullScreen() ? "Exit F_ull Screen" : undefined));

// ---- Zoom IDE
const ZOOM_KEY = "appearance.zoom";
const zoom = () => zoomPercent(getSetting<string>(ZOOM_KEY, String(DEFAULT_ZOOM)));
const setZoom = (percent: number) => void saveSetting(ZOOM_KEY, String(percent));

registerCommand({ id: "ZoomInIdeAction", title: "Zoom In", category: "View", run: () => setZoom(nextZoom(zoom(), 1)) });
registerCommand({ id: "ZoomOutIdeAction", title: "Zoom Out", category: "View", run: () => setZoom(nextZoom(zoom(), -1)) });
registerCommand({ id: "ResetIdeScaleAction", title: "Reset", category: "View", run: () => setZoom(DEFAULT_ZOOM) });

if (typeof document !== "undefined") {
  createRoot(() => {
    // Every window (also dialogs and detached editors) applies the zoom to itself.
    createEffect(
      on(
        () => settingsState.loaded && zoom(),
        (percent) => {
          if (percent) void windowApi.setZoom(percent / 100).catch(() => undefined);
        },
      ),
    );
    createEffect(() => {
      const root = document.documentElement;
      const mode = appearanceMode();
      if (mode) root.dataset.appearanceMode = mode;
      else delete root.dataset.appearanceMode;
      if (isFullScreen()) root.dataset.fullScreen = "";
      else delete root.dataset.fullScreen;
    });
  });
  if (isTauri() && !isDialogWindow()) void syncFullScreen();
}

// ---- Dock Unpinned / Undock: the window hides when another part of the IDE gets the focus

/** Elements that belong to a tool window although they are outside its panel. */
function belongsTo(id: string, target: Element): boolean {
  return (
    !!target.closest(`[data-tool-window-panel="${CSS.escape(id)}"]`) ||
    !!target.closest(`[data-tool-window="${CSS.escape(id)}"]`) ||
    // Its menus and popups (the ⋮ menu, completion, context menus) render in overlays.
    !!target.closest(".context-menu, .popup-backdrop, [role='menu'], [role='dialog'], [role='listbox']")
  );
}

function hideUnfocused(target: EventTarget | null): void {
  if (!(target instanceof Element)) return;
  for (const slot of SLOTS) {
    const id = layout.active[slot];
    if (id && hidesOnFocusLoss(viewModeOf(id)) && !belongsTo(id, target)) hideToolWindow(id);
  }
}

if (typeof document !== "undefined" && !isDialogWindow()) {
  // Pointer and focus changes, not keys: shortcuts stay with the keymap.
  document.addEventListener("pointerdown", (e) => hideUnfocused(e.target), true);
  document.addEventListener("focusin", (e) => hideUnfocused(e.target), true);
}
