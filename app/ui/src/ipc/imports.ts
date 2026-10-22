/**
 * Optimize Imports and Auto Import (app/src-tauri/src/commands/imports.rs,
 * phpforge_lang_php::imports, forge_codeintel::imports). PHP is computed by Forge; JavaScript,
 * TypeScript and Vue use the TypeScript server's source actions. Offsets are UTF-16.
 */
import { invoke } from "./index";
import type { TextEditLsp, WorkspaceEdit } from "./codeintel";
import type { LspPosition, LspRange } from "./lsp";

/** Optimize Imports of the editor's text. */
export interface OptimizeAnswer {
  /** One replacement of the changed part, `null` when nothing changes. */
  edit: TextEditLsp | null;
  /** "Removed 2 imports, rearranged imports" / "Imports are already optimized". */
  message: string;
}

export interface FolderOptimizeAnswer {
  edit: WorkspaceEdit;
  files: number;
  changed: number;
  message: string;
}

/** A class name that resolves to no known class, with the classes an import could bring in. */
export interface UnresolvedClass {
  start: number;
  end: number;
  name: string;
  resolved: string;
  doc: boolean;
  candidates: string[];
}

/** Data of the ambiguous "Import class" fix (`PhpImportClass`). */
export interface ImportChoice {
  start: number;
  end: number;
  name: string;
  doc: boolean;
  candidates: string[];
}

export interface OnTheFlyRequest {
  caret: number;
  editedFrom?: number | null;
  editedTo?: number | null;
  add: boolean;
  optimize: boolean;
}

/** An edit of a PHP text (UTF-16 offsets). */
export interface OffsetEdit {
  start: number;
  end: number;
  newText: string;
}

export interface OnTheFlyAnswer {
  edits: OffsetEdit[];
  added: string[];
  removed: string[];
}

export const importsApi = {
  optimize: (path: string, content: string) => invoke<OptimizeAnswer>("imports_optimize", { path, content }),
  optimizePaths: (req: { paths: string[]; includeSubdirectories: boolean; onlyVcsChanged: boolean; buffers?: Record<string, string> }) =>
    invoke<FolderOptimizeAnswer>("imports_optimize_paths", req),
  phpOnTheFly: (path: string, content: string, request: OnTheFlyRequest) =>
    invoke<OnTheFlyAnswer>("php_imports_on_the_fly", { path, content, request }),
  phpHint: (path: string, content: string, offset: number) =>
    invoke<UnresolvedClass | null>("php_import_hint", { path, content, offset }),
  /** Edits referring to `fqn` at `start..end`: the short name plus an import, or the FQN. */
  phpReference: (req: { content: string; start: number; end: number; fqn: string; doc?: boolean; auto?: boolean }) =>
    invoke<OffsetEdit[]>("php_import_reference", req),
  /** The TypeScript server's on-the-fly import edit, when it stays away from the caret. */
  webOnTheFly: (req: { path: string; content: string; add: boolean; optimize: boolean; caret: LspPosition; edited?: LspRange | null }) =>
    invoke<WorkspaceEdit | null>("code_imports_on_the_fly", req),
};
