/**
 * Reactive keymap state: the selected keymap, user overrides layered on top of the
 * PHPStorm defaults, and the action catalog (PHPStorm actions + plugin actions).
 *
 * Overrides are kept in localStorage for now; once a settings backend exists it can
 * replace `loadOverrides`/`saveOverrides` without touching callers.
 */
import { createMemo, createRoot, createSignal } from "solid-js";
import { commands } from "@/state/commands";
import {
  DEFAULT_KEYMAPS,
  PHPSTORM_ACTIONS,
  type ActionContext,
  type ActionDef,
  type KeymapId,
} from "./phpstormKeymap";
import { buildBindings, type Binding } from "./engine";
import {
  canonicalShortcut,
  detectPlatform,
  formatShortcutText,
  tryParseShortcut,
  shortcutId,
  type Platform,
} from "./shortcut";

const KEYMAP_KEY = "forge.keymap.active";
const OVERRIDES_KEY = "forge.keymap.overrides";

export type Overrides = Partial<Record<KeymapId, Record<string, string[]>>>;

function loadJson<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

function saveJson(key: string, value: unknown): void {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* storage unavailable: overrides still apply for this session */
  }
}

const defaultKeymap = (): KeymapId => (detectPlatform() === "mac" ? "macOS" : "$default");

const [keymapId, setKeymapIdSignal] = createSignal<KeymapId>(
  (() => {
    const v = loadJson<string | null>(KEYMAP_KEY, null);
    return v === "$default" || v === "macOS" ? v : defaultKeymap();
  })(),
);
const [overrides, setOverridesSignal] = createSignal<Overrides>(loadJson<Overrides>(OVERRIDES_KEY, {}));
const [extraActions, setExtraActions] = createSignal<ActionDef[]>([]);
const [extraDefaults, setExtraDefaults] = createSignal<Record<KeymapId, Record<string, string[]>>>({
  $default: {},
  macOS: {},
});

export { keymapId, overrides };

/** Platform whose key names the active keymap uses (macOS keymap → Cmd/⌘ symbols). */
export const keymapPlatform = (): Platform => (keymapId() === "macOS" ? "mac" : "other");

export function setKeymapId(id: KeymapId): void {
  setKeymapIdSignal(id);
  saveJson(KEYMAP_KEY, id);
}

const phpstormById = new Map(PHPSTORM_ACTIONS.map((a) => [a.id, a]));

/**
 * Register actions contributed by features/plugins (with optional default shortcuts per
 * keymap). PHPStorm actions are built in and need no registration.
 */
export function registerKeymapActions(
  actions: (Omit<ActionDef, "context" | "native"> & {
    context?: ActionContext;
    native?: boolean;
    shortcuts?: Partial<Record<KeymapId, string[]>>;
  })[],
): void {
  const defs = actions.map(({ shortcuts: _s, ...a }) => ({ context: "any" as const, native: false, ...a }));
  setExtraActions((prev) => [...prev.filter((p) => !defs.some((d) => d.id === p.id)), ...defs]);
  setExtraDefaults((prev) => {
    const next = { $default: { ...prev.$default }, macOS: { ...prev.macOS } };
    for (const a of actions) {
      for (const km of ["$default", "macOS"] as KeymapId[]) {
        const s = a.shortcuts?.[km];
        if (s) next[km][a.id] = s;
      }
    }
    return next;
  });
}

export const { allActions, actionById, effectiveShortcuts, bindings } = createRoot(() => {
  /** Every known action: PHPStorm catalog, plugin actions and bare registered commands. */
  const allActions = createMemo<ActionDef[]>(() => {
    const extra = extraActions().filter((a) => !phpstormById.has(a.id));
    const known = new Set([...phpstormById.keys(), ...extra.map((a) => a.id)]);
    const fromCommands: ActionDef[] = [...commands().values()]
      .filter((c) => !known.has(c.id))
      .map((c) => ({ id: c.id, title: c.title, group: c.category ? `Plugins/${c.category}` : "Plugins", context: "any", native: false }));
    return [...PHPSTORM_ACTIONS, ...extra, ...fromCommands];
  });
  const byId = createMemo(() => new Map(allActions().map((a) => [a.id, a])));
  const actionById = (id: string): ActionDef | undefined => byId().get(id);

  /** Action id → shortcut strings in the active keymap, overrides applied. */
  const effectiveShortcuts = createMemo<Record<string, string[]>>(() => {
    const km = keymapId();
    return { ...DEFAULT_KEYMAPS[km], ...extraDefaults()[km], ...(overrides()[km] ?? {}) };
  });

  const bindings = createMemo<Binding[]>(() =>
    buildBindings(
      Object.fromEntries(Object.entries(effectiveShortcuts()).filter(([id]) => !byId().get(id)?.native)),
      (id) => byId().get(id)?.context ?? "any",
      keymapPlatform(),
    ),
  );
  return { allActions, actionById, effectiveShortcuts, bindings };
});

/** Shortcut strings of an action in the active keymap. */
export const shortcutsOf = (actionId: string): string[] => effectiveShortcuts()[actionId] ?? [];

/** First shortcut of an action, formatted for display ("Ctrl+Shift+A" / "⇧⌘A"). */
export const shortcutLabel = (actionId: string): string => {
  const s = shortcutsOf(actionId)[0];
  return s ? formatShortcutText(s, keymapPlatform()) : "";
};

export const formatForKeymap = (text: string) => formatShortcutText(text, keymapPlatform());

function writeOverrides(next: Overrides): void {
  setOverridesSignal(next);
  saveJson(OVERRIDES_KEY, next);
}

/** Replace all shortcuts of an action in the active keymap. */
export function setActionShortcuts(actionId: string, shortcuts: string[]): void {
  const km = keymapId();
  const all = overrides();
  writeOverrides({ ...all, [km]: { ...(all[km] ?? {}), [actionId]: shortcuts } });
}

export function addActionShortcut(actionId: string, shortcut: string): void {
  const canon = canonicalShortcut(shortcut, keymapPlatform());
  const list = shortcutsOf(actionId);
  if (canon && list.some((s) => canonicalShortcut(s, keymapPlatform()) === canon)) return;
  setActionShortcuts(actionId, [...list, shortcut]);
}

export function removeActionShortcut(actionId: string, shortcut: string): void {
  const target = tryParseShortcut(shortcut, keymapPlatform());
  const id = target ? shortcutId(target) : shortcut;
  setActionShortcuts(
    actionId,
    shortcutsOf(actionId).filter((s) => {
      const p = tryParseShortcut(s, keymapPlatform());
      return (p ? shortcutId(p) : s) !== id;
    }),
  );
}

export const isModified = (actionId: string): boolean => !!overrides()[keymapId()]?.[actionId];

export function resetAction(actionId: string): void {
  const km = keymapId();
  const all = overrides();
  const { [actionId]: _drop, ...rest } = all[km] ?? {};
  writeOverrides({ ...all, [km]: rest });
}

export function resetKeymap(): void {
  const { [keymapId()]: _drop, ...rest } = overrides();
  writeOverrides(rest);
}
