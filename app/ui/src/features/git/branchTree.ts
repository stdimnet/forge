/**
 * The visible rows of the branch trees (Branches popup, Git tool window). The folders come
 * from Rust (`forge_vcs::branch_tree`, in `Branches.localTree` / `remoteTree`); this module
 * only decides which of them are open and flattens them for rendering. Free of DOM.
 */
import { getSetting } from "@/state/settings";
import type { BranchNode } from "./api";

/** "Group By Directory": Settings | Version Control | Git and the Branches popup's settings menu. */
export const GROUP_BY_DIRECTORY = "vcs.git.branches.groupByDirectory";
export const groupByDirectory = (): boolean => getSetting<boolean>(GROUP_BY_DIRECTORY, true);

export interface BranchTreeRow {
  /** Unique in its tree: a folder and a branch may share a path (`feat` next to `feat/x`). */
  key: string;
  node: BranchNode;
  depth: number;
  /** Folders and remotes only. */
  expanded: boolean;
}

/** What the user opened (`true`) or closed (`false`), by row key; other folders use the default. */
export type Expansion = ReadonlyMap<string, boolean>;

export const isFolder = (node: BranchNode): boolean => node.kind !== "branch";

export const branchRowKey = (section: string, node: BranchNode): string => `${section}:${isFolder(node) ? "dir" : "branch"}:${node.path}`;

/** Like PHPStorm: remotes and the folders of the current branch start open, other folders closed. */
export const expandedByDefault = (node: BranchNode): boolean => node.kind === "remote" || node.current;

export interface FlattenOptions {
  /** Prefix of the row keys (`local`, `remote`). */
  section: string;
  expansion: Expansion;
  /** Depth of the top level. */
  depth?: number;
  /**
   * Speed search (see `searchQuery`): the branches and folders whose name contains it. The
   * folders above a match are shown open, whatever their state is; a folder whose own name
   * matches is shown in its state, with everything in it when it is open.
   */
  query?: string;
}

/** The speed search text as it is compared: trimmed, without regard to case. */
export const searchQuery = (text: string): string => text.trim().toLowerCase();

/** Where `query` (from `searchQuery`) is found in `text`, for the highlight; null when it is not. */
export function matchRange(text: string, query: string): [number, number] | null {
  const at = query ? text.toLowerCase().indexOf(query) : -1;
  return at < 0 ? null : [at, at + query.length];
}

export function flattenBranchTree(nodes: readonly BranchNode[], o: FlattenOptions): BranchTreeRow[] {
  const q = o.query ?? "";
  const hit = (text: string) => matchRange(text, q) !== null;
  /** Without a search, and inside an open folder that matches by its own name. */
  const walk = (level: readonly BranchNode[], depth: number): BranchTreeRow[] =>
    level.flatMap((node): BranchTreeRow[] => {
      const key = branchRowKey(o.section, node);
      if (!isFolder(node)) return [{ key, node, depth, expanded: false }];
      const expanded = o.expansion.get(key) ?? expandedByDefault(node);
      return [{ key, node, depth, expanded }, ...(expanded ? walk(node.children, depth + 1) : [])];
    });
  const search = (level: readonly BranchNode[], depth: number): BranchTreeRow[] =>
    level.flatMap((node): BranchTreeRow[] => {
      const key = branchRowKey(o.section, node);
      // A branch by its name in the folder, or by its full name when the search has a slash.
      if (!isFolder(node)) return hit(node.label) || (q.includes("/") && hit(node.path)) ? [{ key, node, depth, expanded: false }] : [];
      const inside = search(node.children, depth + 1);
      if (inside.length) return [{ key, node, depth, expanded: true }, ...inside];
      return hit(node.label) ? walk([node], depth) : [];
    });
  return q ? search(nodes, o.depth ?? 0) : walk(nodes, o.depth ?? 0);
}

export function toggled(expansion: Expansion, row: Pick<BranchTreeRow, "key" | "expanded">, open = !row.expanded): Map<string, boolean> {
  return new Map(expansion).set(row.key, open);
}

/**
 * "Group By Directory" off: every remote with its branches directly below it, named by the
 * rest of their name (`feat/ui/tabs`), in the order of `names` (the flat list, as git sorts it).
 */
export function ungrouped(nodes: readonly BranchNode[], names: readonly string[]): BranchNode[] {
  const order = new Map(names.map((name, i) => [name, i]));
  const at = (b: BranchNode) => order.get(b.path) ?? names.length;
  const leaves = (node: BranchNode): BranchNode[] => (isFolder(node) ? node.children.flatMap(leaves) : [node]);
  return nodes.map((node) =>
    node.kind === "remote"
      ? {
          ...node,
          children: leaves(node)
            .sort((a, b) => at(a) - at(b))
            .map((b) => ({ ...b, label: b.path.slice(node.path.length + 1) })),
        }
      : node,
  );
}
