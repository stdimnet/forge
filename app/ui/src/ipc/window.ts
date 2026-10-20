/**
 * Window chrome and dialog windows (`app/src-tauri/src/commands/window.rs`, `forge-window`).
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
  /** `style` overrides the setting (e.g. `?windowControls=gnome`). */
  chrome: (style?: string) => invoke<WindowChrome>("window_chrome", { style: style ?? null }),
  /** Open (or focus) a dialog window owned by the calling window. */
  openDialog: (request: DialogWindowRequest) => invoke<void>("dialog_window_open", { request }),
  /** Close the calling dialog window, reporting `result` to its parent. */
  closeDialog: (result: unknown) => invoke<void>("dialog_window_close", { result: result ?? null }),
  /** Send a message from the calling dialog window to its parent. */
  messageParent: (message: unknown) => invoke<void>("dialog_window_message", { message: message ?? null }),
  /** A dialog this window opened has closed. */
  onDialogClosed: (cb: (e: DialogClosedEvent) => void) => listen<DialogClosedEvent>("dialog://closed", cb),
  onDialogMessage: (cb: (e: DialogMessageEvent) => void) => listen<DialogMessageEvent>("dialog://message", cb),
  /** The OS asks this dialog window to close (Alt+F4); run the dialog's Cancel path. */
  onCloseRequested: (cb: (e: { window: string }) => void) => listen<{ window: string }>("dialog://close-requested", cb),
};
