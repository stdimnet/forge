/**
 * Appearance & Behavior | System Settings at runtime, like PHPStorm:
 * - Autosave: save all files when Forge loses the focus (`system.autosave.onFrameDeactivation`)
 *   and after the user was idle for `system.autosave.idleSeconds` (`system.autosave.onIdle`).
 * - Synchronization: when Forge gets the focus back, open files are re-read from disk and the
 *   project view and Git status refresh (`system.syncExternalChanges`).
 * - Exit: "Confirm Exit" before the main window closes (`system.confirmExit`), then every
 *   file is saved; Cmd+Q (`exitApp`) does the same for all project windows. A file that
 *   cannot be saved keeps the window open unless the user discards it (`./unsaved.ts`).
 *   Installed by the main window only (`installSystemBehaviour`).
 */
import { createEffect, createRoot, onCleanup } from "solid-js";
import { isTauri, listen } from "@/ipc";
import { windowApi } from "@/ipc/window";
import { editor, isDirty, saveAll } from "@/state/editor";
import { dispatchFsChanges } from "@/state/fsWatch";
import { getSetting, saveSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { askConfirmWithCheckbox } from "@/features/contextMenu/dialogs";
import { showBalloon } from "@/keymap";
import { t as tr } from "@/i18n";
import { saveBeforeLeaving } from "./unsaved";

export { saveBeforeLeaving };

/** Save every modified file; failures are reported, never thrown. */
export async function autosave(): Promise<void> {
  if (!editor.tabs.some(isDirty)) return;
  try {
    await saveAll();
  } catch (e) {
    showBalloon(tr("tools.system.cannotSaveFiles"), String(e instanceof Error ? e.message : e), 6000);
  }
}

/** Re-check open files and the repository after Forge was in the background. */
export function synchronize(): void {
  if (!workspace()) return;
  const open = editor.tabs.filter((t) => !t.view).map((t) => t.path);
  dispatchFsChanges({ changed: open, removed: [], git: true });
}

/** Events that count as activity for "Save files if the IDE is idle". */
const ACTIVITY = ["keydown", "mousedown", "wheel"] as const;
/** Focus changes closer together than this are one switch (e.g. a menu taking focus). */
const SYNC_THROTTLE_MS = 2000;

let installed = false;

/** Install the window behaviour; call once from the main window. */
export function installSystemBehaviour(): void {
  if (installed) return;
  installed = true;
  let lastSync = 0;

  window.addEventListener("blur", () => {
    if (getSetting<boolean>("system.autosave.onFrameDeactivation", true)) void autosave();
  });
  window.addEventListener("focus", () => {
    if (!getSetting<boolean>("system.syncExternalChanges", true) || Date.now() - lastSync < SYNC_THROTTLE_MS) return;
    lastSync = Date.now();
    synchronize();
  });

  createRoot(() =>
    createEffect(() => {
      if (!getSetting<boolean>("system.autosave.onIdle", false)) return;
      const ms = Math.max(1, getSetting<number>("system.autosave.idleSeconds", 15)) * 1000;
      let timer: ReturnType<typeof setTimeout> | undefined;
      const restart = () => {
        clearTimeout(timer);
        timer = setTimeout(() => void autosave(), ms);
      };
      for (const e of ACTIVITY) window.addEventListener(e, restart, true);
      restart();
      onCleanup(() => {
        clearTimeout(timer);
        for (const e of ACTIVITY) window.removeEventListener(e, restart, true);
      });
    }),
  );

  if (isTauri()) {
    void confirmExitOnClose();
    // Another window (or a second project window) asked everyone to exit: save first.
    void listen("app://quit", async () => {
      if (!(await saveBeforeLeaving("window"))) return;
      await windowApi.exit().catch(() => undefined);
    });
  }
}

/** Exit (Cmd+Q): after the "Confirm Exit" question every project window saves and closes. */
export async function exitApp(): Promise<void> {
  if (!isTauri()) return;
  if (getSetting<boolean>("system.confirmExit", true)) {
    const answer = await askConfirmWithCheckbox({
      title: "Confirm Exit",
      message: "Are you sure you want to exit Forge? All project windows will close.",
      confirm: "Exit",
      checkbox: "Do not ask me again",
    });
    if (!answer.ok) return;
    if (answer.checked) await saveSetting("system.confirmExit", false).catch(() => undefined);
  }
  await windowApi.quitAll().catch((e) => showBalloon(tr("tools.system.cannotExitForge"), String(e instanceof Error ? e.message : e), 6000));
}

/** Close this window (Cmd+W without a tab to close); the main window asks "Confirm Exit". */
export async function closeThisWindow(): Promise<void> {
  if (!isTauri()) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().close();
}

async function confirmExitOnClose(): Promise<void> {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().onCloseRequested(async (event) => {
    if (getSetting<boolean>("system.confirmExit", true)) {
      const answer = await askConfirmWithCheckbox({
        title: "Confirm Exit",
        message: "Are you sure you want to exit Forge?",
        confirm: "Exit",
        checkbox: "Do not ask me again",
      });
      if (!answer.ok) {
        event.preventDefault();
        return;
      }
      if (answer.checked) await saveSetting("system.confirmExit", false).catch(() => undefined);
    }
    if (!(await saveBeforeLeaving("window"))) event.preventDefault();
  });
}
