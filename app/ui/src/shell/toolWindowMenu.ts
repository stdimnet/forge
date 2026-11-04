/**
 * The placement entries of a tool window, shared by the stripe button's context menu and the
 * Options (⋮) menu of the tool window header, like PHPStorm's New UI.
 */
import { item, separator, showContextMenu, submenu, type MenuEntry } from "@/features/contextMenu/state";
import { hideToolWindow, moveToolWindowTo, removeFromSidebar, setToolWindowViewMode, slotOfToolWindow, viewModeOf } from "@/state/layout";
import { SLOT_LABELS, VIEW_MODE_LABELS } from "@/state/toolWindowLayout";
import { t, tOr } from "@/i18n";

/** View Mode › Dock Pinned / Dock Unpinned / Undock / Float / Window, like PHPStorm. */
export function viewModeEntries(id: string): MenuEntry[] {
  const current = viewModeOf(id);
  return VIEW_MODE_LABELS.map(([mode, label]) => item(tOr(`toolwindow.viewMode.${mode}`, label), () => setToolWindowViewMode(id, mode), undefined, { checked: current === mode }));
}

function placementEntries(id: string): MenuEntry[] {
  const current = slotOfToolWindow(id);
  return [
    submenu(t("toolwindow.viewMode"), viewModeEntries(id)),
    submenu(
      t("toolwindow.moveTo"),
      SLOT_LABELS.filter(([slot]) => slot !== current).map(([slot, label]) => item(tOr(`toolwindow.slot.${slot}`, label), () => moveToolWindowTo(id, slot))),
    ),
    item(t("toolwindow.removeFromSidebar"), () => removeFromSidebar(id)),
  ];
}

/** Right click on a stripe button. */
export function openStripeMenu(id: string, x: number, y: number): void {
  showContextMenu(placementEntries(id), x, y);
}

/** The ⋮ (Options) button of a tool window header. */
export function openToolWindowOptions(id: string, x: number, y: number): void {
  showContextMenu([...placementEntries(id), separator, item(t("toolwindow.hide"), () => hideToolWindow(id), "HideActiveWindow")], x, y);
}
