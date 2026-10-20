/**
 * Main menu model on the UI side. The structure, labels, mnemonics, shortcuts and enabled
 * states are computed in Rust (`forge-keymap::menu`, `keymap_main_menu`); this module only
 * collects what the request needs (plugin contributions, dynamic lists, checked toggles) and
 * maps the answer onto the context menu renderer.
 *
 *   registerMainMenuEntries("Tools", [group("_Laravel", [action("Laravel.ShowRoutes")])]);
 *   registerMainMenuDynamic("toolWindows", () => [{ key: "project", label: "Project", run }]);
 *   registerMainMenuToggle("ViewToolButtons", () => getSetting("appearance.showToolWindowBars"));
 */
import {
  keymapApi,
  type MainMenuContribution,
  type MainMenuDynamicItem,
  type MainMenuEntry,
  type MainMenuItem,
  type MainMenuRequest,
} from "@/ipc/keymap";
import type { PluginInfo } from "@/ipc/settings";
import type { ResolvedEntry, ResolvedItem } from "@/features/contextMenu/state";

export const action = (id: string, label?: string): MainMenuEntry => ({ type: "action", id, label });
export const toggle = (id: string, label?: string): MainMenuEntry => ({ type: "action", id, label, checkable: true });
export const group = (label: string, items: MainMenuEntry[]): MainMenuEntry => ({ type: "group", label, items });
export const separator: MainMenuEntry = { type: "separator" };

interface UiContribution extends MainMenuContribution {
  /** Contribute only while this is true (e.g. the project uses the framework). */
  when?: () => boolean;
}

const contributions: UiContribution[] = [];

/** Add entries to a main menu path ("Tools", "View/Tool Windows"); missing submenus are created. */
export function registerMainMenuEntries(
  path: string,
  items: MainMenuEntry[],
  options: { before?: string; after?: string; when?: () => boolean } = {},
): void {
  contributions.push({ path, items, ...options });
}

/** A dynamic item with the code that runs when it is picked. */
export interface DynamicMenuItem extends MainMenuDynamicItem {
  run?: () => void;
}

type DynamicProvider = () => DynamicMenuItem[] | Promise<DynamicMenuItem[]>;
const providers = new Map<string, DynamicProvider>();

/** Supply the items of a dynamic list (`recentProjects`, `toolWindows`) when the menu opens. */
export function registerMainMenuDynamic(source: string, provider: DynamicProvider): void {
  providers.set(source, provider);
}

const toggles = new Map<string, () => boolean>();

/** Report whether a checkable action (View | Appearance | Status Bar, …) is on. */
export function registerMainMenuToggle(actionId: string, isOn: () => boolean): void {
  toggles.set(actionId, isOn);
}

/** Main menu contributions of the given plugins' manifests (`[[contributions.mainMenu]]`). */
export function manifestContributions(plugins: PluginInfo[]): MainMenuContribution[] {
  return plugins.flatMap((p) => p.contributions?.mainMenu ?? []);
}

export interface MenuRequestBuild {
  request: MainMenuRequest;
  /** Dynamic item key → code to run. */
  handlers: Map<string, () => void>;
}

/** Collect everything the Rust side needs to resolve the menu. */
export async function buildRequest(opts: { git: boolean; plugins: PluginInfo[] }): Promise<MenuRequestBuild> {
  const handlers = new Map<string, () => void>();
  const dynamic: Record<string, MainMenuDynamicItem[]> = {};
  await Promise.all(
    [...providers].map(async ([source, provider]) => {
      let items: DynamicMenuItem[] = [];
      try {
        items = await provider();
      } catch (e) {
        console.warn(`main menu: ${source} not available`, e);
      }
      dynamic[source] = items.map(({ run, ...item }) => {
        const key = `${source}:${item.key}`;
        if (run) handlers.set(key, run);
        return { ...item, key };
      });
    }),
  );
  const checked = [...toggles].filter(([, on]) => safely(on)).map(([id]) => id);
  const ui = contributions
    .filter((c) => !c.when || safely(c.when))
    .map(({ when: _when, ...c }) => c);
  return {
    request: { contributions: [...manifestContributions(opts.plugins), ...ui], dynamic, checked, git: opts.git },
    handlers,
  };
}

function safely(fn: () => boolean): boolean {
  try {
    return fn();
  } catch {
    return false;
  }
}

/** Map resolved Rust items onto the context menu renderer's entries. */
export function toResolved(items: MainMenuItem[], handlers: Map<string, () => void>): ResolvedEntry[] {
  return items.map((item): ResolvedEntry => {
    if (item.kind === "separator") return "separator";
    const base: ResolvedItem = {
      kind: item.kind === "submenu" ? "submenu" : "action",
      label: item.label ?? "",
      mnemonic: item.mnemonic,
      disabled: !item.enabled,
    };
    if (item.kind === "submenu") return { ...base, items: toResolved(item.items ?? [], handlers) };
    return {
      ...base,
      action: item.action,
      run: item.key ? handlers.get(item.key) : undefined,
      shortcut: item.shortcut,
      detail: item.detail,
      checkable: item.checkable,
      checked: item.checked,
      // A dynamic item without a handler and without an action has nothing to run.
      ...(item.key && !handlers.has(item.key) && !item.action ? { disabled: true } : {}),
    };
  });
}

/** Resolve the menu: one submenu per top-level menu (File, Edit, …). */
export async function loadMainMenu(opts: { git: boolean; plugins: PluginInfo[] }): Promise<ResolvedItem[]> {
  const { request, handlers } = await buildRequest(opts);
  const menu = await keymapApi.mainMenu(request);
  return toResolved(menu.menus, handlers) as ResolvedItem[];
}
