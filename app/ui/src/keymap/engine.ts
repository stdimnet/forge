/**
 * Keymap engine: turns key events into action ids. Pure and DOM-free so it can be unit
 * tested; `dispatch.ts` wires it to `window` and to the command registry.
 *
 * Handles single keystrokes, two-stroke chords ("Ctrl+K Ctrl+C"), modifier double-taps
 * ("Shift Shift") and focus contexts (editor / text input / everything else).
 */
import {
  shortcutId,
  strokeFromEvent,
  strokeId,
  modifierOfEvent,
  tryParseShortcut,
  type KeyEventLike,
  type KeyStroke,
  type Modifier,
  type Platform,
  type Shortcut,
} from "./shortcut";
import type { ActionContext } from "./phpstormKeymap";

/** Where keyboard focus currently is. */
export type FocusContext = "editor" | "input" | "terminal" | "toolwindow" | "global";

export interface Binding {
  actionId: string;
  shortcut: Shortcut;
  context: ActionContext;
}

export interface DispatchResult {
  /** Action to run, if any. */
  action?: string;
  /** Whether the event belongs to the keymap (caller should preventDefault). */
  consumed: boolean;
}

export interface EngineOptions {
  doubleTapMs?: number;
  now?: () => number;
}

/**
 * Keystrokes a text field needs for itself. In inputs these never trigger actions, so
 * Ctrl+A/C/V or word navigation keep working; plain and Shift+ keys are always left alone.
 */
const INPUT_RESERVED = new Set(
  [
    "Ctrl+A", "Ctrl+C", "Ctrl+V", "Ctrl+X", "Ctrl+Z", "Ctrl+Y", "Ctrl+Shift+Z", "Ctrl+Insert",
    "Shift+Insert", "Shift+Delete", "Ctrl+Left", "Ctrl+Right", "Ctrl+Shift+Left",
    "Ctrl+Shift+Right", "Ctrl+Backspace", "Ctrl+Delete", "Ctrl+Home", "Ctrl+End",
    "Ctrl+Shift+Home", "Ctrl+Shift+End", "Meta+A", "Meta+C", "Meta+V", "Meta+X", "Meta+Z",
    "Meta+Shift+Z", "Meta+Left", "Meta+Right", "Meta+Shift+Left", "Meta+Shift+Right",
    "Meta+Backspace", "Alt+Left", "Alt+Right", "Alt+Shift+Left", "Alt+Shift+Right",
    "Alt+Backspace", "Alt+Delete", "Meta+Up", "Meta+Down",
  ],
);

/** Whether a keystroke may trigger an action while a plain text field has focus. */
export function allowedInInput(s: KeyStroke): boolean {
  if (INPUT_RESERVED.has(strokeId(s))) return false;
  if (/^F\d+$/.test(s.key)) return true;
  if (s.key === "Escape" || s.key === "Enter" || s.key === "Tab") return false;
  return s.ctrl || s.alt || s.meta;
}

/**
 * Whether a keystroke may trigger an action while the terminal has focus. Like PHPStorm's
 * "Override IDE shortcuts", plain Ctrl+letter, Alt+letter, Escape and editing keys go to the
 * shell; function keys, Alt+digit (tool windows) and Ctrl+Shift / Ctrl+Alt combinations
 * stay with the IDE (except Ctrl+Shift+C/V, the terminal's copy and paste).
 */
export function allowedInTerminal(s: KeyStroke): boolean {
  if (/^F\d+$/.test(s.key)) return true;
  if (s.alt && !s.ctrl && /^\d$/.test(s.key)) return true;
  if (s.meta) return !["C", "V"].includes(s.key);
  if (s.ctrl && s.shift && ["C", "V"].includes(s.key)) return false;
  return s.ctrl && (s.shift || s.alt);
}

/** Whether an action with `context` may fire under `focus`. */
export function contextMatches(context: ActionContext, focus: FocusContext): boolean {
  if (context === "any") return true;
  if (context === "editor") return focus === "editor";
  return focus === "toolwindow" || focus === "terminal";
}

/** Build bindings from an action → shortcut strings map. Unparsable strings are skipped. */
export function buildBindings(
  shortcuts: Record<string, string[]>,
  contextOf: (actionId: string) => ActionContext,
  platform: Platform,
): Binding[] {
  const out: Binding[] = [];
  for (const [actionId, list] of Object.entries(shortcuts)) {
    for (const text of list) {
      const shortcut = tryParseShortcut(text, platform);
      if (shortcut) out.push({ actionId, shortcut, context: contextOf(actionId) });
    }
  }
  return out;
}

export interface Conflict {
  actionId: string;
  shortcut: Shortcut;
  /** "same": identical shortcut; "prefix": one is the first keystroke of the other's chord. */
  kind: "same" | "prefix";
}

/** Bindings that would clash with assigning `shortcut` to `actionId`. */
export function findConflicts(bindings: Binding[], shortcut: Shortcut, actionId?: string): Conflict[] {
  const id = shortcutId(shortcut);
  const strokes = shortcut.kind === "keys" ? shortcut.strokes : null;
  const first = strokes ? strokeId(strokes[0]) : null;
  const out: Conflict[] = [];
  for (const b of bindings) {
    if (b.actionId === actionId) continue;
    const bid = shortcutId(b.shortcut);
    if (bid === id) {
      out.push({ actionId: b.actionId, shortcut: b.shortcut, kind: "same" });
    } else if (first && b.shortcut.kind === "keys") {
      const bFirst = strokeId(b.shortcut.strokes[0]);
      const oneIsChord = strokes!.length !== b.shortcut.strokes.length;
      if (oneIsChord && bFirst === first) out.push({ actionId: b.actionId, shortcut: b.shortcut, kind: "prefix" });
    }
  }
  return out;
}

/** All shortcuts bound to more than one action (same context overlap only). */
export function allConflicts(bindings: Binding[]): Map<string, string[]> {
  const byId = new Map<string, Binding[]>();
  for (const b of bindings) {
    const id = shortcutId(b.shortcut);
    byId.set(id, [...(byId.get(id) ?? []), b]);
  }
  const out = new Map<string, string[]>();
  for (const [id, list] of byId) {
    const ids = [...new Set(list.map((b) => b.actionId))];
    const overlapping = list.some((a, i) =>
      list.some((b, j) => j > i && a.actionId !== b.actionId && (a.context === "any" || b.context === "any" || a.context === b.context)),
    );
    if (ids.length > 1 && overlapping) out.set(id, ids);
  }
  return out;
}

interface StrokeEntry {
  single: Binding[];
  /** second stroke id → bindings */
  chords: Map<string, Binding[]>;
}

export class KeymapEngine {
  private index = new Map<string, StrokeEntry>();
  private doubles = new Map<Modifier, Binding[]>();
  private pendingChord: { entry: StrokeEntry; focus: FocusContext } | null = null;
  private tap: { mod: Modifier; clean: boolean } | null = null;
  private lastTap: { mod: Modifier; at: number } | null = null;
  private readonly doubleTapMs: number;
  private readonly now: () => number;

  constructor(bindings: Binding[] = [], opts: EngineOptions = {}) {
    this.doubleTapMs = opts.doubleTapMs ?? 400;
    this.now = opts.now ?? (() => Date.now());
    this.setBindings(bindings);
  }

  setBindings(bindings: Binding[]): void {
    this.index.clear();
    this.doubles.clear();
    this.reset();
    for (const b of bindings) {
      if (b.shortcut.kind === "double") {
        this.doubles.set(b.shortcut.modifier, [...(this.doubles.get(b.shortcut.modifier) ?? []), b]);
        continue;
      }
      const [first, second] = b.shortcut.strokes;
      const key = strokeId(first);
      const entry: StrokeEntry = this.index.get(key) ?? { single: [], chords: new Map() };
      if (second) {
        const sk = strokeId(second);
        entry.chords.set(sk, [...(entry.chords.get(sk) ?? []), b]);
        this.index.set(key, entry);
        continue;
      }
      entry.single.push(b);
      this.index.set(key, entry);
    }
  }

  /** Whether the engine is waiting for the second keystroke of a chord. */
  get chordPending(): boolean {
    return this.pendingChord != null;
  }

  reset(): void {
    this.pendingChord = null;
    this.tap = null;
    this.lastTap = null;
  }

  private pick(list: Binding[] | undefined, focus: FocusContext): string | undefined {
    if (!list) return undefined;
    // Context-specific bindings win over global ones (e.g. Generate vs. New… on Alt+Insert).
    const matching = list.filter((b) => contextMatches(b.context, focus));
    return (matching.find((b) => b.context !== "any") ?? matching[0])?.actionId;
  }

  handleKeyDown(e: KeyEventLike, focus: FocusContext): DispatchResult {
    const mod = modifierOfEvent(e);
    if (mod) {
      // A bare modifier press starts a potential tap unless other modifiers are held.
      const others = +e.ctrlKey + +e.altKey + +e.shiftKey + +e.metaKey - 1;
      this.tap = !e.repeat && others === 0 ? { mod, clean: true } : null;
      return { consumed: false };
    }
    this.tap = null;
    this.lastTap = null;
    const stroke = strokeFromEvent(e);
    const sid = strokeId(stroke);

    if (this.pendingChord) {
      const { entry, focus: chordFocus } = this.pendingChord;
      this.pendingChord = null;
      const action = this.pick(entry.chords.get(sid), chordFocus);
      if (action) return { action, consumed: true };
      // Unknown second stroke: swallow it like PHPStorm does.
      return { consumed: true };
    }

    if (focus === "input" && !allowedInInput(stroke)) return { consumed: false };
    if (focus === "terminal" && !allowedInTerminal(stroke)) return { consumed: false };
    const entry = this.index.get(sid);
    if (!entry) return { consumed: false };
    const hasChord = [...entry.chords.values()].some((l) => l.some((b) => contextMatches(b.context, focus)));
    if (hasChord) {
      this.pendingChord = { entry, focus };
      return { consumed: true };
    }
    const action = this.pick(entry.single, focus);
    return action ? { action, consumed: true } : { consumed: false };
  }

  handleKeyUp(e: KeyEventLike, focus: FocusContext): DispatchResult {
    const mod = modifierOfEvent(e);
    if (!mod || !this.tap || this.tap.mod !== mod || !this.tap.clean) {
      this.tap = null;
      return { consumed: false };
    }
    this.tap = null;
    const t = this.now();
    if (this.lastTap && this.lastTap.mod === mod && t - this.lastTap.at <= this.doubleTapMs) {
      this.lastTap = null;
      const action = this.pick(this.doubles.get(mod), focus);
      return action ? { action, consumed: true } : { consumed: false };
    }
    this.lastTap = { mod, at: t };
    return { consumed: false };
  }

  /** Mouse clicks and focus changes break a double-tap sequence. */
  interrupt(): void {
    this.tap = null;
    this.lastTap = null;
  }
}
