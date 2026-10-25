/**
 * Project structure and the Project view's rows (Rust: `forge_scopes::{structure, view}`,
 * `commands/project_structure.rs`). Marking folders, the Directories page and the File
 * Nesting defaults run synchronously through WebAssembly (`features/projectStructure`);
 * `project_tree_list` reads the project where it lives.
 */
import { invoke, listen } from "./index";
import type { FileKind } from "./workspace";

export type FolderKind = "excluded" | "resource" | "testResource" | "included";
export type ExcludeReason = "user" | "default" | "pattern" | "parent";
export type MarkAction = "excluded" | "cancelExclusion" | "resource" | "testResource" | "unmark";

/** One entry of `projectStructure.folders`. */
export interface FolderMark {
  path: string;
  kind: FolderKind;
}

/** The stored project structure (`forge_scopes::structure::StructureConfig`). */
export interface StructureConfig {
  folders: FolderMark[];
  excludePatterns: string;
}

/** How a folder is marked and why it is excluded. */
export interface FolderState {
  kind: FolderKind | null;
  reason: ExcludeReason | null;
}

/** A File Nesting rule: parent suffix and `;`-separated child suffixes. */
export interface NestingRule {
  parent: string;
  children: string;
}

/** Project view options (⋮) that change the rows. */
export interface ViewOptions {
  showExcluded: boolean;
  foldersOnTop: boolean;
  sortByType: boolean;
  compactMiddlePackages: boolean;
  fileNesting: boolean;
  /** Empty uses PHPStorm's predefined rules. */
  nestingRules: NestingRule[];
}

/** One Project view row. */
export interface TreeRow {
  /** The label; a compacted chain of folders reads `a/b/c`. */
  name: string;
  path: string;
  kind: FileKind;
  ignored?: boolean;
  excluded?: boolean;
  /** The folders a compacted row stands for, outermost first. */
  compacted?: string[];
  /** Files nested under this file (File Nesting). */
  nested?: TreeRow[];
}

export const projectStructureApi = {
  treeList: (path: string, options: ViewOptions) => invoke<TreeRow[]>("project_tree_list", { path, options }),
  /** The excluded paths changed (the backend has them already). */
  onChanged: (cb: () => void) => listen<null>("project-structure://changed", () => cb()),
};
