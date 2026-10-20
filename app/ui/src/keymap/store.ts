/**
 * Reactive view of the keymap computed in Rust (`forge-keymap`): the selected keymap, every
 * action with its shortcuts (PHPStorm defaults, plugin defaults and user overrides) and the
 * display strings for the keymap's platform. Changes go to the backend, which stores them in
 * the settings (`keymap.active`, `keymap.overrides.*`) and returns the new state.
 */
import { createEffect, createMemo, createRoot, createSignal } from "solid-js";
import { listen } from "@/ipc";
import {
  keymapApi,
  type ActionInfo,
  type ActionRegistration,
  type CommandInfo,
  type KeymapChange,
  type KeymapId,
  type KeymapSnapshot,
  type Platform,
  type ShortcutCheck,
} from "@/ipc/keymap";
import { SETTINGS_CHANGED_EVENT, type SettingsChange } from "@/ipc/settings";
import { commands } from "@/state/commands";
import { formatShortcut, loadBindings } from "./core";

export type { ActionContext, ActionDef, ActionInfo, KeymapId, Platform } from "@/ipc/keymap";

const [snapshot, setSnapshot] = createSignal<KeymapSnapshot | null>(null);
export { snapshot as keymapSnapshot };

let loaded: Promise<void> | null = null;
let resolveLoaded: () => void = () => {};
const loadedOnce = new Promise<void>((r) => (resolveLoaded = r));

let requested = 0;
let adopted = 0;

/** Track a backend call; its snapshot is used unless a later call's answer came first. */
function track(request: Promise<KeymapSnapshot>, what: string): Promise<void> {
  const seq = ++requested;
  return request.then(
    (s) => {
      if (seq < adopted) return;
      adopted = seq;
      setSnapshot(s);
      loadBindings(s);
      resolveLoaded();
    },
    // Without a backend (unit tests of other features) registration simply does nothing.
    (e) => (what === "registration" ? console.debug : console.warn)(`keymap ${what} failed`, e),
  );
}

/** Fetch the keymap from the backend. */
export function refreshKeymap(): Promise<void> {
  loaded = track(keymapApi.actions(), "loading");
  return loaded;
}

/** Resolves once the first keymap snapshot has arrived. */
export const whenKeymapLoaded = (): Promise<void> => loadedOnce;

let listening = false;

/** Load the keymap and follow changes of the keymap settings. */
export async function startKeymap(): Promise<void> {
  if (!listening) {
    listening = true;
    await listen<SettingsChange>(SETTINGS_CHANGED_EVENT, (c) => {
      if (c.key.startsWith("keymap.")) void refreshKeymap();
    });
  }
  await (loaded ?? refreshKeymap());
}

// ---- Registration of plugin actions and commands (batched per tick)

let queuedActions: ActionRegistration[] = [];
let queuedCommands: CommandInfo[] | null = null;
let flushScheduled = false;

function scheduleFlush(): void {
  if (flushScheduled) return;
  flushScheduled = true;
  queueMicrotask(() => {
    flushScheduled = false;
    const actions = queuedActions;
    const cmds = queuedCommands ?? undefined;
    queuedActions = [];
    queuedCommands = null;
    void track(keymapApi.registerActions({ actions, commands: cmds }), "registration");
  });
}

/**
 * Register actions contributed by features/plugins (with optional default shortcuts per
 * keymap). PHPStorm actions are built in and need no registration.
 */
export function registerKeymapActions(actions: ActionRegistration[]): void {
  queuedActions.push(...actions);
  scheduleFlush();
}

createRoot(() =>
  createEffect(() => {
    queuedCommands = [...commands().values()].map((c) => ({ id: c.id, title: c.title, category: c.category }));
    scheduleFlush();
  }),
);

// ---- Reading

export const { allActions, actionById } = createRoot(() => {
  const allActions = (): ActionInfo[] => snapshot()?.actions ?? [];
  const byId = createMemo(() => new Map(allActions().map((a) => [a.id, a])));
  return { allActions, actionById: (id: string): ActionInfo | undefined => byId().get(id) };
});

export const keymapId = (): KeymapId => snapshot()?.keymap ?? "$default";
export const keymaps = () => snapshot()?.keymaps ?? [];
export const actionGroupOrder = () => snapshot()?.groupOrder ?? [];
export const modifiedCount = () => snapshot()?.modifiedCount ?? 0;

/** Platform whose key names the active keymap uses (macOS keymap → Cmd/⌘ symbols). */
export const keymapPlatform = (): Platform => snapshot()?.platform ?? "other";

/** Shortcut strings of an action in the active keymap. */
export const shortcutsOf = (actionId: string): string[] => actionById(actionId)?.shortcuts ?? [];

/** Display strings of an action's shortcuts. */
export const displayShortcutsOf = (actionId: string): string[] => actionById(actionId)?.display ?? [];

/** First shortcut of an action, formatted for display ("Ctrl+Shift+A" / "⇧⌘A"). */
export const shortcutLabel = (actionId: string): string => displayShortcutsOf(actionId)[0] ?? "";

/** Format any shortcut string for the active keymap. */
export const formatForKeymap = (text: string): string => formatShortcut(text, keymapPlatform());

export const isModified = (actionId: string): boolean => !!actionById(actionId)?.modified;

/** Check a shortcut for validity and conflicts before assigning it to `actionId`. */
export const checkShortcut = (shortcut: string, actionId?: string): Promise<ShortcutCheck> =>
  keymapApi.conflicts(shortcut, actionId);

/** Ids of the actions bound to `shortcut` (a single stroke also finds chords). */
export const findActionsByShortcut = (shortcut: string): Promise<string[]> => keymapApi.findByShortcut(shortcut);

// ---- Changing

function change(c: KeymapChange): Promise<void> {
  return track(keymapApi.setOverride(c), "change");
}

export const setKeymapId = (keymap: KeymapId | "auto") => change({ op: "selectKeymap", keymap });

/** Replace all shortcuts of an action in the active keymap. */
export const setActionShortcuts = (actionId: string, shortcuts: string[]) => change({ op: "set", actionId, shortcuts });

/** Add a shortcut; with `removeConflicts`, actions bound to the same shortcut lose it. */
export const addActionShortcut = (actionId: string, shortcut: string, removeConflicts = false) =>
  change({ op: "add", actionId, shortcut, removeConflicts });

export const removeActionShortcut = (actionId: string, shortcut: string) => change({ op: "remove", actionId, shortcut });

export const resetAction = (actionId: string) => change({ op: "reset", actionId });

export const resetKeymap = () => change({ op: "resetAll" });

/** Where earlier versions kept the keymap in the browser storage. */
export const LEGACY_KEYS = { active: "forge.keymap.active", overrides: "forge.keymap.overrides" };

/** Move a keymap saved by an earlier version from localStorage into the settings. */
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
  keymapApi
    .importLegacy(active, raw)
    .then(() => {
      localStorage.removeItem(LEGACY_KEYS.active);
      localStorage.removeItem(LEGACY_KEYS.overrides);
      return refreshKeymap();
    })
    .catch((e) => console.warn("keymap could not be migrated", e));
}
