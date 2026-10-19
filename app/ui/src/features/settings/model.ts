/**
 * Pure logic of the Settings dialog: building the page tree from registered pages,
 * search matching and highlighting. Kept free of components so it is easy to test.
 */
import type { SettingsPage } from "@/features/settingsPages";

/** PHPStorm's top-level order; unknown groups follow alphabetically. */
export const GROUP_ORDER = [
  "Appearance & Behavior",
  "Keymap",
  "Editor",
  "Plugins",
  "Version Control",
  "Languages & Frameworks",
  "Tools",
  "AI",
  "Advanced Settings",
];

export interface TreeNode {
  /** Page id, or `group:<name>` for a group without a page of its own. */
  id: string;
  title: string;
  page?: SettingsPage;
  children: TreeNode[];
}

export const groupNodeId = (group: string) => `group:${group}`;

const groupRank = (g: string) => {
  const i = GROUP_ORDER.indexOf(g);
  return i === -1 ? GROUP_ORDER.length : i;
};

/**
 * Pages become a tree: group > page > child pages (via `parent`). A page whose title equals
 * its group and that has no parent (e.g. "Plugins", "Keymap") is a top-level leaf.
 */
export function buildTree(pages: SettingsPage[]): TreeNode[] {
  const nodes = new Map<string, TreeNode>();
  for (const page of pages) nodes.set(page.id, { id: page.id, title: page.title, page, children: [] });

  const roots: TreeNode[] = [];
  const groups = new Map<string, TreeNode>();
  const groupNode = (name: string) => {
    let g = groups.get(name);
    if (!g) {
      g = { id: groupNodeId(name), title: name, children: [] };
      groups.set(name, g);
      roots.push(g);
    }
    return g;
  };

  for (const page of pages) {
    const node = nodes.get(page.id)!;
    const parent = page.parent ? nodes.get(page.parent) : undefined;
    if (parent && parent !== node) parent.children.push(node);
    else if (!page.parent && page.title === page.group) roots.push(node);
    else groupNode(page.group).children.push(node);
  }

  const rootGroup = (n: TreeNode) => n.page?.group ?? n.title;
  return roots.sort((a, b) => {
    const d = groupRank(rootGroup(a)) - groupRank(rootGroup(b));
    return d || rootGroup(a).localeCompare(rootGroup(b));
  });
}

/** Depth-first list of nodes (used for keyboard navigation and first-match selection). */
export function flatten(nodes: TreeNode[]): TreeNode[] {
  return nodes.flatMap((n) => [n, ...flatten(n.children)]);
}

/** Path of titles from the root to `id`, e.g. ["Editor", "Code Style", "PHP"]. */
export function breadcrumb(nodes: TreeNode[], id: string): TreeNode[] {
  for (const n of nodes) {
    if (n.id === id) return [n];
    const sub = breadcrumb(n.children, id);
    if (sub.length) return [n, ...sub];
  }
  return [];
}

const words = (query: string) => query.toLowerCase().split(/\s+/).filter(Boolean);

/** Every word of `query` occurs in `text` (case-insensitive). An empty query matches. */
export function matchesQuery(text: string, query: string): boolean {
  const hay = text.toLowerCase();
  return words(query).every((w) => hay.includes(w));
}

/** Whether a node matches by its title, keywords or extra searchable text of its page. */
export function nodeMatches(node: TreeNode, query: string, extraText: (pageId: string) => string): boolean {
  const text = [node.title, ...(node.page?.keywords ?? []), node.page ? extraText(node.page.id) : ""].join(" \n ");
  return words(query).length > 0 && matchesQuery(text, query);
}

/** Keep matching nodes and the ancestors of matching nodes. */
export function filterTree(
  nodes: TreeNode[],
  query: string,
  extraText: (pageId: string) => string,
): TreeNode[] {
  if (!words(query).length) return nodes;
  return nodes.flatMap((n) => {
    const children = filterTree(n.children, query, extraText);
    if (children.length || nodeMatches(n, query, extraText)) return [{ ...n, children }];
    return [];
  });
}

export interface Segment {
  text: string;
  hit: boolean;
}

/** Split `text` into segments, marking occurrences of any query word as hits. */
export function highlight(text: string, query: string): Segment[] {
  const ws = words(query);
  if (!ws.length) return [{ text, hit: false }];
  const lower = text.toLowerCase();
  const hits = new Array<boolean>(text.length).fill(false);
  for (const w of ws) {
    for (let i = lower.indexOf(w); i !== -1; i = lower.indexOf(w, i + 1)) hits.fill(true, i, i + w.length);
  }
  const out: Segment[] = [];
  for (let i = 0; i < text.length; i++) {
    const last = out[out.length - 1];
    if (last && last.hit === hits[i]) last.text += text[i];
    else out.push({ text: text[i], hit: hits[i] });
  }
  return out;
}
