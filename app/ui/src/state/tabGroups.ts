/**
 * Editor tab groups (split editors) as plain data: which tabs every group shows, the active
 * tab per group, pinned tabs, the tab limit and the closing policy. Pure functions over a
 * `TabLayout` so the rules can be tested without the UI; `state/editor.ts` keeps the
 * layout in its store and the documents (one per file, shared by all groups) next to it.
 *
 * Tab ids are file paths, view tab paths or custom tab ids (`forge:…`).
 */

export type SplitOrientation = "vertical" | "horizontal";

export interface TabGroup {
  id: number;
  /** Tab ids in display order; pinned tabs come first. */
  tabs: string[];
  active: string | null;
  /** Tab ids, most recently activated first (closing policy, tab limit). */
  recent: string[];
}

export interface TabLayout {
  groups: TabGroup[];
  activeGroup: number;
  /** `vertical`: groups side by side (Split Right); `horizontal`: stacked (Split Down). */
  orientation: SplitOrientation;
  /** Pinned tab ids (pinned in every group that shows them). */
  pinned: string[];
  nextGroupId: number;
}

export type ActivateOnClose = "left" | "right" | "recent";

export interface TabPolicy {
  /** Maximum tabs per group; 0 or less means no limit. */
  limit: number;
  /** When the limit is reached, close unmodified tabs before modified ones. */
  closeUnmodifiedFirst: boolean;
  /** Which tab becomes active when the active tab is closed. */
  activateOnClose: ActivateOnClose;
  isDirty: (id: string) => boolean;
}

export const DEFAULT_POLICY: TabPolicy = {
  limit: 0,
  closeUnmodifiedFirst: true,
  activateOnClose: "recent",
  isDirty: () => false,
};

export function newLayout(): TabLayout {
  return { groups: [{ id: 1, tabs: [], active: null, recent: [] }], activeGroup: 1, orientation: "vertical", pinned: [], nextGroupId: 2 };
}

const clone = (l: TabLayout): TabLayout => ({
  ...l,
  pinned: [...l.pinned],
  groups: l.groups.map((g) => ({ ...g, tabs: [...g.tabs], recent: [...g.recent] })),
});

export const groupById = (l: TabLayout, id: number): TabGroup | undefined => l.groups.find((g) => g.id === id);

export const activeGroupOf = (l: TabLayout): TabGroup => groupById(l, l.activeGroup) ?? l.groups[0];

/** The active tab of the active group. */
export const activeTabId = (l: TabLayout): string | null => activeGroupOf(l).active;

/** Every tab id shown in some group. */
export function openIds(l: TabLayout): Set<string> {
  return new Set(l.groups.flatMap((g) => g.tabs));
}

export const isPinned = (l: TabLayout, id: string) => l.pinned.includes(id);

/** The group that shows `id`, preferring the active group. */
export function groupOf(l: TabLayout, id: string): TabGroup | undefined {
  const active = activeGroupOf(l);
  return active.tabs.includes(id) ? active : l.groups.find((g) => g.tabs.includes(id));
}

function touch(g: TabGroup, id: string): void {
  g.active = id;
  g.recent = [id, ...g.recent.filter((r) => r !== id)];
}

/** Index after the last pinned tab of a group. */
const pinnedEnd = (l: TabLayout, g: TabGroup) => g.tabs.filter((t) => l.pinned.includes(t)).length;

/** Activate a tab of a group (and the group). */
export function activateTab(layout: TabLayout, groupId: number, id: string): TabLayout {
  const l = clone(layout);
  const g = groupById(l, groupId);
  if (!g || !g.tabs.includes(id)) return layout;
  touch(g, id);
  l.activeGroup = g.id;
  return l;
}

/** Tabs the tab limit closes in `g` (never pinned, never `keep`). */
function overLimit(l: TabLayout, g: TabGroup, keep: string, policy: TabPolicy): string[] {
  if (policy.limit <= 0 || g.tabs.length <= policy.limit) return [];
  const excess = g.tabs.length - policy.limit;
  const order = (id: string) => {
    const r = g.recent.indexOf(id);
    return r < 0 ? Number.MAX_SAFE_INTEGER : r;
  };
  // Least recently used first; with the policy, unmodified ones before modified ones.
  const candidates = g.tabs
    .filter((t) => t !== keep && !l.pinned.includes(t))
    .sort((a, b) => {
      if (policy.closeUnmodifiedFirst) {
        const d = Number(policy.isDirty(a)) - Number(policy.isDirty(b));
        if (d) return d;
      }
      return order(b) - order(a);
    });
  return candidates.slice(0, excess);
}

/**
 * Open (or activate) `id` in a group (default: the active group). A new tab goes after the
 * active tab like PHPStorm. Returns the tabs the tab limit closed.
 */
export function openTab(
  layout: TabLayout,
  id: string,
  opts: { group?: number; policy?: TabPolicy; background?: boolean } = {},
): { layout: TabLayout; evicted: string[] } {
  const l = clone(layout);
  const g = groupById(l, opts.group ?? l.activeGroup) ?? activeGroupOf(l);
  let evicted: string[] = [];
  if (!g.tabs.includes(id)) {
    const at = g.active ? g.tabs.indexOf(g.active) + 1 : g.tabs.length;
    // A pinned tab (moved from another group) joins the pinned ones.
    g.tabs.splice(l.pinned.includes(id) ? pinnedEnd(l, g) : Math.max(at, pinnedEnd(l, g)), 0, id);
    evicted = overLimit(l, g, id, opts.policy ?? DEFAULT_POLICY);
    g.tabs = g.tabs.filter((t) => !evicted.includes(t));
    g.recent = g.recent.filter((t) => !evicted.includes(t));
  }
  if (opts.background && g.active) {
    g.recent = [...g.recent.filter((r) => r !== id), id];
  } else {
    touch(g, id);
    l.activeGroup = g.id;
  }
  return { layout: l, evicted };
}

/** The tab to activate after `closing` (the active tab) leaves `g`. */
function successor(g: TabGroup, closing: string, rest: string[], policy: TabPolicy): string | null {
  if (!rest.length) return null;
  const idx = g.tabs.indexOf(closing);
  if (policy.activateOnClose === "recent") {
    const r = g.recent.find((id) => id !== closing && rest.includes(id));
    if (r) return r;
  }
  const left = g.tabs.slice(0, idx).reverse().find((t) => rest.includes(t));
  const right = g.tabs.slice(idx + 1).find((t) => rest.includes(t));
  return policy.activateOnClose === "left" ? (left ?? right ?? null) : (right ?? left ?? null);
}

/**
 * Close tabs in one group, or in every group with `groupId` null. Empty groups disappear
 * while another group is left.
 */
export function closeTabs(layout: TabLayout, groupId: number | null, ids: string[], policy: TabPolicy = DEFAULT_POLICY): TabLayout {
  const l = clone(layout);
  for (const g of l.groups) {
    if (groupId !== null && g.id !== groupId) continue;
    const rest = g.tabs.filter((t) => !ids.includes(t));
    if (rest.length === g.tabs.length) continue;
    if (g.active && ids.includes(g.active)) {
      const next = successor(g, g.active, rest, policy);
      g.active = next;
    }
    g.tabs = rest;
    g.recent = g.recent.filter((t) => !ids.includes(t));
    if (g.active) g.recent = [g.active, ...g.recent.filter((t) => t !== g.active)];
  }
  return dropEmptyGroups(l);
}

function dropEmptyGroups(l: TabLayout): TabLayout {
  if (l.groups.length > 1) {
    const kept = l.groups.filter((g) => g.tabs.length > 0);
    l.groups = kept.length ? kept : [l.groups[0]];
  }
  if (!groupById(l, l.activeGroup)) {
    // The neighbour of the removed group, or the first one.
    l.activeGroup = l.groups[0].id;
  }
  const known = openIds(l);
  l.pinned = l.pinned.filter((p) => known.has(p));
  return l;
}

/** Tabs closed by "Close Other Tabs" (pinned tabs stay). */
export const othersOf = (l: TabLayout, groupId: number, id: string): string[] =>
  (groupById(l, groupId)?.tabs ?? []).filter((t) => t !== id && !l.pinned.includes(t));

/** Tabs closed by "Close All Tabs" (pinned tabs stay unless `includePinned`). */
export const allOf = (l: TabLayout, groupId: number, includePinned = false): string[] =>
  (groupById(l, groupId)?.tabs ?? []).filter((t) => includePinned || !l.pinned.includes(t));

/** Tabs closed by "Close Tabs to the Left" / "to the Right" (pinned tabs stay). */
export function sideOf(l: TabLayout, groupId: number, id: string, side: "left" | "right"): string[] {
  const tabs = groupById(l, groupId)?.tabs ?? [];
  const i = tabs.indexOf(id);
  if (i < 0) return [];
  return (side === "left" ? tabs.slice(0, i) : tabs.slice(i + 1)).filter((t) => !l.pinned.includes(t));
}

/** Pin or unpin a tab; pinned tabs move to the front of their groups. */
export function togglePin(layout: TabLayout, id: string): TabLayout {
  const l = clone(layout);
  const pinning = !l.pinned.includes(id);
  l.pinned = pinning ? [...l.pinned, id] : l.pinned.filter((p) => p !== id);
  for (const g of l.groups) {
    if (!g.tabs.includes(id)) continue;
    const rest = g.tabs.filter((t) => t !== id);
    const end = rest.filter((t) => l.pinned.includes(t)).length;
    rest.splice(end, 0, id);
    g.tabs = rest;
  }
  return l;
}

/**
 * Split Right (`vertical`) / Split Down (`horizontal`): open `id` (default: the active tab
 * of the group) in a new group next to `groupId`. A layout split the other way changes its
 * orientation (Forge splits in one direction at a time).
 */
export function split(layout: TabLayout, orientation: SplitOrientation, groupId?: number, id?: string): TabLayout {
  const l = clone(layout);
  const from = groupById(l, groupId ?? l.activeGroup) ?? activeGroupOf(l);
  const tab = id ?? from.active;
  if (!tab) return layout;
  const g: TabGroup = { id: l.nextGroupId++, tabs: [tab], active: tab, recent: [tab] };
  l.groups.splice(l.groups.indexOf(from) + 1, 0, g);
  l.orientation = orientation;
  l.activeGroup = g.id;
  return l;
}

/** The group next to `groupId` (after it, or before it for the last group). */
export function oppositeGroup(l: TabLayout, groupId: number): TabGroup | undefined {
  const i = l.groups.findIndex((g) => g.id === groupId);
  if (i < 0 || l.groups.length < 2) return undefined;
  return l.groups[i + 1] ?? l.groups[i - 1];
}

/** "Move to Opposite Group": move a tab to the neighbouring group (and activate it there). */
export function moveToOpposite(layout: TabLayout, groupId: number, id: string, policy: TabPolicy = DEFAULT_POLICY): TabLayout {
  const target = oppositeGroup(layout, groupId);
  if (!target) return layout;
  const pinned = isPinned(layout, id);
  let l = openTab(layout, id, { group: target.id, policy }).layout;
  l = closeTabs(l, groupId, [id], policy);
  if (pinned && !l.pinned.includes(id)) l = togglePin(l, id);
  return activateTab(l, target.id, id);
}

/**
 * Unsplit: merge `groupId` (default: all groups) into its neighbour; tabs keep their
 * order and duplicates collapse.
 */
export function unsplit(layout: TabLayout, groupId?: number): TabLayout {
  if (layout.groups.length < 2) return layout;
  const l = clone(layout);
  const merge = (into: TabGroup, from: TabGroup) => {
    for (const t of from.tabs) if (!into.tabs.includes(t)) into.tabs.push(t);
    into.recent = [...into.recent, ...from.recent.filter((r) => !into.recent.includes(r))];
    l.groups = l.groups.filter((g) => g !== from);
  };
  if (groupId === undefined) {
    const active = activeGroupOf(l);
    const first = l.groups[0];
    for (const g of [...l.groups.slice(1)]) merge(first, g);
    if (active.active) touch(first, active.active);
    l.activeGroup = first.id;
  } else {
    const from = groupById(l, groupId);
    const into = from && oppositeGroup(l, from.id);
    if (!from || !into) return layout;
    merge(into, from);
    if (from.active) touch(into, from.active);
    l.activeGroup = into.id;
  }
  // Pinned tabs first again after merging.
  for (const g of l.groups) g.tabs = [...g.tabs.filter((t) => l.pinned.includes(t)), ...g.tabs.filter((t) => !l.pinned.includes(t))];
  return l;
}

/** Rename a tab id everywhere (a file was renamed or moved). */
export function renameId(layout: TabLayout, from: string, to: string): TabLayout {
  const l = clone(layout);
  const r = (t: string) => (t === from ? to : t);
  for (const g of l.groups) {
    g.tabs = g.tabs.map(r);
    g.recent = g.recent.map(r);
    if (g.active) g.active = r(g.active);
  }
  l.pinned = l.pinned.map(r);
  return l;
}

/** Next/previous tab of a group (Alt+Right / Alt+Left). */
export function cycle(l: TabLayout, groupId: number, delta: number): string | null {
  const g = groupById(l, groupId);
  if (!g?.tabs.length) return null;
  const i = g.active ? g.tabs.indexOf(g.active) : 0;
  return g.tabs[(i + delta + g.tabs.length) % g.tabs.length];
}

/** The group after/before the active one (Goto Next/Previous Splitter). */
export function neighbourGroup(l: TabLayout, delta: number): TabGroup | undefined {
  const i = l.groups.findIndex((g) => g.id === l.activeGroup);
  if (l.groups.length < 2) return undefined;
  return l.groups[(i + delta + l.groups.length) % l.groups.length];
}
