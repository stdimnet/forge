/**
 * Where tool windows sit, like PHPStorm's New UI: every anchor (left, right, bottom) has a
 * primary and a split group. The left stripe shows Left Top, Left Bottom and, at its foot,
 * Bottom Left; the right stripe shows Right Top, Right Bottom and, at its foot, Bottom Right.
 * The two windows open in the groups of one anchor share its area (stacked on the sides,
 * side by side at the bottom).
 *
 * Pure functions over the registration defaults and the user's placements (which are saved
 * with the project's layout, see `forge_core::project_state::ToolWindowPlacement`). The
 * workbench store (`state/layout.ts`) applies them.
 */
import type { FloatingBounds, LayoutAnchor, LayoutSlot, ToolWindowPlacement, ViewMode } from "@/ipc/projectState";

export type { FloatingBounds, LayoutSlot as ToolWindowSlot, ToolWindowPlacement, ViewMode };
type Slot = LayoutSlot;

export const SLOTS: Slot[] = ["left", "leftSplit", "right", "rightSplit", "bottom", "bottomSplit"];

/** PHPStorm's names of the slots, in the order of its Move To menu. */
export const SLOT_LABELS: [Slot, string][] = [
  ["left", "Left Top"],
  ["leftSplit", "Left Bottom"],
  ["right", "Right Top"],
  ["rightSplit", "Right Bottom"],
  ["bottom", "Bottom Left"],
  ["bottomSplit", "Bottom Right"],
];

/** The slots each stripe shows: top part first, then the group at its foot. */
export const STRIPE_SLOTS: Record<"left" | "right", { top: Slot[]; foot: Slot }> = {
  left: { top: ["left", "leftSplit"], foot: "bottom" },
  right: { top: ["right", "rightSplit"], foot: "bottomSplit" },
};

export const slotFor = (anchor: LayoutAnchor, split = false): Slot => (split ? `${anchor}Split` : anchor) as Slot;
export const anchorOf = (slot: Slot): LayoutAnchor => slot.replace("Split", "") as LayoutAnchor;
export const isSplitSlot = (slot: Slot): boolean => slot.endsWith("Split");
export const isSlot = (value: string): value is Slot => (SLOTS as string[]).includes(value);

/** What a tool window's registration says about its place. */
export interface ToolWindowDefault {
  id: string;
  anchor: LayoutAnchor;
  /** Starts in the split group (Left Bottom, Right Bottom, Bottom Right). */
  split?: boolean;
  order: number;
}

export type Placements = Record<string, ToolWindowPlacement>;

/** Where a tool window sits: the user's placement, else its registration default. */
export function placementOf(def: ToolWindowDefault, placements: Placements): ToolWindowPlacement {
  return placements[def.id] ?? { slot: slotFor(def.anchor, def.split), order: def.order, removed: false };
}

/** The tool windows of one stripe group in order, without the ones removed from the sidebar. */
export function groupIds(windows: ToolWindowDefault[], placements: Placements, slot: Slot): string[] {
  return windows
    .map((w, i) => ({ w, p: placementOf(w, placements), i }))
    .filter(({ p }) => p.slot === slot && !p.removed)
    .sort((a, b) => a.p.order - b.p.order || a.w.order - b.w.order || a.i - b.i)
    .map(({ w }) => w.id);
}

/** Order step between neighbours, so a later default can still slot in between. */
const STEP = 10;

/**
 * Move a tool window into `slot`, in front of the window `before` (default or unknown: at the
 * end). The windows of the target group get explicit orders; moving puts a window that was
 * removed from the sidebar back on it.
 */
export function moveToolWindow(
  windows: ToolWindowDefault[],
  placements: Placements,
  id: string,
  slot: Slot,
  before?: string,
): Placements {
  if (!windows.some((w) => w.id === id)) return placements;
  const group = groupIds(windows, placements, slot).filter((g) => g !== id);
  const at = before === undefined ? -1 : group.indexOf(before);
  group.splice(at < 0 ? group.length : at, 0, id);
  const next: Placements = { ...placements };
  group.forEach((g, i) => {
    const def = windows.find((w) => w.id === g)!;
    next[g] = { ...placementOf(def, placements), slot, order: (i + 1) * STEP, removed: false };
  });
  return next;
}

/** Remove a tool window from the sidebar, or put it back. */
export function setRemoved(windows: ToolWindowDefault[], placements: Placements, id: string, removed: boolean): Placements {
  const def = windows.find((w) => w.id === id);
  if (!def) return placements;
  const current = placementOf(def, placements);
  if (current.removed === removed) return placements;
  return { ...placements, [id]: { ...current, removed } };
}

/** PHPStorm's View Mode submenu, in its order. */
export const VIEW_MODE_LABELS: [ViewMode, string][] = [
  ["dockPinned", "Dock Pinned"],
  ["dockUnpinned", "Dock Unpinned"],
  ["undock", "Undock"],
  ["float", "Float"],
  ["window", "Window"],
];

/** How a tool window shows when it is open. */
export const modeOf = (def: ToolWindowDefault, placements: Placements): ViewMode => placementOf(def, placements).mode ?? "dockPinned";

/** Docked modes take room at their place; the others show above the IDE or in a window. */
export const isDockedMode = (mode: ViewMode): boolean => mode === "dockPinned" || mode === "dockUnpinned";

/** Modes that hide the window when another part of the IDE gets the focus. */
export const hidesOnFocusLoss = (mode: ViewMode): boolean => mode === "dockUnpinned" || mode === "undock";

/** Set the View Mode of a tool window (its place and order stay). */
export function setViewMode(windows: ToolWindowDefault[], placements: Placements, id: string, mode: ViewMode): Placements {
  const def = windows.find((w) => w.id === id);
  if (!def) return placements;
  const { mode: _old, ...current } = placementOf(def, placements);
  return { ...placements, [id]: mode === "dockPinned" ? current : { ...current, mode } };
}

/** Remember where a floating tool window is. */
export function setFloatingBounds(windows: ToolWindowDefault[], placements: Placements, id: string, bounds: FloatingBounds): Placements {
  const def = windows.find((w) => w.id === id);
  if (!def) return placements;
  const rounded = {
    x: Math.round(bounds.x),
    y: Math.round(bounds.y),
    width: Math.round(bounds.width),
    height: Math.round(bounds.height),
  };
  return { ...placements, [id]: { ...placementOf(def, placements), floating: rounded } };
}

/**
 * Where a floating window opens the first time: over its anchor's side of the IDE window
 * (`area` is the size of the IDE window).
 */
export function defaultFloatingBounds(anchor: LayoutAnchor, area: { width: number; height: number }): FloatingBounds {
  const width = Math.min(anchor === "bottom" ? 720 : 360, Math.max(240, area.width - 80));
  const height = Math.min(anchor === "bottom" ? 300 : 480, Math.max(160, area.height - 120));
  const x = anchor === "right" ? area.width - width - 60 : anchor === "left" ? 60 : Math.round((area.width - width) / 2);
  const y = anchor === "bottom" ? area.height - height - 60 : 80;
  return { x: Math.max(0, x), y: Math.max(0, y), width, height };
}

/** Keep a floating window reachable: at least `grip` pixels of its title stay inside the area. */
export function clampFloating(bounds: FloatingBounds, area: { width: number; height: number }, grip = 48): FloatingBounds {
  const width = Math.max(160, Math.min(bounds.width, area.width));
  const height = Math.max(80, Math.min(bounds.height, area.height));
  return {
    width,
    height,
    x: Math.min(Math.max(bounds.x, grip - width), area.width - grip),
    y: Math.min(Math.max(bounds.y, 0), area.height - grip),
  };
}

/** Open tool window per slot. */
export type ActiveWindows = Record<Slot, string | null>;

/** The slot that shows `id`, if it is open. */
export const openSlotOf = (active: ActiveWindows, id: string): Slot | undefined => SLOTS.find((s) => active[s] === id);

/**
 * After a window moved to `slot`: if it was open, it stays open at its new place (replacing
 * the window open there), like PHPStorm.
 */
export function moveOpenWindow(active: ActiveWindows, id: string, slot: Slot): ActiveWindows {
  const from = openSlotOf(active, id);
  if (!from || from === slot) return active;
  return { ...active, [from]: null, [slot]: id };
}

/** Where a drop at `pointer` lands in a group whose buttons span `[start, end]` (main axis). */
export function dropIndex(buttons: { start: number; end: number }[], pointer: number): number {
  const i = buttons.findIndex((b) => pointer < (b.start + b.end) / 2);
  return i < 0 ? buttons.length : i;
}

/**
 * The window a dragged one is dropped in front of, from the drop index among the shown
 * buttons `group` (which may include the dragged window); `undefined` = at the end.
 */
export function dropBefore(group: string[], id: string, index: number): string | undefined {
  const rest = group.slice(index);
  return rest[0] === id ? rest[1] : rest[0];
}
