import { createStore, reconcile } from "solid-js/store";
import { findToolWindow, toolWindows, type ToolWindow, type ToolWindowAnchor } from "@/features/registry";
import {
  SLOTS,
  anchorOf,
  groupIds,
  moveOpenWindow,
  moveToolWindow,
  openSlotOf,
  placementOf,
  setRemoved,
  type ActiveWindows,
  type Placements,
  type ToolWindowSlot,
} from "./toolWindowLayout";

export type { ToolWindowSlot };

interface LayoutState {
  /** Open tool window per slot (one per stripe group, like PHPStorm). */
  active: ActiveWindows;
  size: Record<ToolWindowAnchor, number>;
  /** Share of the first window when an anchor's area shows two (per mille). */
  splits: Record<ToolWindowAnchor, number>;
  /** Tool windows the user moved, reordered or removed from the sidebar. */
  placements: Placements;
  /** Selected tab inside tool windows that have tabs (tool window id → tab id). */
  tabs: Record<string, string>;
}

/** Size limits of the tool window areas, as `forge_core::project_state` enforces them. */
export const SIZE_LIMITS: Record<ToolWindowAnchor, [number, number]> = {
  left: [180, 900],
  right: [180, 900],
  bottom: [100, 700],
};
/** Limits of the split ratio (per mille), as `forge_core::project_state` enforces them. */
export const SPLIT_LIMITS: [number, number] = [100, 900];

/** The factory layout: Project open, every window at its registered place. */
export const DEFAULT_ACTIVE: ActiveWindows = {
  left: "project",
  leftSplit: null,
  right: null,
  rightSplit: null,
  bottom: null,
  bottomSplit: null,
};
export const DEFAULT_SIZES: Record<ToolWindowAnchor, number> = { left: 300, right: 380, bottom: 240 };
export const DEFAULT_SPLITS: Record<ToolWindowAnchor, number> = { left: 500, right: 500, bottom: 500 };

const [layout, setLayout] = createStore<LayoutState>({
  active: { ...DEFAULT_ACTIVE },
  size: { ...DEFAULT_SIZES },
  splits: { ...DEFAULT_SPLITS },
  placements: {},
  tabs: {},
});

export { layout, setLayout };

/** The slot a tool window sits in (the user's placement, else its registration). */
export function slotOfToolWindow(id: string): ToolWindowSlot {
  const tw = findToolWindow(id);
  return tw ? placementOf(tw, layout.placements).slot : "left";
}

export const isToolWindowVisible = (id: string): boolean => openSlotOf(layout.active, id) !== undefined;

/** Removed from the sidebar (Remove from Sidebar): no stripe button. */
export const isRemovedFromSidebar = (id: string): boolean => !!layout.placements[id]?.removed;

/** The stripe buttons of one group, in order (only windows the project shows). */
export function stripeToolWindows(slot: ToolWindowSlot): ToolWindow[] {
  const shown = toolWindows().filter((tw) => tw.when?.() ?? true);
  return groupIds(shown, layout.placements, slot).map((id) => shown.find((tw) => tw.id === id)!);
}

/** Show a tool window at its place; a window removed from the sidebar gets its button back. */
export function showToolWindow(id: string): void {
  if (isRemovedFromSidebar(id)) setLayout("placements", reconcile(setRemoved(toolWindows(), layout.placements, id, false)));
  setLayout("active", slotOfToolWindow(id), id);
}

export function hideToolWindow(id: string): void {
  const slot = openSlotOf(layout.active, id);
  if (slot) setLayout("active", slot, null);
}

/** Toggle a tool window: clicking the open one hides it. */
export function toggleToolWindow(id: string): void {
  if (isToolWindowVisible(id)) hideToolWindow(id);
  else showToolWindow(id);
}

/** Move To / drag and drop: put a tool window into `slot` in front of `before` (default: at the end). */
export function moveToolWindowTo(id: string, slot: ToolWindowSlot, before?: string): void {
  setLayout("placements", reconcile(moveToolWindow(toolWindows(), layout.placements, id, slot, before)));
  setLayout("active", moveOpenWindow({ ...layout.active }, id, slot));
}

/** Remove from Sidebar: hide the window and its stripe button (it stays in View | Tool Windows). */
export function removeFromSidebar(id: string): void {
  hideToolWindow(id);
  setLayout("placements", reconcile(setRemoved(toolWindows(), layout.placements, id, true)));
}

export function resizePanel(anchor: ToolWindowAnchor, size: number): void {
  const [min, max] = SIZE_LIMITS[anchor];
  setLayout("size", anchor, Math.max(min, Math.min(max, Math.round(size))));
}

/** Set the share of the first window of an anchor's area that shows two (per mille). */
export function resizeSplit(anchor: ToolWindowAnchor, ratio: number): void {
  const [min, max] = SPLIT_LIMITS;
  setLayout("splits", anchor, Math.max(min, Math.min(max, Math.round(ratio))));
}

/** The open tool windows of an anchor's area: the primary group first. */
export const openInArea = (anchor: ToolWindowAnchor): ToolWindowSlot[] =>
  SLOTS.filter((s) => anchorOf(s) === anchor && layout.active[s]);

/** The selected tab of a tool window with tabs (restored with the project's layout). */
export const toolWindowTab = (id: string, fallback: string): string => layout.tabs[id] ?? fallback;

export function setToolWindowTab(id: string, tab: string): void {
  setLayout("tabs", id, tab);
}
