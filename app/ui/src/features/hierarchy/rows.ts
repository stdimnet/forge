/**
 * The visible rows of a Hierarchy tab: the Rust views (`model.ts`) walked with the tab's
 * expansion state. Rows are keyed by the ids from the root, since one type or function can
 * appear on several branches.
 */
import type { CallSite, HierarchyItem } from "@/ipc/hierarchy";
import { callView, methodView, typeView, type HierarchyNode, type MethodMark } from "./model";
import { ROOT_KEY, type CallTab, type HierarchyTab } from "./state";

export interface HierarchyRow {
  key: string;
  depth: number;
  item: HierarchyItem;
  hasChildren: boolean;
  expanded: boolean;
  current?: boolean;
  mark?: MethodMark | null;
  /** Call Hierarchy: where the calls are made. */
  calls?: CallSite[];
  /** Call Hierarchy: the function calls itself further up (not expandable). */
  recursive?: boolean;
  /** A placeholder row below an expanded call node. */
  status?: "loading" | "error";
  message?: string;
}

function treeRows(roots: HierarchyNode[], collapsed: string[]): HierarchyRow[] {
  const out: HierarchyRow[] = [];
  const walk = (n: HierarchyNode, depth: number, parent: string) => {
    const key = parent ? `${parent}/${n.item.id}` : n.item.id;
    const expanded = !collapsed.includes(key);
    out.push({ key, depth, item: n.item, hasChildren: n.children.length > 0, expanded, current: n.current, mark: n.mark });
    if (expanded) for (const c of n.children) walk(c, depth + 1, key);
  };
  for (const r of roots) walk(r, 0, "");
  return out;
}

/** Keys of every row with children (Expand All). */
export function allKeys(tab: HierarchyTab): string[] {
  return rowsOf({ ...tab, collapsed: [] } as HierarchyTab)
    .filter((r) => r.hasChildren)
    .map((r) => r.key);
}

/** Can a call node be asked for its callers / callees? */
export const expandable = (item: HierarchyItem) => !!item.path && item.kind !== "file" && !item.path.includes("://");

function callRows(tab: CallTab): HierarchyRow[] {
  const out: HierarchyRow[] = [];
  const walk = (item: HierarchyItem, calls: CallSite[] | undefined, depth: number, key: string, ancestors: string[]) => {
    const recursive = ancestors.includes(item.id);
    const canExpand = depth === 0 || (expandable(item) && !recursive);
    const expanded = canExpand && tab.expanded.includes(key);
    const loaded = tab.calls[key];
    const known = Array.isArray(loaded) ? loaded : undefined;
    out.push({
      key,
      depth,
      item,
      calls,
      recursive,
      // Unknown until loaded: show the arrow; loaded and empty: no arrow.
      hasChildren: canExpand && (known === undefined || known.length > 0),
      expanded,
    });
    if (!expanded) return;
    if (loaded === "loading" || loaded === undefined) {
      out.push({ key: `${key}/…`, depth: depth + 1, item, hasChildren: false, expanded: false, status: "loading", message: "Loading…" });
      return;
    }
    if (!Array.isArray(loaded)) {
      out.push({ key: `${key}/!`, depth: depth + 1, item, hasChildren: false, expanded: false, status: "error", message: loaded.error });
      return;
    }
    const children = callView({ base: tab.root, calls: loaded, scope: tab.scope, sortAlphabetically: tab.sortAlphabetically });
    for (const c of children) walk(c.item, c.calls, depth + 1, `${key}/${c.item.id}`, [...ancestors, item.id]);
  };
  walk(tab.root, undefined, 0, ROOT_KEY, []);
  return out;
}

export function rowsOf(tab: HierarchyTab): HierarchyRow[] {
  switch (tab.kind) {
    case "type":
      return treeRows(
        typeView({ graph: tab.graph, view: tab.view, scope: tab.scope, sortAlphabetically: tab.sortAlphabetically }).roots,
        tab.collapsed,
      );
    case "method":
      return treeRows(
        methodView({
          graph: tab.graph,
          hideNotImplemented: tab.hideNotImplemented,
          scope: tab.scope,
          sortAlphabetically: tab.sortAlphabetically,
        }).roots,
        tab.collapsed,
      );
    case "call":
      return callRows(tab);
  }
}
