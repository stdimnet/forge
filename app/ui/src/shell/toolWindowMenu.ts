/**
 * The placement entries of a tool window, shared by the stripe button's context menu and the
 * Options (⋮) menu of the tool window header, like PHPStorm's New UI.
 */
import { item, separator, showContextMenu, submenu, type MenuEntry } from "@/features/contextMenu/state";
import { hideToolWindow, moveToolWindowTo, removeFromSidebar, slotOfToolWindow } from "@/state/layout";
import { SLOT_LABELS } from "@/state/toolWindowLayout";

function placementEntries(id: string): MenuEntry[] {
  const current = slotOfToolWindow(id);
  return [
    submenu(
      "Move To",
      SLOT_LABELS.filter(([slot]) => slot !== current).map(([slot, label]) => item(label, () => moveToolWindowTo(id, slot))),
    ),
    item("Remove from Sidebar", () => removeFromSidebar(id)),
  ];
}

/** Right click on a stripe button. */
export function openStripeMenu(id: string, x: number, y: number): void {
  showContextMenu(placementEntries(id), x, y);
}

/** The ⋮ (Options) button of a tool window header. */
export function openToolWindowOptions(id: string, x: number, y: number): void {
  showContextMenu([...placementEntries(id), separator, item("Hide", () => hideToolWindow(id), "HideActiveWindow")], x, y);
}
