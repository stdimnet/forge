/**
 * Extract / Introduce and Inline refactorings (see app/src-tauri/src/commands/refactor.rs):
 * Rust analyses PHP code and computes the edits; language servers' refactorings for other
 * files come through `code_refactor_actions`. Positions are LSP positions (0-based line,
 * UTF-16 character).
 */
import { invoke } from "./index";
import type { LspPosition, LspRange } from "./lsp";
import type { ContextAction, FilePreview, RenameConflict, WorkspaceEdit } from "./codeintel";

export type IntroduceKind = "variable" | "constant" | "field" | "parameter";
export type FieldInit = "currentMethod" | "fieldDeclaration" | "constructor";
export type Visibility = "public" | "protected" | "private";

export interface ExpressionChoice {
  range: LspRange;
  /** One line: `$this->getUser()->name`. */
  text: string;
}

export interface IntroduceInfo {
  range: LspRange;
  text: string;
  /** Occurrences that "Replace all" replaces (the expression included). */
  occurrences: number;
  /** Name suggestions, best first (constants in UPPER_CASE, others without `$`). */
  names: string[];
  className?: string | null;
  /** Introduce Parameter: `User::send()`. */
  functionName?: string | null;
  initPlaces: FieldInit[];
  isStatic: boolean;
  /** Can be a field's or parameter's default value. */
  constantExpression: boolean;
  /** Introduce Constant outside a class: a global `const`. */
  global: boolean;
  /** Introduce Parameter: calls that get the argument. */
  callSites: number;
}

export interface IntroducePrepareAnswer {
  /** Several expressions at the caret: the "Expressions" popup. */
  expressions: ExpressionChoice[];
  info?: IntroduceInfo | null;
}

export interface IntroduceOptions {
  name: string;
  replaceAll: boolean;
  visibility?: Visibility | null;
  init?: FieldInit;
  defaultValue?: boolean;
}

export interface RefactorAnswer {
  edit: WorkspaceEdit;
  preview: FilePreview[];
  conflicts: RenameConflict[];
  /** The new name's occurrences in the current file after the edit (in-place editing). */
  nameRanges: LspRange[];
  skipped: number;
}

export interface ExtractParameter {
  name: string;
  typeHint?: string | null;
}

export type ReturnKind = "none" | "expression" | "variables" | "return";

export interface ExtractMethodInfo {
  /** "Extract Method" / "Extract Function". */
  title: string;
  range: LspRange;
  name: string;
  parameters: ExtractParameter[];
  outputs: string[];
  returns: ReturnKind;
  inClass: boolean;
  usesThis: boolean;
  staticContext: boolean;
  duplicates: number;
  className?: string | null;
}

export interface ParameterChoice {
  original: string;
  name: string;
}

export interface ExtractMethodOptions {
  name: string;
  visibility?: Visibility | null;
  isStatic: boolean;
  parameters: ParameterChoice[];
  replaceDuplicates: boolean;
}

export type InlineKind = "variable" | "method" | "function" | "constant";
export type InlineMode = "all" | "allKeep" | "this";

export interface InlineInfo {
  kind: InlineKind;
  /** "Inline Variable", "Inline Method"... */
  title: string;
  /** `$total`, `total()`, `MAX`. */
  name: string;
  onReference: boolean;
  occurrences: number;
  readOnly: boolean;
}

export const refactorApi = {
  introducePrepare: (path: string, kind: IntroduceKind, range: LspRange, content: string) =>
    invoke<IntroducePrepareAnswer>("refactor_introduce_prepare", {
      path,
      kind,
      start: range.start,
      end: range.end,
      content,
    }),
  introduce: (
    path: string,
    kind: IntroduceKind,
    range: LspRange,
    content: string,
    options: IntroduceOptions,
    buffers?: Record<string, string>,
  ) =>
    invoke<RefactorAnswer>("refactor_introduce", {
      path,
      kind,
      start: range.start,
      end: range.end,
      content,
      options,
      buffers,
    }),
  extractMethodPrepare: (path: string, range: LspRange, content: string) =>
    invoke<ExtractMethodInfo>("refactor_extract_method_prepare", {
      path,
      start: range.start,
      end: range.end,
      content,
    }),
  extractMethod: (path: string, range: LspRange, content: string, options: ExtractMethodOptions) =>
    invoke<RefactorAnswer>("refactor_extract_method", {
      path,
      start: range.start,
      end: range.end,
      content,
      options,
    }),
  inlinePrepare: (path: string, position: LspPosition, content: string, buffers?: Record<string, string>) =>
    invoke<InlineInfo>("refactor_inline_prepare", {
      path,
      position,
      content,
      buffers,
    }),
  inline: (path: string, position: LspPosition, content: string, mode: InlineMode, buffers?: Record<string, string>) =>
    invoke<RefactorAnswer>("refactor_inline", {
      path,
      position,
      content,
      mode,
      buffers,
    }),
  /** A language server's refactorings for a Refactor menu action (JS/TS). */
  lspActions: (path: string, range: LspRange, refactoring: string) =>
    invoke<ContextAction[]>("code_refactor_actions", {
      path,
      line: range.start.line,
      character: range.start.character,
      endLine: range.end.line,
      endCharacter: range.end.character,
      refactoring,
    }),
};
