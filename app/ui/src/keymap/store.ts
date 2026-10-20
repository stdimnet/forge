/**
 * Reactive keymap state: the selected keymap, user overrides layered on top of the
 * PHPStorm defaults, and the action catalog (PHPStorm actions + plugin actions).
 *
 * The selected keymap and the overrides are user settings (`keymap.active`,
 * `keymap.overrides.<keymap>`, declared in `plugins/core.toml`); see `encodeOverrides`
 * for the stored format.
 */
import { createEffect, createMemo, createRoot, createSignal, on } from "solid-js";
import { commands } from "@/state/commands";
import { getSetting, saveSetting, setLocalSetting, settingsState } from "@/state/settings";
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

export type Overrides = Partial<Record<KeymapId, Record<string, string[]>>>;

/** Settings keys. */
export const KEYMAP_SETTING = "keymap.active";
export const overridesSetting = (km: KeymapId) => `keymap.overrides.${km === "macOS" ? "macOS" : "default"}`;
const KEYMAPS: KeymapId[] = ["$default", "macOS"];

/**
 * Overrides of one keymap as a settings list: one entry per action, the action id followed
 * by its shortcuts, tab separated (shortcuts may contain commas and spaces). An entry
 * with only the id removes all shortcuts of the action.
 */
export function encodeOverrides(map: Record<string, string[]>): string[] {
  return Object.entries(map)
    .sort(([a], [b]) => a.localeCompare(b))
    .map(([id, shortcuts]) => [id, ...shortcuts].join("\t"));
}

export function decodeOverrides(list: unknown): Record<string, string[]> {
  const out: Record<string, string[]> = {};
  if (!Array.isArray(list)) return out;
  for (const entry of list) {
    if (typeof entry !== "string" || !entry) continue;
    const [id, ...shortcuts] = entry.split("\t");
    if (id) out[id] = shortcuts.filter(Boolean);
  }
  return out;
}

const defaultKeymap = (): KeymapId => (detectPlatform() === "mac" ? "macOS" : "$default");

const keymapId = (): KeymapId => {
  const v = getSetting<string>(KEYMAP_SETTING, "auto");
  return v === "$default" || v === "macOS" ? v : defaultKeymap();
};

/** Store a keymap setting: applied at once, persisted in the background. */
function persist(key: string, value: string | string[]): void {
  setLocalSetting(key, value);
  saveSetting(key, value, "user").catch((e) => console.warn(`could not save ${key}`, e));
}

const [extraActions, setExtraActions] = createSignal<ActionDef[]>([]);
const [extraDefaults, setExtraDefaults] = createSignal<Record<KeymapId, Record<string, string[]>>>({
  $default: {},
  macOS: {},
});

export { keymapId, overrides };

/** Platform whose key names the active keymap uses (macOS keymap → Cmd/⌘ symbols). */
export const keymapPlatform = (): Platform => (keymapId() === "macOS" ? "mac" : "other");

export function setKeymapId(id: KeymapId): void {
  persist(KEYMAP_SETTING, id);
}

/** User overrides of every keymap. */
const overrides = (): Overrides =>
  Object.fromEntries(KEYMAPS.map((km) => [km, decodeOverrides(getSetting<string[]>(overridesSetting(km), []))]));

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
  const before = overrides();
  for (const km of KEYMAPS) {
    const list = encodeOverrides(next[km] ?? {});
    if (JSON.stringify(list) !== JSON.stringify(encodeOverrides(before[km] ?? {}))) persist(overridesSetting(km), list);
  }
}

/** Where earlier versions kept the keymap in the browser storage. */
export const LEGACY_KEYS = { active: "forge.keymap.active", overrides: "forge.keymap.overrides" };

/** Settings to write for a keymap saved by an earlier version in localStorage. */
export function legacyKeymapChanges(active: string | null, rawOverrides: string | null): Record<string, string | string[]> {
  const out: Record<string, string | string[]> = {};
  try {
    const id = active ? (JSON.parse(active) as unknown) : null;
    if (id === "$default" || id === "macOS") out[KEYMAP_SETTING] = id;
  } catch {
    /* ignore a corrupt value */
  }
  try {
    const old = rawOverrides ? (JSON.parse(rawOverrides) as Overrides) : {};
    for (const km of KEYMAPS) {
      const map = old[km];
      if (map && Object.keys(map).length) out[overridesSetting(km)] = encodeOverrides(map);
    }
  } catch {
    /* ignore a corrupt value */
  }
  return out;
}

/** Move a keymap from localStorage into the settings backend once settings are loaded. */
export function migrateLegacyKeymap(): void {
  let active: string | null = null;
  let raw: string | null = null;
  try {
    active = localStorage.getItem(LEGACY_KEYS.active);
    raw = localStorage.getItem(LEGACY_KEYS.overrides);
  } catch {
    return;
  }
  if (active === null && raw === null) return;
  createRoot((dispose) =>
    createEffect(
      on(
        () => settingsState.loaded,
        (loaded) => {
          if (!loaded) return;
          dispose();
          const changes = Object.entries(legacyKeymapChanges(active, raw)).filter(([k]) => !(k in settingsState.user));
          void Promise.all(changes.map(([k, v]) => saveSetting(k, v, "user")))
            .then(() => {
              localStorage.removeItem(LEGACY_KEYS.active);
              localStorage.removeItem(LEGACY_KEYS.overrides);
            })
            .catch((e) => console.warn("keymap could not be migrated", e));
        },
      ),
    ),
  );
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
