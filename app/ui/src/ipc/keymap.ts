/**
 * Keymap IPC. Mirrors `forge-keymap` and the `keymap_*` commands in
 * `app/src-tauri/src/commands/keymap.rs`. In browser mode the same Rust code runs as
 * WebAssembly (`mock/keymapMock.ts`).
 */
import { invoke } from "./index";

export type KeymapId = "$default" | "macOS";
export type Platform = "mac" | "other";
/** Where an action may fire: "editor" actions only while a code editor has focus. */
export type ActionContext = "any" | "editor" | "toolwindow" | "diff";
/** Where keyboard focus currently is. */
export type FocusContext = "editor" | "input" | "terminal" | "toolwindow" | "diff" | "global";

export interface ActionDef {
  id: string;
  title: string;
  /** Tree path in the keymap settings, e.g. "Main Menu/Navigate". */
  group: string;
  context: ActionContext;
  native: boolean;
  description?: string;
}

/** An action with its shortcuts in the active keymap. */
export interface ActionInfo extends ActionDef {
  /** Shortcut strings as stored ("Ctrl+Shift+A", "Ctrl+K Ctrl+C", "Double Shift"). */
  shortcuts: string[];
  /** The same shortcuts formatted for the keymap's platform ("Ctrl+Shift+A" / "⇧⌘A"). */
  display: string[];
  modified: boolean;
  /** Other command ids that implement the action. */
  aliases?: string[];
}

export interface KeymapSnapshot {
  keymap: KeymapId;
  /** The `keymap.active` setting: "auto", "$default" or "macOS". */
  selected: string;
  platform: Platform;
  keymaps: { id: KeymapId; title: string }[];
  groupOrder: string[];
  modifiedCount: number;
  actions: ActionInfo[];
}

/** Action contributed by a feature or plugin, with optional default shortcuts per keymap. */
export interface ActionRegistration extends Omit<ActionDef, "context" | "native"> {
  context?: ActionContext;
  native?: boolean;
  shortcuts?: Partial<Record<KeymapId, string[]>>;
}

export interface CommandInfo {
  id: string;
  title: string;
  category?: string;
}

export type KeymapChange =
  | { op: "selectKeymap"; keymap: string }
  | { op: "set"; actionId: string; shortcuts: string[] }
  | { op: "add"; actionId: string; shortcut: string; removeConflicts?: boolean }
  | { op: "remove"; actionId: string; shortcut: string }
  | { op: "reset"; actionId: string }
  | { op: "resetAll" };

export interface ConflictInfo {
  actionId: string;
  title: string;
  group: string;
  /** "same": identical shortcut; "prefix": one is the first keystroke of the other's chord. */
  kind: "same" | "prefix";
  display: string;
}

export interface ShortcutCheck {
  valid: boolean;
  canonical?: string;
  display: string;
  conflicts: ConflictInfo[];
}

/** A key event as forwarded to the resolver (DOM field names). */
export interface KeyInput {
  type: "keydown" | "keyup" | "interrupt" | "reset";
  key?: string;
  code?: string;
  ctrlKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
  metaKey?: boolean;
  repeat?: boolean;
  focus?: FocusContext;
  /** Event time in milliseconds. */
  time?: number;
}

export interface Resolution {
  /** Whether the event belongs to the keymap (call `preventDefault`). */
  consumed: boolean;
  action?: string;
  pending?: "chord" | "doubleTap";
}

/** An entry of a main menu definition (`MenuEntry` in `forge-keymap`). */
export type MainMenuEntry =
  /** `label` replaces the action title, `_` marks its mnemonic ("Save _All"). */
  | { type: "action"; id: string; label?: string; mnemonic?: string; checkable?: boolean }
  | { type: "group"; label: string; items: MainMenuEntry[] }
  | { type: "separator" }
  | { type: "dynamic"; source: string };

/** Entries added to a main menu path ("Tools", "View/Tool Windows"). */
export interface MainMenuContribution {
  path: string;
  items: MainMenuEntry[];
  /** Insert before / after the entry with this action id or submenu label. */
  before?: string;
  after?: string;
}

/** An item of a dynamic list (recent projects, tool windows). */
export interface MainMenuDynamicItem {
  key: string;
  label: string;
  /** Action whose shortcut is shown. */
  action?: string;
  detail?: string;
  enabled?: boolean;
  checked?: boolean;
}

export interface MainMenuRequest {
  contributions: MainMenuContribution[];
  dynamic: Record<string, MainMenuDynamicItem[]>;
  /** Checkable actions that are on. */
  checked: string[];
  git: boolean;
}

/** A resolved menu item. */
export interface MainMenuItem {
  kind: "action" | "submenu" | "separator";
  label?: string;
  /** Character index of the mnemonic in `label`. */
  mnemonic?: number;
  action?: string;
  /** Key of a dynamic item. */
  key?: string;
  shortcut?: string;
  detail?: string;
  enabled: boolean;
  checkable?: boolean;
  checked?: boolean;
  items?: MainMenuItem[];
}

export interface MainMenu {
  menus: MainMenuItem[];
}

export const keymapApi = {
  actions: () => invoke<KeymapSnapshot>("keymap_actions"),
  registerActions: (args: { actions?: ActionRegistration[]; commands?: CommandInfo[] }) =>
    invoke<KeymapSnapshot>("keymap_register_actions", { ...args }),
  setOverride: (change: KeymapChange) => invoke<KeymapSnapshot>("keymap_set_override", { change }),
  conflicts: (shortcut: string, actionId?: string) =>
    invoke<ShortcutCheck>("keymap_conflicts", { shortcut, actionId }),
  findByShortcut: (shortcut: string) => invoke<string[]>("keymap_find_by_shortcut", { shortcut }),
  resolve: (event: KeyInput) => invoke<Resolution>("keymap_resolve", { event }),
  mainMenu: (request: MainMenuRequest) => invoke<MainMenu>("keymap_main_menu", { request }),
  importLegacy: (active: string | null, overrides: string | null) =>
    invoke<string[]>("keymap_import_legacy", { active, overrides }),
};
