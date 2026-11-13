/**
 * Editor actions computed in Rust (`crates/forge-editing`, `app/src-tauri/src/commands/editor.rs`):
 * postfix completion, Complete Current Statement and Cyclic Expand Word. Browser mode runs
 * the same Rust code as WebAssembly. Offsets are UTF-16 code units, like CodeMirror positions.
 */
import { invoke } from "./index";

export type PostfixLanguage = "php" | "js" | "lua";

/** A postfix template as Settings | Editor | General | Postfix Completion lists it. */
export interface PostfixTemplateInfo {
  /** `php.if`; `editor.postfix.disabled` lists these ids. */
  id: string;
  key: string;
  language: PostfixLanguage;
  /** "PHP", "JavaScript and TypeScript", "Lua". */
  group: string;
  description: string;
  before: string;
  after: string;
}

export interface PostfixRequest {
  /** Forge file type id. */
  language: string;
  /** The caret's line up to the caret. */
  line: string;
  indent?: string;
  disabled?: string[];
}

export interface PostfixItem {
  id: string;
  key: string;
  description: string;
  /** Start of the replaced text in the line; the replacement ends at the caret. */
  from: number;
  expression: string;
  text: string;
  /** Selection after expanding, relative to the start of `text`. */
  selectionFrom: number;
  selectionTo: number;
}

export interface PostfixCompletion {
  /** Start of the typed key in the line. */
  keyFrom: number;
  key: string;
  items: PostfixItem[];
}

export interface StatementRequest {
  language: string;
  text: string;
  offset: number;
  indent?: string;
}

export interface StatementEdit {
  /** Insertions in the original text, sorted. */
  changes: { from: number; to: number; insert: string }[];
  /** Caret in the new text. */
  cursor: number;
}

export interface ExpandRequest {
  text: string;
  offset: number;
  /** Texts of the other open files. */
  others?: string[];
  backward?: boolean;
  language?: string;
}

export interface ExpandResult {
  /** Start of the prefix; it ends at the caret. */
  from: number;
  prefix: string;
  /** Words to cycle through, nearest first. */
  candidates: string[];
}

export const editingApi = {
  postfixTemplates: () => invoke<PostfixTemplateInfo[]>("editor_postfix_templates"),
  postfixComplete: (request: PostfixRequest) => invoke<PostfixCompletion | null>("editor_postfix_complete", { request }),
  completeStatement: (request: StatementRequest) => invoke<StatementEdit | null>("editor_complete_statement", { request }),
  expandWord: (request: ExpandRequest) => invoke<ExpandResult>("editor_expand_word", { request }),
};
