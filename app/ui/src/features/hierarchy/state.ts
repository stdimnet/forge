/**
 * Tabs of the Hierarchy tool window: one per Type Hierarchy (Ctrl+H), Method Hierarchy
 * (Ctrl+Shift+H) or Call Hierarchy (Ctrl+Alt+H) run, with its view options. The data comes
 * from `code_*_hierarchy` (`ipc/hierarchy.ts`); the trees are computed from it in Rust
 * (`model.ts`). Callers and callees are loaded lazily, one level per expanded node.
 */
import { createSignal } from "solid-js";
import { createStore, produce } from "solid-js/store";
import type { EditorView } from "@codemirror/view";
import { openFileAt } from "@/editor/views";
import { showToolWindow } from "@/state/layout";
import { showBalloon } from "@/keymap";
import { isLibraryPath } from "@/ipc/codeintel";
import { hierarchyApi, type CallDirection, type CallNode, type HierarchyItem, type TypeGraph } from "@/ipc/hierarchy";
import type { LspPosition } from "@/ipc/lsp";
import { showHint } from "@/features/lsp/hint";
import { syncNow } from "@/features/lsp/sync";
import { toLsp } from "@/features/lsp/util";
import type { HierarchyScope, TypeView } from "./model";

export const HIERARCHY_WINDOW = "hierarchy";

interface Origin {
  path: string;
  pos: LspPosition;
  content?: string;
}

interface TabBase {
  id: number;
  title: string;
  origin: Origin;
  scope: HierarchyScope;
  sortAlphabetically: boolean;
  /** Collapsed rows (type trees start expanded). */
  collapsed: string[];
  /** Expanded rows (call trees start collapsed below the root). */
  expanded: string[];
  selected: string | null;
}

export interface TypeTab extends TabBase {
  kind: "type";
  view: TypeView;
  graph: TypeGraph;
}

export interface MethodTab extends TabBase {
  kind: "method";
  graph: TypeGraph;
  hideNotImplemented: boolean;
}

export type CallChildren = CallNode[] | "loading" | { error: string };

export interface CallTab extends TabBase {
  kind: "call";
  root: HierarchyItem;
  direction: CallDirection;
  /** Callers or callees by row key, as the sources answered (before scope and sorting). */
  calls: Record<string, CallChildren>;
}

export type HierarchyTab = TypeTab | MethodTab | CallTab;

const [state, setState] = createStore<{ tabs: HierarchyTab[]; active: number | null }>({ tabs: [], active: null });
export { state as hierarchyState };

let nextId = 1;

export const activeHierarchyTab = (): HierarchyTab | undefined => state.tabs.find((t) => t.id === state.active);
export const setActiveHierarchyTab = (id: number) => setState("active", id);

/** "Autoscroll to Source": selecting a node opens it. */
const [autoscroll, setAutoscroll] = createSignal(false);
export { autoscroll, setAutoscroll };

export function closeHierarchyTab(id: number): void {
  const idx = state.tabs.findIndex((t) => t.id === id);
  if (idx < 0) return;
  setState(
    produce((s) => {
      s.tabs.splice(idx, 1);
      if (s.active === id) s.active = s.tabs[Math.min(idx, s.tabs.length - 1)]?.id ?? null;
    }),
  );
}

export function updateTab(id: number, fn: (t: HierarchyTab) => void): void {
  setState(
    "tabs",
    (t) => t.id === id,
    produce((t) => fn(t)),
  );
}

/** A new run replaces the tab of the same kind and title (PHPStorm reuses unpinned tabs). */
type NewTab = Omit<TypeTab, "id"> | Omit<MethodTab, "id"> | Omit<CallTab, "id">;

function openTab(tab: NewTab): number {
  const same = state.tabs.find((t) => t.kind === tab.kind && t.title === tab.title);
  const id = same?.id ?? nextId++;
  const full = { ...tab, id } as HierarchyTab;
  setState(
    produce((s) => {
      const i = s.tabs.findIndex((t) => t.id === id);
      if (i >= 0) s.tabs[i] = full;
      else s.tabs.push(full);
      s.active = id;
    }),
  );
  showToolWindow(HIERARCHY_WINDOW);
  return id;
}

function base(title: string, origin: Origin): Omit<TabBase, "id"> {
  return { title, origin, scope: "all", sortAlphabetically: false, collapsed: [], expanded: [], selected: null };
}

const errorText = (e: unknown) => String(e instanceof Error ? e.message : e);

async function origin(path: string, view: EditorView, pos: number): Promise<Origin | null> {
  if (isLibraryPath(path)) {
    showHint(view, pos, "Hierarchies are computed for project files");
    return null;
  }
  await syncNow(path);
  return { path, pos: toLsp(view.state.doc, pos), content: view.state.doc.toString() };
}


/** Type Hierarchy (Ctrl+H) of the type at the caret. */
export async function typeHierarchyAt(path: string, view: EditorView, pos: number): Promise<void> {
  const o = await origin(path, view, pos);
  if (!o) return;
  const answer = await hierarchyApi.typeHierarchy(o.path, o.pos, o.content).catch((e) => ({ graph: null, title: "", message: errorText(e) }));
  if (!answer.graph) {
    showHint(view, pos, answer.message ?? "No type found at the caret");
    return;
  }
  openTab({ ...base(answer.title, o), kind: "type", view: "class", graph: answer.graph });
}

/** Method Hierarchy (Ctrl+Shift+H) of the method at the caret. */
export async function methodHierarchyAt(path: string, view: EditorView, pos: number): Promise<void> {
  const o = await origin(path, view, pos);
  if (!o) return;
  const answer = await hierarchyApi.methodHierarchy(o.path, o.pos, o.content).catch((e) => ({ graph: null, title: "", message: errorText(e) }));
  if (!answer.graph) {
    showHint(view, pos, answer.message ?? "No method found at the caret");
    return;
  }
  openTab({ ...base(answer.title, o), kind: "method", graph: answer.graph, hideNotImplemented: false });
}

export const ROOT_KEY = "root";

/** Call Hierarchy (Ctrl+Alt+H) of the method or function at the caret: its callers. */
export async function callHierarchyAt(path: string, view: EditorView, pos: number): Promise<void> {
  const o = await origin(path, view, pos);
  if (!o) return;
  const answer = await hierarchyApi.callHierarchy(o.path, o.pos, o.content).catch((e) => ({ root: null, title: "", message: errorText(e) }));
  if (!answer.root) {
    showHint(view, pos, answer.message ?? "No method or function found at the caret");
    return;
  }
  const id = openTab({ ...base(answer.title, o), kind: "call", root: answer.root, direction: "callers", calls: {}, expanded: [ROOT_KEY], selected: ROOT_KEY });
  void loadCalls(id, ROOT_KEY, answer.root);
}

/** Loads the callers or callees of the row `key` (once). */
export async function loadCalls(tabId: number, key: string, item: HierarchyItem, force = false): Promise<void> {
  const tab = state.tabs.find((t) => t.id === tabId);
  if (!tab || tab.kind !== "call") return;
  if (!force && tab.calls[key] !== undefined) return;
  const direction = tab.direction;
  updateTab(tabId, (t) => {
    if (t.kind === "call") t.calls[key] = "loading";
  });
  let result: CallChildren;
  try {
    result = await hierarchyApi.calls(item, direction);
  } catch (e) {
    result = { error: errorText(e) };
  }
  updateTab(tabId, (t) => {
    // Ignore answers for a direction the user switched away from meanwhile.
    if (t.kind === "call" && t.direction === direction) t.calls[key] = result;
  });
}

/** Callers / Callees: the tree starts again from the root. */
export function setCallDirection(tabId: number, direction: CallDirection): void {
  const tab = state.tabs.find((t) => t.id === tabId);
  if (!tab || tab.kind !== "call" || tab.direction === direction) return;
  updateTab(tabId, (t) => {
    if (t.kind !== "call") return;
    t.direction = direction;
    t.calls = {};
    t.expanded = [ROOT_KEY];
    t.selected = ROOT_KEY;
  });
  void loadCalls(tabId, ROOT_KEY, tab.root);
}

/** Refresh: asks the sources again for the tab's origin, keeping the view options. */
export async function refreshTab(tabId: number): Promise<void> {
  const tab = state.tabs.find((t) => t.id === tabId);
  if (!tab) return;
  const { path, pos, content } = tab.origin;
  try {
    if (tab.kind === "call") {
      updateTab(tabId, (t) => {
        if (t.kind === "call") t.calls = {};
      });
      const answer = await hierarchyApi.callHierarchy(path, pos, content);
      if (answer.root) {
        const root = answer.root;
        updateTab(tabId, (t) => {
          if (t.kind === "call") t.root = root;
        });
      }
      const fresh = state.tabs.find((t) => t.id === tabId) as CallTab | undefined;
      if (fresh) void loadCalls(tabId, ROOT_KEY, fresh.root, true);
      return;
    }
    const answer = await (tab.kind === "type" ? hierarchyApi.typeHierarchy : hierarchyApi.methodHierarchy)(path, pos, content);
    if (answer.graph) {
      const graph = answer.graph;
      updateTab(tabId, (t) => {
        if (t.kind !== "call") t.graph = graph;
      });
    }
  } catch (e) {
    showBalloon("Hierarchy could not be refreshed", errorText(e));
  }
}

/** Opens a node's declaration (or a call site). */
export function openItem(item: { path: string; line: number; column: number }): void {
  if (!item.path) return;
  void openFileAt(item.path, item.line + 1, item.column + 1);
}

export function toggleKey(list: string[], key: string, on: boolean): string[] {
  const has = list.includes(key);
  if (on && !has) return [...list, key];
  if (!on && has) return list.filter((k) => k !== key);
  return list;
}
