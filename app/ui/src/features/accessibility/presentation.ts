/**
 * Presentation Assistant (PHPStorm: Settings | Appearance & Behavior | Appearance |
 * Presentation Assistant): the name and shortcut of every invoked action in a popup, for
 * demos and screencasts. Hooked into the command registry (`onCommandInvoked`), which every
 * keymap shortcut, menu item, Find Action entry and toolbar button goes through.
 */
import { createSignal } from "solid-js";
import type { ActionInfo, AlternateKeymap } from "@/ipc/keymap";
import type { CommandInvocation } from "@/state/commands";

export type PopupPosition = "top-left" | "top-center" | "top-right" | "bottom-left" | "bottom-center" | "bottom-right";
export type PopupSize = "small" | "medium" | "large";

export interface PresentationOptions {
  enabled: boolean;
  alternateKeymap: boolean;
}

/** What the popup shows for one action. */
export interface Presentation {
  /** Action id (or command id) shown. */
  id: string;
  title: string;
  /** Shortcuts in the active keymap ("Ctrl+Shift+A"), at most two. */
  shortcuts: string[];
  /** The alternate keymap's shortcut and its name ("⇧⌘A", "PHPStorm (macOS)"). */
  alternate?: { shortcut: string; keymap: string };
}

export interface Lookup {
  action: (id: string) => ActionInfo | undefined;
  alternate: () => AlternateKeymap | undefined;
}

/**
 * The popup content for an invocation, or null when nothing is shown: the assistant is off,
 * or the command is no keymap action (internal commands run by code).
 */
export function presentationOf(inv: CommandInvocation, opts: PresentationOptions, lookup: Lookup): Presentation | null {
  if (!opts.enabled) return null;
  const id = inv.actionId ?? inv.command.id;
  const action = lookup.action(id) ?? (inv.actionId ? lookup.action(inv.command.id) : undefined);
  if (!action && !inv.actionId) return null;
  const title = (action?.title ?? inv.command.title).replace(/(…|\.\.\.)$/, "").trim();
  if (!title) return null;
  const shortcuts = (action?.display ?? []).slice(0, 2);
  const out: Presentation = { id: action?.id ?? id, title, shortcuts };
  if (opts.alternateKeymap) {
    const alt = lookup.alternate();
    const shortcut = alt?.display[out.id]?.[0];
    if (alt && shortcut && !shortcuts.includes(shortcut)) out.alternate = { shortcut, keymap: alt.title };
  }
  return out;
}

/** Font size of the popup title per size setting (px). */
export const POPUP_FONT: Record<PopupSize, number> = { small: 14, medium: 20, large: 28 };

const [current, setCurrent] = createSignal<Presentation | null>(null);
export { current as presentation };

let timer: ReturnType<typeof setTimeout> | undefined;

/** Show a presentation for `ms` milliseconds (a new one replaces it at once). */
export function present(p: Presentation, ms: number): void {
  setCurrent(p);
  clearTimeout(timer);
  timer = setTimeout(() => setCurrent(null), Math.max(500, ms));
}

export function hidePresentation(): void {
  clearTimeout(timer);
  setCurrent(null);
}
