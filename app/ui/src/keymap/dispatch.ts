/**
 * Glue between the DOM, the keymap engine and the command registry.
 */
import { createEffect, createRoot, createSignal } from "solid-js";
import { commands, runCommand } from "@/state/commands";
import { KeymapEngine, type FocusContext } from "./engine";
import { ACTION_ALIASES } from "./phpstormKeymap";
import { actionById, bindings } from "./store";

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
}

const [balloons, setBalloons] = createSignal<Balloon[]>([]);
export { balloons };
let balloonSeq = 0;

export const dismissBalloon = (id: number) => setBalloons((b) => b.filter((x) => x.id !== id));

/** Show a small transient notification (bottom right, like PHPStorm's balloons). */
export function showBalloon(text: string, detail?: string, ms = 2600, actions?: BalloonAction[]): number {
  const id = ++balloonSeq;
  setBalloons((b) => [...b.slice(-2), { id, text, detail, actions }]);
  setTimeout(() => dismissBalloon(id), ms);
  return id;
}

/** Command id that actually implements `actionId` (the id itself or a known alias). */
export function resolveCommand(actionId: string): string | undefined {
  const cmds = commands();
  return [actionId, ...(ACTION_ALIASES[actionId] ?? [])].find((id) => cmds.has(id));
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
      showBalloon(`${actionById(actionId)?.title ?? actionId} failed`, String(err));
    }
    return;
  }
  const title = (actionById(actionId)?.title ?? actionId).replace(/…$/, "");
  showBalloon(`${title} is not available yet`, "This action will arrive with a later Forge update.");
}

/** Classify the focused element for context-sensitive bindings. */
export function focusContextOf(el: Element | null): FocusContext {
  if (!el || !(el instanceof Element)) return "global";
  if (el.closest(".xterm")) return "terminal";
  const textInput =
    el.matches("textarea, select, [contenteditable=''], [contenteditable='true']") ||
    (el.matches("input") && !/^(checkbox|radio|button|submit|range|color)$/.test((el as HTMLInputElement).type));
  if (el.closest(".cm-editor")) return textInput && !el.classList.contains("cm-content") ? "input" : "editor";
  if (textInput) return "input";
  if (el.closest(".tool-window")) return "toolwindow";
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

export const engine = new KeymapEngine();
let installed = false;

/** Attach the global key listeners. Safe to call more than once. */
export function installKeymap(target: Window = window): void {
  if (installed) return;
  installed = true;
  createRoot(() => createEffect(() => engine.setBindings(bindings())));
  const ctx = (e: Event) => focusContextOf((e.target as Element) ?? document.activeElement);
  target.addEventListener(
    "keydown",
    (e) => {
      if (suspended || e.isComposing) return;
      const res = engine.handleKeyDown(e, ctx(e));
      if (!res.consumed) return;
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
      const res = engine.handleKeyUp(e, ctx(e));
      if (res.action) void runAction(res.action);
    },
    true,
  );
  target.addEventListener("mousedown", () => engine.interrupt(), true);
  target.addEventListener("blur", () => engine.reset());
}
