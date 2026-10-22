/**
 * Extract / Introduce and Inline refactorings (see app/src-tauri/src/commands/refactor.rs):
 * Rust analyses PHP code and computes the edits; language servers' refactorings for other
 * files come through `code_refactor_actions`. Positions are LSP positions (0-based line,
 * UTF-16 character).
 */
import { invoke } from "./index";
import type { LspPosition, LspRange } from "./lsp";
import type { ContextAction, FilePreview, RenameConflict, UsagesAnswer, WorkspaceEdit } from "./codeintel";

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

// ---- Move / Copy / Safe Delete / Change Signature / Make Static ----

/** The class Move (F6) and Copy (F5) work on. */
export interface ClassTarget {
  kind: "class";
  /** The declaring file and the class name's position there. */
  path: string;
  position: LspPosition;
  name: string;
  fqn: string;
  namespace: string;
  classKind: "class" | "interface" | "trait" | "enum";
  /** Alone in its file (the file moves with it). */
  sole: boolean;
  directory: string;
}

export interface MovableMember {
  name: string;
  kind: "method" | "property" | "constant";
  signature: string;
  visibility: Visibility;
}

export interface MembersTarget {
  path: string;
  position: LspPosition;
  className: string;
  classFqn: string;
  members: MovableMember[];
  selected?: string | null;
}

/** A function Move Function moves to another file. */
export interface FunctionTarget {
  path: string;
  position: LspPosition;
  name: string;
  fqn: string;
  signature: string;
}

export interface MovePrepareAnswer {
  kind: "class" | "members" | "function";
  class?: ClassTarget | null;
  members?: MembersTarget | null;
  function?: FunctionTarget | null;
}

export interface PathMove {
  from: string;
  to: string;
}

export interface MoveFilesAnswer {
  /** Text edits for the paths after the move (the UI moves the files first). */
  answer: RefactorAnswer;
  /** Every file that moves. */
  files: PathMove[];
}

export interface SafeDeleteAnswer {
  /** `class`, `method`, `parameter`, ... (`file`, `directory`, `files` in the Project view). */
  kind: string;
  name: string;
  /** The Usages Detected text. */
  message: string;
  count: number;
  inComments: number;
  usages: UsagesAnswer;
  answer: RefactorAnswer;
  /** Folders and files to delete outside the workspace edit. */
  paths: string[];
}

export interface SignatureParam {
  name: string;
  typeHint?: string | null;
  defaultValue?: string | null;
  byRef?: boolean;
  variadic?: boolean;
  promotion?: string | null;
  attributes?: string | null;
  used?: boolean;
}

export interface SignatureInfo {
  path: string;
  position: LspPosition;
  kind: "method" | "function";
  name: string;
  className?: string | null;
  visibility?: Visibility | null;
  isStatic: boolean;
  isAbstract: boolean;
  constructor: boolean;
  returnType?: string | null;
  parameters: SignatureParam[];
  /** The current signature as code. */
  signature: string;
}

export interface NewParam {
  /** Index in the current signature; `null` for an added parameter. */
  original: number | null;
  name: string;
  typeHint?: string | null;
  defaultValue?: string | null;
  /** "Default value for call sites" of an added parameter. */
  callValue?: string | null;
  byRef?: boolean;
  variadic?: boolean;
}

export interface ChangeSignatureOptions {
  name: string;
  visibility?: Visibility | null;
  /** Empty for none. */
  returnType: string;
  parameters: NewParam[];
}

export interface MakeStaticInfo {
  path: string;
  position: LspPosition;
  name: string;
  className: string;
  usesThis: boolean;
  fields: string[];
  needsObject: boolean;
  parameter: string;
}

export interface MakeStaticOptions {
  objectParameter?: string | null;
  fieldParameters: boolean;
}

export const moveApi = {
  prepare: (path: string, position: LspPosition, content: string, copy = false, buffers?: Record<string, string>) =>
    invoke<MovePrepareAnswer>("refactor_move_prepare", { path, position, content, copy, buffers }),
  moveClass: (
    target: ClassTarget,
    opts: { namespace: string; directory: string; searchInComments: boolean; content?: string },
    buffers?: Record<string, string>,
  ) =>
    invoke<RefactorAnswer>("refactor_move_class", {
      path: target.path,
      position: target.position,
      content: opts.content,
      targetNamespace: opts.namespace,
      targetDirectory: opts.directory,
      searchInComments: opts.searchInComments,
      buffers,
    }),
  copyClass: (
    target: ClassTarget,
    opts: { name: string; namespace: string; directory: string; content?: string },
    buffers?: Record<string, string>,
  ) =>
    invoke<RefactorAnswer>("refactor_copy_class", {
      path: target.path,
      position: target.position,
      content: opts.content,
      newName: opts.name,
      targetNamespace: opts.namespace,
      targetDirectory: opts.directory,
      buffers,
    }),
  moveMembers: (target: MembersTarget, members: string[], to: string, content?: string, buffers?: Record<string, string>) =>
    invoke<RefactorAnswer>("refactor_move_members", {
      path: target.path,
      position: target.position,
      content,
      members,
      target: to,
      buffers,
    }),
  moveFunction: (target: FunctionTarget, to: string, content?: string, buffers?: Record<string, string>) =>
    invoke<RefactorAnswer>("refactor_move_function", { path: target.path, position: target.position, content, target: to, buffers }),
  moveFiles: (moves: PathMove[], searchForReferences: boolean, searchInComments: boolean, buffers?: Record<string, string>) =>
    invoke<MoveFilesAnswer>("refactor_move_files", { moves, searchForReferences, searchInComments, buffers }),
  safeDelete: (path: string, position: LspPosition, content: string, searchInComments: boolean, buffers?: Record<string, string>) =>
    invoke<SafeDeleteAnswer>("refactor_safe_delete_prepare", { path, position, content, searchInComments, buffers }),
  safeDeleteFiles: (paths: string[], searchInComments: boolean, buffers?: Record<string, string>) =>
    invoke<SafeDeleteAnswer>("refactor_safe_delete_files", { paths, searchInComments, buffers }),
  signature: (path: string, position: LspPosition, content: string, buffers?: Record<string, string>) =>
    invoke<SignatureInfo>("refactor_change_signature_prepare", { path, position, content, buffers }),
  changeSignature: (info: SignatureInfo, options: ChangeSignatureOptions, content?: string, buffers?: Record<string, string>) =>
    invoke<RefactorAnswer>("refactor_change_signature", { path: info.path, position: info.position, content, options, buffers }),
  makeStaticInfo: (path: string, position: LspPosition, content: string, buffers?: Record<string, string>) =>
    invoke<MakeStaticInfo>("refactor_make_static_prepare", { path, position, content, buffers }),
  makeStatic: (info: MakeStaticInfo, options: MakeStaticOptions, content?: string, buffers?: Record<string, string>) =>
    invoke<RefactorAnswer>("refactor_make_static", { path: info.path, position: info.position, content, options, buffers }),
};

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

// ---- Extract Interface / Superclass, Pull Members Up / Push Members Down ----

export type HierarchyRefactoring = "extractInterface" | "extractSuperclass" | "pullUp" | "pushDown";

/** A member of a class as the hierarchy dialogs list it. */
export interface ClassMember {
  /** `orders()`, `$count`, `ROLE`: unique in the class. */
  id: string;
  name: string;
  kind: "method" | "property" | "constant";
  signature: string;
  visibility: Visibility;
  isStatic: boolean;
  isAbstract: boolean;
  constructor: boolean;
  /** The ids of the other members of the class it uses. */
  uses: string[];
}

/** A class or interface of the hierarchy. */
export interface HierarchyClass {
  name: string;
  fqn: string;
  kind: "class" | "interface";
  path: string;
}

export interface HierarchyInfo {
  /** The declaring file and the class name's position there. */
  path: string;
  position: LspPosition;
  directory: string;
  name: string;
  fqn: string;
  namespace: string;
  kind: "class" | "interface" | "trait" | "enum";
  isAbstract: boolean;
  parent?: string | null;
  interfaces: string[];
  members: ClassMember[];
  /** The member at the caret. */
  selected?: string | null;
  /** Pull Members Up: superclasses and interfaces; Push Members Down: direct subclasses. */
  targets: HierarchyClass[];
}

/** A chosen member; `makeAbstract` is "Make abstract" (or "Keep abstract" when pushing down). */
export interface MemberChoice {
  id: string;
  makeAbstract: boolean;
}

export interface ExtractInterfaceOptions {
  name: string;
  namespace: string;
  directory: string;
  members: string[];
  replaceReferences: boolean;
}

export interface ExtractSuperclassOptions {
  name: string;
  namespace: string;
  directory: string;
  members: MemberChoice[];
}

export interface PullUpOptions {
  /** The superclass or interface, fully qualified. */
  target: string;
  members: MemberChoice[];
}

export interface PushDownOptions {
  members: MemberChoice[];
}

export const hierarchyApi = {
  prepare: (path: string, position: LspPosition, content: string, refactoring: HierarchyRefactoring, buffers?: Record<string, string>) =>
    invoke<HierarchyInfo>("refactor_hierarchy_prepare", { path, position, content, refactoring, buffers }),
  extractInterface: (info: HierarchyInfo, options: ExtractInterfaceOptions, content?: string, buffers?: Record<string, string>) =>
    invoke<RefactorAnswer>("refactor_extract_interface", { path: info.path, position: info.position, content, options, buffers }),
  extractSuperclass: (info: HierarchyInfo, options: ExtractSuperclassOptions, content?: string, buffers?: Record<string, string>) =>
    invoke<RefactorAnswer>("refactor_extract_superclass", { path: info.path, position: info.position, content, options, buffers }),
  pullUp: (info: HierarchyInfo, options: PullUpOptions, content?: string, buffers?: Record<string, string>) =>
    invoke<RefactorAnswer>("refactor_pull_up", { path: info.path, position: info.position, content, options, buffers }),
  pushDown: (info: HierarchyInfo, options: PushDownOptions, content?: string, buffers?: Record<string, string>) =>
    invoke<RefactorAnswer>("refactor_push_down", { path: info.path, position: info.position, content, options, buffers }),
};

// ---- Refactor This ----

export interface RefactorThisAnswer {
  /** Forge's refactorings (action ids); `null` when Forge has no analysis for the file. */
  actions?: string[] | null;
  /** The language servers' refactorings (JS/TS). */
  lsp: ContextAction[];
}

export const refactorThisApi = {
  at: (path: string, range: LspRange, content: string) =>
    invoke<RefactorThisAnswer>("code_refactor_this", {
      path,
      line: range.start.line,
      character: range.start.character,
      endLine: range.end.line,
      endCharacter: range.end.character,
      content,
    }),
};

// ---- Extract Component (Vue, React) ----

export interface ExtractComponentInfo {
  framework: "vue" | "react";
  /** A suggested name. */
  name: string;
  /** The props the new component gets. */
  props: string[];
  /** Names the new component imports itself. */
  imports: string[];
  /** The new file's extension (`vue`, `tsx`, `jsx`). */
  extension: string;
  /** The extracted range (bytes, trimmed). */
  start: number;
  end: number;
}

export const extractComponentApi = {
  prepare: (path: string, range: LspRange, content: string) =>
    invoke<ExtractComponentInfo>("refactor_extract_component_prepare", { path, start: range.start, end: range.end, content }),
  extract: (path: string, range: LspRange, content: string, name: string, directory: string) =>
    invoke<RefactorAnswer>("refactor_extract_component", { path, start: range.start, end: range.end, content, name, directory }),
};
