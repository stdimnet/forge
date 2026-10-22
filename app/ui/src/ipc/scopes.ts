/**
 * Scopes and file colors (Rust: `crates/forge-scopes`). The scope language, the Scopes
 * page's preview tree and the colors run synchronously through WebAssembly
 * (`features/scopes/model.ts`); these commands answer what needs the project's files.
 */
import { invoke } from "./index";

export type PredefinedScope =
  | "allPlaces"
  | "projectFiles"
  | "projectProductionFiles"
  | "projectTestFiles"
  | "openFiles"
  | "changedFiles"
  | "currentFile"
  | "nonProjectFiles";

/** What a scope is evaluated against (`forge_scopes::ScopeContext`). */
export interface ScopeContext {
  testRoots?: string[];
  openFiles?: string[];
  changedFiles?: string[];
  currentFile?: string;
}

/** `{ predefined }` or a scope pattern. */
export type ScopeSpec = { predefined: PredefinedScope } | { pattern: string };

/** A scope as searches carry it to the backend (`forge_scopes::ScopeFilter`). */
export interface ScopeFilter {
  scope: ScopeSpec;
  context?: ScopeContext;
}

/** A named scope of the Scopes page (settings `scopes.shared` / `scopes.local`). */
export type NamedScope = {
  name: string;
  pattern: string;
};

export interface ScopeEntry {
  name: string;
  predefined?: PredefinedScope;
  pattern?: string;
  storage?: "shared" | "local";
}

export interface ScopeParseError {
  message: string;
  /** Character offset of the problem. */
  offset: number;
}

export interface ScopeTreeNode {
  name: string;
  path: string;
  dir: boolean;
  matched: boolean;
  matchedFiles: number;
  totalFiles: number;
  children?: ScopeTreeNode[];
}

export interface ScopeTree {
  nodes: ScopeTreeNode[];
  matched: number;
  total: number;
  error?: ScopeParseError;
}

export type ScopeEditOp = "include" | "includeRecursively" | "exclude" | "excludeRecursively";

/** A row of the File Colors tables (settings `fileColors.local` / `fileColors.shared`). */
export type FileColor = {
  scope: string;
  color: string;
};

export const scopesApi = {
  /** Every project file (`.gitignore` respected), listed where the project lives. */
  files: (limit?: number) => invoke<string[]>("scopes_files", { limit }),
  /** Test folders from `composer.json`'s `autoload-dev`. */
  testRoots: () => invoke<string[]>("scopes_test_roots"),
};
