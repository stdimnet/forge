/**
 * Context menu model. Entries refer to PHPStorm action ids, so labels, shortcuts and
 * availability come from the keymap; actions nobody implements yet are left out.
 */
import { createSignal } from "solid-js";
import { actionById, isActionAvailable, shortcutLabel } from "@/keymap";

export type MenuEntry =
  | { type: "action"; action: string; label?: string }
  /** A menu item that runs a closure instead of a registered action; `action` only supplies the shortcut. */
  | { type: "item"; label: string; run: () => void; action?: string }
  | { type: "submenu"; label: string; items: MenuEntry[] }
  | { type: "separator" };

export interface ResolvedItem {
  kind: "action" | "submenu";
  label: string;
  action?: string;
  run?: () => void;
  shortcut?: string;
  items?: ResolvedItem[];
}

export type ResolvedEntry = ResolvedItem | "separator";

export const action = (id: string, label?: string): MenuEntry => ({ type: "action", action: id, label });
export const item = (label: string, run: () => void, action?: string): MenuEntry => ({ type: "item", label, run, action });
export const submenu = (label: string, items: MenuEntry[]): MenuEntry => ({ type: "submenu", label, items });
export const separator: MenuEntry = { type: "separator" };

/** Drop unavailable actions and empty submenus, then collapse leading, trailing and doubled separators. */
export function resolveEntries(entries: MenuEntry[]): ResolvedEntry[] {
  const out: ResolvedEntry[] = [];
  for (const e of entries) {
    if (e.type === "separator") {
      if (out.length && out[out.length - 1] !== "separator") out.push("separator");
    } else if (e.type === "item") {
      out.push({ kind: "action", label: e.label, run: e.run, shortcut: e.action ? shortcutLabel(e.action) : "" });
    } else if (e.type === "action") {
      if (!isActionAvailable(e.action)) continue;
      out.push({
        kind: "action",
        action: e.action,
        label: e.label ?? actionById(e.action)?.title ?? e.action,
        shortcut: shortcutLabel(e.action),
      });
    } else {
      const items = resolveEntries(e.items);
      if (items.length) out.push({ kind: "submenu", label: e.label, items: items as ResolvedItem[] });
    }
  }
  while (out[out.length - 1] === "separator") out.pop();
  return out;
}

export interface OpenMenu {
  x: number;
  y: number;
  entries: ResolvedEntry[];
  /** Called after the menu closed, e.g. to give the focus back to the editor. */
  onClose?: () => void;
}

const [openMenu, setOpenMenu] = createSignal<OpenMenu | null>(null);
export { openMenu };

export function showContextMenu(entries: MenuEntry[], x: number, y: number, onClose?: () => void): void {
  const resolved = resolveEntries(entries);
  if (!resolved.length) return;
  closeContextMenu();
  setOpenMenu({ x, y, entries: resolved, onClose });
}

export function closeContextMenu(): void {
  const menu = openMenu();
  if (!menu) return;
  setOpenMenu(null);
  menu.onClose?.();
}

/** Menus contributed by features, by menu id (`editor`). Later registrations append. */
const menus = new Map<string, MenuEntry[]>();

export function registerContextMenu(id: string, entries: MenuEntry[]): void {
  menus.set(id, [...(menus.get(id) ?? []), ...entries]);
}

export const menuEntries = (id: string): MenuEntry[] => menus.get(id) ?? [];
