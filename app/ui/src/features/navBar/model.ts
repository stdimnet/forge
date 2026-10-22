/**
 * Navigation Bar data: the segments of a tab (`forge_codeintel::navbar` through WebAssembly,
 * with the scopes at the caret from the editor structure) and what each segment's dropdown
 * lists (a folder's entries from the backend, a file's or class's members from the
 * Structure view's providers).
 */
import { createSignal } from "solid-js";
import { fileSystemFor, type EditorTab } from "@/state/editor";
import { getSetting, saveSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { workspaceApi } from "@/ipc/workspace";
import { viewFor } from "@/editor/views";
import { languageIdFor } from "@/editor/languages";
import { editorViewFor } from "@/shell/editorViews";
import { loadFileStructure } from "@/features/php/StructureView";
import type { StructureNode } from "@/features/php/api";
import { navBarSegments, scopesAt, structureOf, structureReady, type NavSegment } from "@/features/codeStructure/model";

export type { NavSegment };

/** View | Appearance | Navigation Bar. */
export type NavBarLocation = "top" | "statusBar" | "hidden";

export const navBarLocation = (): NavBarLocation => {
  const v = getSetting<string>("appearance.navigationBar.location", "top");
  return v === "statusBar" || v === "hidden" ? v : "top";
};
export const setNavBarLocation = (v: NavBarLocation) => saveSetting("appearance.navigationBar.location", v);
export const navBarMembers = (): boolean => getSetting<boolean>("appearance.navigationBar.showMembers", true) !== false;
export const setNavBarMembers = (v: boolean) => saveSetting("appearance.navigationBar.showMembers", v);

/** What the bar shows for a tab: interactive segments, or plain labels (library files, views). */
export interface TabSegments {
  segments: NavSegment[];
  plain?: string[];
}

/** The segments of `tab` (reactive: follows the caret, the content and the settings). */
export function segmentsOf(tab: EditorTab | undefined, group?: number): TabSegments {
  const project = workspace()?.name ?? "";
  if (!tab) return { segments: project ? [{ kind: "project", label: project, path: "" }] : [] };
  const fs = fileSystemFor(tab.path);
  if (fs) return { segments: [], plain: fs.breadcrumbs?.(tab.path) ?? [tab.name] };
  const ev = editorViewFor(tab);
  if (ev) return { segments: [], plain: ev.breadcrumbs?.(tab) ?? [tab.name] };
  // Read the caret and content so the bar follows them.
  void tab.cursor.line;
  void tab.cursor.col;
  void tab.content;
  const members = navBarMembers();
  let scopes: ReturnType<typeof scopesAt> = [];
  if (members && structureReady()) {
    const view = viewFor(tab.path, group);
    if (view) {
      const head = view.state.selection.main.head;
      const line = view.state.doc.lineAt(head);
      scopes = scopesAt(structureOf(view.state.doc, languageIdFor(tab.path)), line.number - 1, head - line.from);
    }
  }
  const segments = navBarSegments({ project, path: tab.path, scopes, showMembers: members });
  return { segments: segments.length ? segments : [{ kind: "file", label: tab.name, path: tab.path }] };
}

/** An entry of a segment's dropdown. */
export interface NavItem {
  kind: "directory" | "file" | "member";
  label: string;
  /** Directory or file. */
  path: string;
  /** Members: the structure node (and the file it belongs to in `path`). */
  node?: StructureNode;
  ignored?: boolean;
}

async function textOf(path: string, tab?: EditorTab): Promise<string> {
  if (tab?.path === path) return tab.content;
  return workspaceApi.readFile(path);
}

const sameName = (node: StructureNode, label: string) => node.name.replace(/\(\)$/, "") === label.replace(/\(.*\)$/, "");

/** The node a member segment stands for, and its parent's children. */
function findMember(nodes: StructureNode[], seg: NavSegment): { node: StructureNode; siblings: StructureNode[] } | null {
  for (const n of nodes) {
    if ((n.line === seg.line || (seg.line !== undefined && Math.abs(n.line - seg.line) <= 2)) && sameName(n, seg.label) && n.kind !== "region") {
      return { node: n, siblings: nodes };
    }
    const inner = findMember(n.children ?? [], seg);
    if (inner) return inner;
  }
  return null;
}

/** Members of a level, with region groups flattened (the bar lists members, not regions). */
const members = (nodes: StructureNode[]): StructureNode[] =>
  nodes.flatMap((n) => (n.kind === "region" ? members(n.children ?? []) : [n]));

/** What the dropdown of `seg` lists: a folder's entries, a file's or a member's members. */
export async function childrenOf(seg: NavSegment, tab?: EditorTab): Promise<NavItem[]> {
  if (seg.kind === "project" || seg.kind === "directory") {
    const entries = await workspaceApi.listDir(seg.path);
    return entries.map((e) => ({ kind: e.kind === "dir" ? "directory" : "file", label: e.name, path: e.path, ignored: e.ignored }));
  }
  const structure = await loadFileStructure(seg.path, await textOf(seg.path, tab)).catch(() => null);
  const nodes = structure?.nodes ?? [];
  const asItems = (list: StructureNode[]): NavItem[] =>
    members(list)
      .filter((n) => !n.inheritedFrom)
      .map((n) => ({ kind: "member", label: n.name, path: seg.path, node: n }));
  if (seg.kind === "file") return asItems(nodes);
  const found = findMember(nodes, seg);
  if (!found) return [];
  const kids = asItems(found.node.children ?? []);
  return kids.length ? kids : asItems(found.siblings);
}

/** The floating bar of Alt+Home when the bar is hidden (by editor group). */
const [floating, setFloating] = createSignal<{ group: number } | null>(null);
export { floating as floatingNavBar, setFloating as setFloatingNavBar };

/** A request to focus a bar (Alt+Home); the bar that matches takes it. */
const [focusRequest, setFocusRequest] = createSignal<{ placement: "top" | "statusBar" | "floating"; group?: number; seq: number } | null>(null);
export { focusRequest };
let seq = 0;
export function requestNavBarFocus(placement: "top" | "statusBar" | "floating", group?: number): void {
  setFocusRequest({ placement, group, seq: ++seq });
}
export const clearNavBarFocusRequest = () => setFocusRequest(null);
