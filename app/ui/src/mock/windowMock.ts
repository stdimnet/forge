/**
 * Browser-mode window chrome. The style is resolved by the same Rust code as in the desktop
 * app (`forge_window::resolve` via WebAssembly). The browser draws its own window frame, so
 * "auto" means native decorations here: the main toolbar shows no window controls unless a
 * style is chosen in Settings or forced with `?windowControls=<style>`. Dialog windows are
 * desktop-only; in browser mode dialogs render in-page (see `shell/window/dialogs.ts`).
 * Full screen uses the Fullscreen API and IDE zoom CSS `zoom`.
 */
import { registerMock } from "@/ipc";
import type { WindowChrome } from "@/ipc/window";
import { callWasm, forgeWasm } from "@/wasm";
import { mockSettingValue } from "./settingsMock";

function platform(): { os: WindowChrome["os"]; desktop: WindowChrome["desktop"] } {
  const ua = typeof navigator === "undefined" ? "" : navigator.userAgent;
  if (/Windows/i.test(ua)) return { os: "windows", desktop: "windows" };
  if (/Mac OS X|Macintosh/i.test(ua)) return { os: "macos", desktop: "macos" };
  if (/Linux|X11|BSD/i.test(ua)) return { os: "linux", desktop: "other" };
  return { os: "other", desktop: "other" };
}

registerMock("window_chrome", async (args) => {
  const forced = typeof args.style === "string" ? args.style : null;
  const setting = forced ?? String(mockSettingValue("appearance.windowControls") ?? "auto");
  const wasm = await forgeWasm();
  const chrome = await callWasm<WindowChrome>(wasm.window_chrome, { setting, ...platform() });
  return !forced && chrome.setting === "auto" ? { ...chrome, native: true } : chrome;
});

const desktopOnly = () => {
  throw new Error("dialog windows need the desktop app");
};
registerMock("dialog_window_open", desktopOnly);
registerMock("dialog_window_close", desktopOnly);
registerMock("dialog_window_message", desktopOnly);

// Full screen and IDE zoom act on the page: the Fullscreen API (where the browser allows
// it) and CSS `zoom` on <html>, which scales the whole UI like the desktop webview zoom.
let fullscreen = false;
registerMock("window_set_fullscreen", async ({ on }) => {
  fullscreen = !!on;
  try {
    if (on && !document.fullscreenElement) await document.documentElement.requestFullscreen?.();
    if (!on && document.fullscreenElement) await document.exitFullscreen?.();
  } catch {
    /* not allowed without a user gesture (or headless): the IDE still shows the mode */
  }
});
registerMock("window_is_fullscreen", () => fullscreen || !!document.fullscreenElement);
registerMock("window_set_zoom", ({ factor }) => {
  const f = Math.min(3, Math.max(0.5, Number(factor) || 1));
  const root = document.documentElement.style;
  root.setProperty("zoom", f === 1 ? "" : String(f));
  // Viewport units do not follow CSS zoom: full-height layouts divide by it (shell.css).
  root.setProperty("--ide-zoom", f === 1 ? "" : String(f));
});

/** Browser mode has no file manager to open; the request is accepted and only logged. */
export const revealedInFileManager: string[] = [];
registerMock("reveal_in_file_manager", (args) => {
  revealedInFileManager.push(String(args.path));
});
