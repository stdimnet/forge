/**
 * Type, Method and Call Hierarchy IPC (app/src-tauri/src/commands/hierarchy.rs): the data
 * of a hierarchy from the language servers and the PHP index. The views are computed from it
 * by `forge_codeintel::hierarchy` through WebAssembly (`features/hierarchy/model.ts`).
 * Paths are workspace relative (or `library://…`); positions are LSP positions.
 */
import { invoke } from "./index";
import type { LspPosition, LspRange } from "./lsp";

export type HierarchyKind =
  | "class"
  | "interface"
  | "trait"
  | "enum"
  | "method"
  | "constructor"
  | "function"
  | "property"
  | "file"
  | "other";

export interface HierarchyItem {
  id: string;
  /** `PaymentGateway`, `charge()`. */
  name: string;
  kind: HierarchyKind;
  /** Namespace of a type, declaring class of a method. */
  container?: string | null;
  detail?: string | null;
  /** Empty for a type known only by name. */
  path: string;
  line: number;
  column: number;
  isAbstract?: boolean;
  /** The language server's own item (opaque). */
  handle?: unknown;
}

export type MethodPresence = "defined" | "abstract";

export interface TypeEntry {
  item: HierarchyItem;
  extends?: string[];
  implements?: string[];
  method?: MethodPresence | null;
}

export interface TypeGraph {
  root: string;
  types: TypeEntry[];
  method?: string | null;
}

export interface TypeHierarchyAnswer {
  title: string;
  graph: TypeGraph | null;
  source: "lsp" | "index" | "none";
  message?: string | null;
}

export interface CallSite {
  path: string;
  range: LspRange;
  preview: string;
}

export interface CallNode {
  item: HierarchyItem;
  calls: CallSite[];
}

export interface CallHierarchyAnswer {
  title: string;
  root: HierarchyItem | null;
  source: "lsp" | "index" | "none";
  message?: string | null;
}

export type CallDirection = "callers" | "callees";

export const hierarchyApi = {
  typeHierarchy: (path: string, pos: LspPosition, content?: string) =>
    invoke<TypeHierarchyAnswer>("code_type_hierarchy", { path, line: pos.line, character: pos.character, content }),
  methodHierarchy: (path: string, pos: LspPosition, content?: string) =>
    invoke<TypeHierarchyAnswer>("code_method_hierarchy", { path, line: pos.line, character: pos.character, content }),
  callHierarchy: (path: string, pos: LspPosition, content?: string) =>
    invoke<CallHierarchyAnswer>("code_call_hierarchy", { path, line: pos.line, character: pos.character, content }),
  calls: (item: HierarchyItem, direction: CallDirection) =>
    invoke<CallNode[]>("code_call_hierarchy_calls", { item, direction }),
};
