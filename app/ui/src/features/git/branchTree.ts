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
   * Speed search: only branches whose full name matches, in their folders, which are all
   * shown open, whatever their state is.
   */
  match?: (name: string) => boolean;
}

export function flattenBranchTree(nodes: readonly BranchNode[], o: FlattenOptions): BranchTreeRow[] {
  const rows: BranchTreeRow[] = [];
  const walk = (level: readonly BranchNode[], depth: number): void => {
    for (const node of level) {
      const key = branchRowKey(o.section, node);
      if (!isFolder(node)) {
        if (!o.match || o.match(node.path)) rows.push({ key, node, depth, expanded: false });
        continue;
      }
      const expanded = o.match ? true : (o.expansion.get(key) ?? expandedByDefault(node));
      const at = rows.push({ key, node, depth, expanded });
      if (expanded) walk(node.children, depth + 1);
      // Searching: a folder without a matching branch is not shown.
      if (o.match && rows.length === at) rows.pop();
    }
  };
  walk(nodes, o.depth ?? 0);
  return rows;
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
