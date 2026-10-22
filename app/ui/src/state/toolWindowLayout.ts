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
import type { LayoutAnchor, LayoutSlot, ToolWindowPlacement } from "@/ipc/projectState";

export type { LayoutSlot as ToolWindowSlot, ToolWindowPlacement };
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
