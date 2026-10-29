/**
 * Glue between the DOM, the Rust key resolver and the command registry. This file only
 * captures key and mouse events, classifies the focused element and runs the returned
 * action; which key or click means what is decided by `forge-keymap`.
 */
import { createSignal } from "solid-js";
import type { FocusContext, KeyInput, Resolution } from "@/ipc/keymap";
import { keymapApi } from "@/ipc/keymap";
import { commands, runCommand } from "@/state/commands";
import { DEFAULT_GROUP, logNotification, modeOf, registerNotificationGroup, type NotificationType } from "@/state/notifications";
import { loadKeymapCore, mouseActionOf, mouseInputOf, resolveKeySync } from "./core";
import { actionById, startKeymap } from "./store";

export interface BalloonAction {
  label: string;
  run: () => void;
}

export interface Balloon {
  id: number;
  text: string;
  detail?: string;
  /** Links under the text, like PHPStorm's notification actions ("Configure"). */
  actions?: BalloonAction[];
  type?: NotificationType;
  /** Stays until closed (the group is shown as sticky balloons). */
  sticky?: boolean;
}

export interface BalloonOptions {
  /** Notification group (Settings | Appearance & Behavior | Notifications); default "Forge". */
  group?: string;
  type?: NotificationType;
  /** `false`: a transient hint that is not kept in the Notifications tool window. */
  log?: boolean;
}

const [balloons, setBalloons] = createSignal<Balloon[]>([]);
export { balloons };
let balloonSeq = 0;

export const dismissBalloon = (id: number) => setBalloons((b) => b.filter((x) => x.id !== id));

/**
 * Show a small notification (bottom right, like PHPStorm's balloons). It closes after `ms`;
 * `0` keeps it until the user closes it or picks an action. It is also logged in the
 * Notifications tool window; the group's display setting can make it sticky or log-only.
 */
export function showBalloon(text: string, detail?: string, ms = 2600, actions?: BalloonAction[], opts: BalloonOptions = {}): number {
  const id = ++balloonSeq;
  const group = opts.group ?? DEFAULT_GROUP;
  const type = opts.type ?? "info";
  if (opts.log !== false) logNotification({ group, type, title: text, content: detail, actions });
  const mode = modeOf(group);
  if (mode === "log" && opts.log !== false) return id;
  const sticky = mode === "sticky" && opts.log !== false;
  setBalloons((b) => [...b.slice(-2), { id, text, detail, actions, type, sticky: sticky || ms <= 0 }]);
  const life = sticky ? 0 : ms;
  if (life > 0) setTimeout(() => dismissBalloon(id), life);
  return id;
}

/** `showBalloon` for the notifications of one group (the group is registered for the settings). */
export function balloonsFor(group: string): typeof showBalloon {
  registerNotificationGroup(group);
  return (text, detail, ms, actions, opts) => showBalloon(text, detail, ms, actions, { group, ...opts });
}

/** Command id that actually implements `actionId` (the id itself or a known alias). */
export function resolveCommand(actionId: string): string | undefined {
  const cmds = commands();
  return [actionId, ...(actionById(actionId)?.aliases ?? [])].find((id) => cmds.has(id));
}

export const isActionAvailable = (actionId: string): boolean => resolveCommand(actionId) != null;

/**
 * Run an action by PHPStorm id. Actions nobody implements yet show a subtle
 * "not available yet" balloon instead of failing.
 */
export async function runAction(actionId: string): Promise<void> {
  const cmd = resolveCommand(actionId);
  if (cmd) {
    try {
      await runCommand(cmd);
    } catch (err) {
      console.error(`action ${actionId} failed`, err);
      showBalloon(`${actionById(actionId)?.title ?? actionId} failed`, String(err), 2600, undefined, { type: "error" });
    }
    return;
  }
  const title = (actionById(actionId)?.title ?? actionId).replace(/…$/, "");
  showBalloon(`${title} is not available yet`, "This action will arrive with a later Forge update.", 2600, undefined, { log: false });
}

/** Classify the focused element for context-sensitive bindings. */
export function focusContextOf(el: Element | null): FocusContext {
  if (!el || !(el instanceof Element)) return "global";
  if (el.closest(".xterm")) return "terminal";
  // Diff and merge windows: their own Next/Previous Difference etc., also in their editors.
  if (el.closest("[data-focus-context='diff']")) return "diff";
  const textInput =
    el.matches("textarea, select, [contenteditable=''], [contenteditable='true']") ||
    (el.matches("input") && !/^(checkbox|radio|button|submit|range|color)$/.test((el as HTMLInputElement).type));
  // Jupyter notebooks: command mode on the cells, edit mode in a cell's editor. Outputs that
  // take keys themselves (DataFrame tables) opt out with "notebook-output".
  const notebook = el.closest("[data-focus-context='notebook'], [data-focus-context='notebook-output']");
  if (notebook?.getAttribute("data-focus-context") === "notebook") {
    if (el.closest(".cm-editor")) return textInput && !el.classList.contains("cm-content") ? "input" : "notebook-editor";
    return textInput ? "input" : "notebook";
  }
  if (el.closest(".cm-editor")) return textInput && !el.classList.contains("cm-content") ? "input" : "editor";
  // Text fields that also take their tool window's actions (the commit message: Ctrl+M).
  if (textInput && el.closest("[data-focus-context='toolwindow-input']")) return "toolwindow-input";
  if (textInput) return "input";
  // Panels in the editor area that act like tool windows (data editors) opt in.
  if (el.closest(".tool-window, [data-focus-context='toolwindow']")) return "toolwindow";
  return "global";
}

let suspended = 0;

/** Pause shortcut handling (e.g. while a "press a shortcut" field records keys). */
export function suspendKeymap(): () => void {
  suspended++;
  let done = false;
  return () => {
    if (!done) suspended--;
    done = true;
  };
}

/**
 * Resolve an event: synchronously in-process when the engine is loaded, otherwise through
 * IPC (then the browser default can no longer be prevented, but the action still runs).
 */
function resolve(input: KeyInput): Resolution | null {
  const res = resolveKeySync(input);
  if (res) return res;
  keymapApi.resolve(input).then(
    (r) => r.action && void runAction(r.action),
    (e) => console.warn("key not resolved", e),
  );
  return null;
}

function keyInput(type: "keydown" | "keyup", e: KeyboardEvent): KeyInput {
  return {
    type,
    key: e.key,
    code: e.code,
    ctrlKey: e.ctrlKey,
    altKey: e.altKey,
    shiftKey: e.shiftKey,
    metaKey: e.metaKey,
    repeat: e.repeat,
    focus: focusContextOf((e.target as Element) ?? document.activeElement),
    time: e.timeStamp,
  };
}

/** Set when a feature handled the current click itself (e.g. Ctrl+Click in the editor). */
let mouseClaimed = false;

/**
 * Tell the dispatcher that the current mouse gesture (from mousedown to click) is handled,
 * so its mouse shortcut does not run a second time. Editor extensions that act on a click at
 * the clicked position call this after asking {@link editorMouseAction}.
 */
export function claimMouseGesture(): void {
  mouseClaimed = true;
}

/**
 * The action a mouse event in an editor is bound to (`GotoDeclaration` for Ctrl+Click in the
 * PHPStorm keymap). Until the engine is loaded, PHPStorm's defaults apply.
 */
export function editorMouseAction(e: MouseEvent): string | null {
  const input = { ...mouseInputOf(e), clickCount: Math.max(1, e.detail) };
  const bound = mouseActionOf(input, "editor");
  if (bound !== undefined) return bound;
  const mod = e.ctrlKey || e.metaKey;
  if (e.button === 0 && mod && e.altKey) return "GotoImplementation";
  if ((e.button === 0 && mod) || (e.button === 1 && !mod && !e.altKey && !e.shiftKey)) return "GotoDeclaration";
  return null;
}

/** Wheel turns closer together than this run a wheel shortcut once (trackpads send many). */
const WHEEL_REPEAT_MS = 120;
let lastWheel = 0;

function mouseInput(e: MouseEvent): KeyInput {
  return {
    type: "mouse",
    ...mouseInputOf(e),
    focus: focusContextOf(e.target as Element),
    time: e.timeStamp,
  };
}

/** Resolve a click or wheel turn; true when a mouse shortcut ran. */
function onMouse(e: MouseEvent): boolean {
  if (suspended) return false;
  const res = resolveKeySync(mouseInput(e));
  if (!res?.action) return false;
  e.preventDefault();
  e.stopPropagation();
  void runAction(res.action);
  return true;
}

let installed = false;

/** Attach the global key and mouse listeners. Safe to call more than once. */
export function installKeymap(target: Window = window): Promise<void> {
  if (installed) return Promise.resolve();
  installed = true;
  target.addEventListener(
    "keydown",
    (e) => {
      if (suspended || e.isComposing) return;
      const res = resolve(keyInput("keydown", e));
      if (!res?.consumed) return;
      e.preventDefault();
      e.stopPropagation();
      if (res.action) void runAction(res.action);
    },
    true,
  );
  target.addEventListener(
    "keyup",
    (e) => {
      if (suspended) return;
      const res = resolve(keyInput("keyup", e));
      if (res?.action) void runAction(res.action);
    },
    true,
  );
  target.addEventListener(
    "mousedown",
    () => {
      mouseClaimed = false;
      resolveKeySync({ type: "interrupt" });
    },
    true,
  );
  // Mouse shortcuts ("Ctrl+Button1 Click", "Button2 Click", "Alt+Wheel Up"). Clicks are
  // resolved after the element's own handlers (bubble phase), so they can claim the gesture.
  const onClick = (e: MouseEvent) => {
    if (mouseClaimed || e.defaultPrevented) return;
    onMouse(e);
  };
  target.addEventListener("click", onClick);
  target.addEventListener("auxclick", onClick);
  target.addEventListener(
    "wheel",
    (e) => {
      if (!(e.ctrlKey || e.altKey || e.shiftKey || e.metaKey) || e.deltaY === 0) return;
      if (e.timeStamp - lastWheel < WHEEL_REPEAT_MS) {
        // Part of the same turn: keep the browser from zooming or scrolling meanwhile.
        if (resolveKeySync(mouseInput(e))?.action) e.preventDefault();
        return;
      }
      if (onMouse(e)) lastWheel = e.timeStamp;
    },
    { capture: true, passive: false },
  );
  target.addEventListener("blur", () => resolveKeySync({ type: "reset" }));
  return Promise.all([loadKeymapCore(), startKeymap()]).then(() => {});
}
