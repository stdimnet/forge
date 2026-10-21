/**
 * Code intelligence IPC (see app/src-tauri/src/commands/codeintel.rs and library.rs): Go to
 * Declaration, Find / Show Usages, Implementation, Type Declaration, Super Method, gutter
 * markers, rename, context actions and read-only library files. Paths are workspace
 * relative, or `library://…` for files outside the project; positions are LSP positions
 * (0-based line, UTF-16 character).
 */
import { invoke } from "./index";
import type { LspPosition, LspRange } from "./lsp";

/** Library files: PHP stubs and files outside the project a server pointed to. */
export const LIBRARY_PREFIX = "library://";
export const STUBS_PREFIX = "library://php-stubs/";
export const LIBRARY_FILE_PREFIX = "library://file/";

export const isLibraryPath = (path: string): boolean => path.startsWith(LIBRARY_PREFIX);

export interface NavTarget {
  path: string;
  /** 0-based line and UTF-16 column of the name. */
  line: number;
  column: number;
  /** `User`, `Model::save()`; may be empty for language server answers. */
  name: string;
  /** Namespace, declaring class or (stubs) the PHP extension. */
  container?: string | null;
}

export interface DeclarationAnswer {
  /** The caret is on a declaration: show its usages instead. */
  onDeclaration: boolean;
  title?: string | null;
  targets: NavTarget[];
  source: "lsp" | "index" | "stubs" | "none";
  message?: string | null;
}

export interface SymbolHover {
  range: LspRange;
  container: string;
  signature: string;
}

export type UsageKind =
  | "import"
  | "traitUse"
  | "extends"
  | "newInstance"
  | "methodCall"
  | "functionCall"
  | "staticAccess"
  | "write"
  | "read"
  | "typeHint"
  | "instanceof"
  | "attribute"
  | "doc"
  | "comment"
  | "stringLiteral"
  | "dynamic"
  | "unclassified";

export interface UsageLocation {
  path: string;
  range: LspRange;
  kind?: UsageKind | null;
}

export interface UsageRow {
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  /** The line, leading whitespace removed. */
  text: string;
  /** UTF-16 offsets into `text`. */
  ranges: [number, number][];
  kind: UsageKind;
}

export interface UsageFile {
  path: string;
  usages: UsageRow[];
}

export interface UsageGroup {
  kind?: UsageKind | null;
  label: string;
  count: number;
  files: UsageFile[];
}

export interface UsageTree {
  total: number;
  fileCount: number;
  groups: UsageGroup[];
}

export interface UsagesAnswer {
  title: string;
  source: "lsp" | "index" | "none";
  declaration?: UsageLocation | null;
  byType: UsageTree;
  byFile: UsageTree;
}

export type NavKind = "implementation" | "superMethod" | "typeDeclaration";

export interface MissingFeature {
  feature: string;
  message: string;
  premium: boolean;
}

export interface NavigateAnswer {
  targets: NavTarget[];
  missing?: MissingFeature | null;
}

export type MarkerKind = "subclassed" | "implemented" | "overridden" | "implements" | "overrides";

export interface LineMarkerInfo {
  /** 0-based line. */
  line: number;
  kind: MarkerKind;
  title: string;
  targets: NavTarget[];
}

export interface LibraryFile {
  text: string;
  /** "PHP Runtime", "External Libraries". */
  library: string;
  /** Where it comes from: "JetBrains phpstorm-stubs 2025.3 (Apache-2.0)", a path. */
  detail: string;
}

export interface TextEditLsp {
  range: LspRange;
  newText: string;
}

export interface WorkspaceEdit {
  changes: (
    | { kind: "edit"; path: string; edits: TextEditLsp[] }
    | { kind: "create"; path: string; overwrite?: boolean }
    | { kind: "rename"; from: string; to: string }
    | { kind: "delete"; path: string }
  )[];
}

export interface AppliedFile {
  path: string;
  before: string | null;
  after: string | null;
}

export interface PreviewRow {
  line: number;
  column: number;
  /** The current line, leading whitespace removed. */
  text: string;
  /** UTF-16 offsets into `text` of the parts that change. */
  ranges: [number, number][];
  after: string;
}

export interface FilePreview {
  path: string;
  operation: "edit" | "create" | "rename" | "delete";
  newPath?: string | null;
  rows: PreviewRow[];
}

export interface PrepareRenameAnswer {
  /** The name to edit (without a leading `$`, see `sigil`). */
  range?: LspRange | null;
  placeholder?: string | null;
  invalid: boolean;
  /** Why nothing can be renamed here. */
  message?: string | null;
  /** The language server cannot rename (Intelephense Premium); `source` says who does. */
  missing?: MissingFeature | null;
  source: "lsp" | "index";
  /** Occurrences in this file (including `range`), edited live by the in-place rename. */
  occurrences: LspRange[];
  /** "Rename class 'User' and its usages to:". */
  label?: string | null;
  /** The class file renamed with the class. */
  classFile?: string | null;
  /** The name has a `$` in front that is not part of `range`. */
  sigil: boolean;
}

export interface RenameConflict {
  message: string;
  path?: string | null;
  /** 0-based. */
  line?: number | null;
}

export interface FileRename {
  from: string;
  to: string;
}

export interface RenameAnswer {
  edit: WorkspaceEdit;
  preview: FilePreview[];
  conflicts: RenameConflict[];
  source: "lsp" | "index";
  missing?: MissingFeature | null;
  fileRename?: FileRename | null;
  /** Dynamic usages that were not renamed. */
  skipped: number;
}

/** How the UI applies a workspace edit (`forge_codeintel::plan_edit`). */
export interface EditPlan {
  editors: { path: string; edits: TextEditLsp[] }[];
  backend: WorkspaceEdit;
  saveFirst: string[];
  reopen: FileRename[];
  close: string[];
  fileCount: number;
}

export type ActionKind = "quickFix" | "intention" | "refactor" | "source";

export interface ContextAction {
  title: string;
  kind: ActionKind;
  source: string;
  isPreferred: boolean;
  disabled?: string | null;
  edit?: WorkspaceEdit | null;
  command?: { title?: string; command: string; arguments?: unknown[] } | null;
  needsResolve: boolean;
  server?: string | null;
  raw?: unknown;
  action?: string | null;
  /** Sub-menu entries ("Fix all ... in file"). */
  children?: ContextAction[];
}

export interface ContextActionsAnswer {
  actions: ContextAction[];
  /** The light bulb for the caret line. */
  bulb?: "fix" | "intention" | null;
  missing?: MissingFeature | null;
}

const at = (path: string, pos: LspPosition, content?: string) => ({
  path,
  line: pos.line,
  character: pos.character,
  content,
});

export const codeApi = {
  gotoDeclaration: (path: string, pos: LspPosition, content?: string) =>
    invoke<DeclarationAnswer>("code_goto_declaration", at(path, pos, content)),
  symbolInfo: (path: string, pos: LspPosition, content?: string) =>
    invoke<SymbolHover | null>("code_symbol_info", at(path, pos, content)),
  findUsages: (path: string, pos: LspPosition, content?: string, buffers?: Record<string, string>) =>
    invoke<UsagesAnswer>("code_find_usages", { ...at(path, pos, content), buffers }),
  navigate: (path: string, pos: LspPosition, kind: NavKind, content?: string) =>
    invoke<NavigateAnswer>("code_navigate", { ...at(path, pos, content), kind }),
  lineMarkers: (path: string, content?: string) => invoke<LineMarkerInfo[]>("code_line_markers", { path, content }),
  prepareRename: (path: string, pos: LspPosition, content?: string) =>
    invoke<PrepareRenameAnswer>("code_prepare_rename", at(path, pos, content)),
  rename: (path: string, pos: LspPosition, newName: string, opts: { buffers?: Record<string, string>; renameFile?: boolean } = {}) =>
    invoke<RenameAnswer>("code_rename", {
      path,
      line: pos.line,
      character: pos.character,
      newName,
      buffers: opts.buffers,
      renameFile: opts.renameFile,
    }),
  planEdit: (edit: WorkspaceEdit, open: string[]) => invoke<EditPlan>("code_plan_edit", { edit, open }),
  applyEdit: (edit: WorkspaceEdit, skip?: string[]) => invoke<AppliedFile[]>("code_apply_edit", { edit, skip }),
  revertEdit: (files: AppliedFile[]) => invoke<void>("code_revert_edit", { files }),
  contextActions: (path: string, range: LspRange, content?: string) =>
    invoke<ContextActionsAnswer>("code_context_actions", {
      path,
      line: range.start.line,
      character: range.start.character,
      endLine: range.end.line,
      endCharacter: range.end.character,
      content,
    }),
  resolveAction: (path: string, server: string, raw: unknown) =>
    invoke<ContextAction>("code_resolve_action", { path, server, raw }),
  executeCommand: (path: string, server: string, command: string, args?: unknown[]) =>
    invoke<unknown>("code_execute_command", { path, server, command, arguments: args }),
  readLibrary: (path: string) => invoke<LibraryFile>("library_read", { path }),
};
