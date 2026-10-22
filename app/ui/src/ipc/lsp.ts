/**
 * Language server IPC (see app/src-tauri/src/commands/lsp.rs). Paths are workspace
 * relative; positions are LSP positions: 0-based line, UTF-16 character.
 */
import { invoke, listen } from "./index";

export interface LspPosition {
  line: number;
  character: number;
}

export interface LspRange {
  start: LspPosition;
  end: LspPosition;
}

/** `typo` and `grammar` come from the spelling and grammar inspections (`features/spelling`). */
export type Severity = "error" | "warning" | "weak" | "info" | "typo" | "grammar";

export interface EditorDiagnostic {
  range: LspRange;
  severity: Severity;
  message: string;
  source?: string | null;
  code?: string | null;
  unnecessary?: boolean;
  deprecated?: boolean;
  /** About the file, not a range in it (a typo in the file name): no squiggle. */
  fileLevel?: boolean;
  /** Data of the inspection that reported it (Alt+Enter fixes). */
  data?: unknown;
}

export interface EditorHover {
  /** Markdown. */
  contents: string;
  range?: LspRange | null;
}

export interface EditorCompletionItem {
  label: string;
  /** "method", "function", "class", "property", "variable", "keyword", ... */
  kind: string;
  detail?: string | null;
  documentation?: string | null;
  insertText?: string | null;
  replaceRange?: LspRange | null;
  filterText?: string | null;
  sortText?: string | null;
  deprecated?: boolean;
  raw?: unknown;
}

export interface EditorCompletionList {
  isIncomplete: boolean;
  items: EditorCompletionItem[];
}

export interface EditorLocation {
  path: string;
  range: LspRange;
}

export interface EditorSymbol {
  name: string;
  detail?: string | null;
  kind: string;
  range: LspRange;
  selectionRange: LspRange;
  children: EditorSymbol[];
}

export interface EditorSignatureHelp {
  signatures: {
    label: string;
    documentation?: string | null;
    /** `[start, end)` offsets into `label`. */
    parameters: { label: [number, number]; documentation?: string | null }[];
    activeParameter?: number | null;
  }[];
  activeSignature: number;
  activeParameter: number;
}

export interface TextEdit {
  range: LspRange;
  newText: string;
}

export type ServerState =
  | { state: "notStarted" }
  | { state: "starting" }
  | { state: "ready" }
  | { state: "busy"; message: string; percentage?: number | null }
  | { state: "stopped" }
  | { state: "failed"; error: string };

export type ServerStatus = { id: string; name: string } & ServerState;

export interface DiagnosticsEvent {
  path: string;
  diagnostics: EditorDiagnostic[];
}

export interface LspServerConfig {
  id: string;
  name?: string;
  languageIds: string[];
  fileExtensions: string[];
  command: string;
  args?: string[];
  initializationOptions?: unknown;
  settings?: unknown;
}

/** An inlay hint as a server reports it (`forge_codeintel::inlay::RawInlayHint`). */
export interface RawInlayHint {
  line: number;
  character: number;
  label: string;
  /** LSP `InlayHintKind`: 1 = type, 2 = parameter. */
  kind: number | null;
  paddingLeft: boolean;
  paddingRight: boolean;
  tooltip?: string | null;
}

export type InlayGroup = "parameterName" | "variableType" | "returnType" | "other";

/** An inlay hint to draw: grouped and filtered by Settings | Editor | Inlay Hints. */
export interface EditorInlayHint {
  line: number;
  /** UTF-16 column. */
  character: number;
  label: string;
  group: InlayGroup;
  paddingLeft: boolean;
  paddingRight: boolean;
  tooltip?: string;
}

const at = (path: string, pos: LspPosition) => ({ path, line: pos.line, character: pos.character });

export const lspApi = {
  didOpen: (path: string, text: string) => invoke<void>("lsp_did_open", { path, text }),
  didChange: (path: string, text: string) => invoke<void>("lsp_did_change", { path, text }),
  didSave: (path: string, text: string) => invoke<void>("lsp_did_save", { path, text }),
  didClose: (path: string) => invoke<void>("lsp_did_close", { path }),
  hover: (path: string, pos: LspPosition) => invoke<EditorHover | null>("lsp_hover", at(path, pos)),
  completion: (path: string, pos: LspPosition) =>
    invoke<EditorCompletionList | null>("lsp_completion", at(path, pos)),
  completionResolve: (path: string, item: unknown) =>
    invoke<EditorCompletionItem | null>("lsp_completion_resolve", { path, item }),
  definition: (path: string, pos: LspPosition) => invoke<EditorLocation[]>("lsp_definition", at(path, pos)),
  references: (path: string, pos: LspPosition) => invoke<EditorLocation[]>("lsp_references", at(path, pos)),
  documentSymbols: (path: string) => invoke<EditorSymbol[]>("lsp_document_symbols", { path }),
  signatureHelp: (path: string, pos: LspPosition) =>
    invoke<EditorSignatureHelp | null>("lsp_signature_help", at(path, pos)),
  /** Reformat Code; `languageId` (the file type) selects the code style options for the server. */
  format: (path: string, tabSize = 4, insertSpaces = true, languageId: string | null = null) =>
    invoke<TextEdit[]>("lsp_format", { path, tabSize, insertSpaces, languageId }),
  /** Hints of lines `startLine..=endLine` of the editor's `text`. */
  inlayHints: (path: string, startLine: number, endLine: number, text: string, languageId: string | null) =>
    invoke<EditorInlayHint[]>("lsp_inlay_hints", { path, startLine, endLine, text, languageId }),
  status: () => invoke<ServerStatus[]>("lsp_status"),
  registerServer: (config: LspServerConfig) => invoke<void>("lsp_register_server", { config }),
  onDiagnostics: (cb: (e: DiagnosticsEvent) => void) => listen<DiagnosticsEvent>("lsp://diagnostics", cb),
  onStatus: (cb: (s: ServerStatus[]) => void) => listen<ServerStatus[]>("lsp://status", cb),
};
