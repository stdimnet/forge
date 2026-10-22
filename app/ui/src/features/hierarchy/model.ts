/**
 * The Hierarchy tool window's views, computed by Rust (`forge_codeintel::hierarchy`) through
 * WebAssembly, synchronously, in the desktop app and in browser mode: Class / Supertypes /
 * Subtypes Hierarchy, the Method Hierarchy's marks, scopes and sorting. The UI only renders
 * the trees.
 */
import { createSignal } from "solid-js";
import { forgeWasm, type ForgeWasm } from "@/wasm";
import type { CallNode, HierarchyItem, TypeGraph } from "@/ipc/hierarchy";

export type TypeView = "class" | "supertypes" | "subtypes";
export type HierarchyScope = "all" | "production" | "test" | "thisClass";
export type MethodMark = "defined" | "abstract" | "shouldDefine" | "inherited";

export interface HierarchyNode {
  item: HierarchyItem;
  children: HierarchyNode[];
  /** The type the hierarchy was opened for. */
  current?: boolean;
  mark?: MethodMark | null;
}

export interface HierarchyView {
  roots: HierarchyNode[];
}

let wasm: ForgeWasm | null = null;
const [ready, setReady] = createSignal(false);
export { ready as hierarchyReady };

/** Resolves once the Rust views are loaded. */
export const hierarchyLoaded: Promise<boolean> = forgeWasm().then(
  (w) => {
    wasm = w;
    setReady(true);
    return true;
  },
  (e) => {
    console.warn("hierarchy views not loaded", e);
    return false;
  },
);

function call<T>(pick: (w: ForgeWasm) => (json: string) => string, input: unknown, fallback: T): T {
  // Reading the signal makes views recompute once the module is loaded.
  if (!ready() || !wasm) return fallback;
  try {
    const out = JSON.parse(pick(wasm)(JSON.stringify(input))) as T | { error: string };
    if (out && typeof out === "object" && !Array.isArray(out) && "error" in out && Object.keys(out).length === 1) {
      console.warn("hierarchy view:", (out as { error: string }).error);
      return fallback;
    }
    return out as T;
  } catch (e) {
    console.warn("hierarchy view:", e);
    return fallback;
  }
}

const NO_VIEW: HierarchyView = { roots: [] };

export function typeView(req: { graph: TypeGraph; view: TypeView; scope: HierarchyScope; sortAlphabetically: boolean }): HierarchyView {
  return call((w) => w.hierarchy_type_view, req, NO_VIEW);
}

export function methodView(req: {
  graph: TypeGraph;
  hideNotImplemented: boolean;
  scope: HierarchyScope;
  sortAlphabetically: boolean;
}): HierarchyView {
  return call((w) => w.hierarchy_method_view, req, NO_VIEW);
}

/** One level of a Call Hierarchy after scope and sorting. */
export function callView(req: { base: HierarchyItem; calls: CallNode[]; scope: HierarchyScope; sortAlphabetically: boolean }): CallNode[] {
  return call((w) => w.hierarchy_call_view, req, req.calls);
}

export const SCOPES: { id: HierarchyScope; label: string }[] = [
  { id: "all", label: "All" },
  { id: "production", label: "Production" },
  { id: "test", label: "Test" },
  { id: "thisClass", label: "This Class" },
];

/** PHPStorm's legend of the Method Hierarchy. */
export const MARK_TITLES: Record<MethodMark, string> = {
  defined: "Method is defined in the class",
  abstract: "Method is declared abstract in the class",
  shouldDefine: "Method should be defined since the class is not abstract",
  inherited: "Method is not defined in the class but defined in a superclass",
};
