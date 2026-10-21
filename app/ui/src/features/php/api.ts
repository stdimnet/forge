/**
 * PHPForge IPC (see app/src-tauri/src/commands/php.rs). Offsets are UTF-16 (JS string
 * indices); the backend converts to bytes.
 */
import { invoke } from "@/ipc";

export type SymbolKind =
  | "namespace"
  | "class"
  | "interface"
  | "trait"
  | "enum"
  | "enumCase"
  | "method"
  | "property"
  | "constant"
  | "function"
  // Kinds of other languages shown in the Structure tool window (Lua).
  | "module"
  | "alias"
  | "variable";

export type Visibility = "public" | "protected" | "private";

export interface Modifiers {
  isStatic?: boolean;
  isAbstract?: boolean;
  isFinal?: boolean;
  isReadonly?: boolean;
}

export interface IndexStats {
  files: number;
  parsed: number;
  cached: number;
  classes: number;
  functions: number;
  durationMs: number;
}

export interface IndexProgress {
  done: number;
  total: number;
}

export interface SymbolHit {
  name: string;
  fqn: string;
  kind: SymbolKind;
  container: string;
  path: string;
  offset: number;
  line: number;
  column: number;
  score: number;
  positions: number[];
  vendor?: boolean;
  deprecated?: boolean;
}

export interface StructureNode {
  name: string;
  kind: SymbolKind;
  detail: string;
  visibility?: Visibility;
  modifiers: Modifiers;
  /** UTF-16 offset of the name in the file (in `path` for inherited members). */
  offset: number;
  /** 0-based line. */
  line: number;
  /** UTF-16 column, when the provider knows it. */
  column?: number;
  deprecated?: boolean;
  inheritedFrom?: string;
  path?: string;
  children?: StructureNode[];
}

export interface FileStructure {
  path: string;
  nodes: StructureNode[];
}

export interface Hover {
  markdown: string;
  start: number;
  end: number;
}

export type GenerateKind =
  | "constructor"
  | "getters"
  | "setters"
  | "gettersAndSetters"
  | "toString"
  | "overrideMethods"
  | "implementMethods"
  | "phpDoc";

export interface GenerateOptions {
  fields?: string[];
  methods?: string[];
  fluent?: boolean;
}

export interface TextEdit {
  start: number;
  end: number;
  newText: string;
}

export interface FieldTarget {
  name: string;
  typeHint?: string;
  visibility: Visibility;
  isStatic: boolean;
  isReadonly: boolean;
  promoted: boolean;
  hasGetter: boolean;
  hasSetter: boolean;
  inConstructor: boolean;
}

export interface MethodTarget {
  id: string;
  name: string;
  owner: string;
  ownerKind: SymbolKind;
  signature: string;
  visibility: Visibility;
  isStatic: boolean;
}

export interface GenerateTargets {
  className?: string;
  classKind?: SymbolKind;
  fields: FieldTarget[];
  hasConstructor: boolean;
  hasToString: boolean;
  overrideMethods: MethodTarget[];
  implementMethods: MethodTarget[];
  phpDocTarget?: string;
}

export type ServerChoice = "auto" | "intelephense" | "phpactor" | "none";

export interface PhpLspSettings {
  server: ServerChoice;
  licenceKey?: string | null;
  phpVersion: string;
  maxFileSize: number;
  extraStubs: string[];
  includePaths: string[];
  intelephensePath?: string | null;
  phpactorPath?: string | null;
}

export interface DetectedServers {
  intelephense?: string | null;
  phpactor?: string | null;
}

export const phpApi = {
  indexWorkspace: () => invoke<IndexStats>("php_index_workspace"),
  searchSymbols: (query: string, kind: "class" | "symbol" = "class", limit = 100) =>
    invoke<SymbolHit[]>("php_search_symbols", { query, kind, limit }),
  fileStructure: (path: string, content?: string, inherited = false) =>
    invoke<FileStructure>("php_file_structure", { path, content, inherited }),
  hover: (path: string, offset: number, content?: string) =>
    invoke<Hover | null>("php_hover", { path, offset, content }),
  generateTargets: (path: string, offset: number, content?: string) =>
    invoke<GenerateTargets>("php_generate_targets", { path, offset, content }),
  generate: (path: string, offset: number, kind: GenerateKind, options: GenerateOptions = {}, content?: string) =>
    invoke<TextEdit[]>("php_generate", { path, offset, kind, options, content }),
  /** Reformat Code: the blank lines of the PHP code style applied to `text`; null = unchanged. */
  tidyBlankLines: (text: string) => invoke<string | null>("php_tidy_blank_lines", { text }),
  detectServers: (settings?: Partial<PhpLspSettings>) =>
    invoke<DetectedServers>("php_detect_servers", { settings }),
};
