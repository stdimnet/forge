/**
 * Window chrome, dialog windows, full screen and IDE zoom (`app/src-tauri/src/commands/window.rs`,
 * `forge-window`).
 */
import { invoke, listen } from "./index";

export type ControlsStyle = "windows" | "macos" | "gnome" | "kde" | "linux";
export type WindowButton = "minimize" | "maximize" | "close";

export interface ButtonLayout {
  left: WindowButton[];
  right: WindowButton[];
}

/** How to draw title bars, resolved in Rust from `appearance.windowControls`. */
export interface WindowChrome {
  os: "windows" | "macos" | "linux" | "other";
  desktop: "windows" | "macos" | "gnome" | "kde" | "other";
  setting: string;
  style: ControlsStyle;
  /** The OS draws the title bars; Forge shows no window controls. */
  native: boolean;
  buttons: ButtonLayout;
  dialogButtons: ButtonLayout;
  titleAlign: "left" | "center";
}

export interface DialogWindowRequest {
  id: string;
  title: string;
  /** Default size in CSS pixels. */
  width: number;
  height: number;
  minWidth?: number;
  minHeight?: number;
  props?: unknown;
  /**
   * Modal dialogs (the default) disable their parent. Windows that are not modal (an editor
   * tab or a tool window in its own window) get a taskbar entry instead.
   */
  modal?: boolean;
}

/** Dialog events name the window they are meant for (`window`, a window label). */
export interface DialogClosedEvent {
  window: string;
  id: string;
  result: unknown;
}

export interface DialogMessageEvent {
  window: string;
  id: string;
  message: unknown;
}

export const windowApi = {
  /** Select In… › Explorer / Finder / File Manager: show a project file in the system file manager. */
  revealInFileManager: (path: string) => invoke<void>("reveal_in_file_manager", { path }),
  /** `style` overrides the setting (e.g. `?windowControls=gnome`). */
  chrome: (style?: string) => invoke<WindowChrome>("window_chrome", { style: style ?? null }),
  /** Open (or focus) a dialog window owned by the calling window. */
  openDialog: (request: DialogWindowRequest) => invoke<void>("dialog_window_open", { request }),
  /** Close the calling dialog window, reporting `result` to its parent. */
  /** Exit: every project window saves its files and closes, this one too. */
  quitAll: () => invoke<void>("app_quit_all"),
  /** Leave the process now (the answer to `app://quit`). */
  exit: () => invoke<void>("app_exit"),
  /** The dialog window has rendered; the backend shows it (it is created hidden). */
  dialogReady: () => invoke<void>("dialog_window_ready"),
  closeDialog: (result: unknown) => invoke<void>("dialog_window_close", { result: result ?? null }),
  /** Send a message from the calling dialog window to its parent. */
  messageParent: (message: unknown) => invoke<void>("dialog_window_message", { message: message ?? null }),
  /** A dialog this window opened has closed. */
  onDialogClosed: (cb: (e: DialogClosedEvent) => void) => listen<DialogClosedEvent>("dialog://closed", cb),
  onDialogMessage: (cb: (e: DialogMessageEvent) => void) => listen<DialogMessageEvent>("dialog://message", cb),
  /** View | Appearance | Enter Full Screen for the calling window. */
  setFullscreen: (on: boolean) => invoke<void>("window_set_fullscreen", { on }),
  isFullscreen: () => invoke<boolean>("window_is_fullscreen"),
  /** Zoom IDE: scale the calling window's whole UI (1 = 100 %). */
  setZoom: (factor: number) => invoke<void>("window_set_zoom", { factor }),
  /** The OS asks this dialog window to close (Alt+F4); run the dialog's Cancel path. */
  onCloseRequested: (cb: (e: { window: string }) => void) => listen<{ window: string }>("dialog://close-requested", cb),
};
