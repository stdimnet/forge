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

export type Severity = "error" | "warning" | "weak" | "info";

export interface EditorDiagnostic {
  range: LspRange;
  severity: Severity;
  message: string;
  source?: string | null;
  code?: string | null;
  unnecessary?: boolean;
  deprecated?: boolean;
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
  format: (path: string, tabSize = 4, insertSpaces = true) =>
    invoke<TextEdit[]>("lsp_format", { path, tabSize, insertSpaces }),
  status: () => invoke<ServerStatus[]>("lsp_status"),
  registerServer: (config: LspServerConfig) => invoke<void>("lsp_register_server", { config }),
  onDiagnostics: (cb: (e: DiagnosticsEvent) => void) => listen<DiagnosticsEvent>("lsp://diagnostics", cb),
  onStatus: (cb: (s: ServerStatus[]) => void) => listen<ServerStatus[]>("lsp://status", cb),
};
