/**
 * Spelling and grammar inspections computed in Rust (`crates/forge-spell`,
 * `commands/spell.rs`). Offsets are UTF-16 document offsets, like CodeMirror's.
 */
import { invoke } from "./index";
import type { ContextAction } from "./codeintel";

/** Where a segment of text comes from. */
export type SpellContext = "comment" | "docComment" | "string" | "identifier" | "text" | "commitMessage" | "fileName";

export interface SpellSegment {
  /** Document offset of `text`. */
  from: number;
  text: string;
  kind: SpellContext;
}

/** The `spelling.*` and `grammar.*` settings. */
export interface SpellOptions {
  spelling: boolean;
  grammar: boolean;
  comments: boolean;
  strings: boolean;
  identifiers: boolean;
  text: boolean;
  commitMessages: boolean;
  fileNames: boolean;
  /** Grammar rule ids that are off: `repeated-word`, `a-an`, `sentence-start`, `double-space`. */
  disabledRules: string[];
}

export interface SpellInspectRequest {
  segments: SpellSegment[];
  options: SpellOptions;
  /** Project-level and application-level dictionary words. */
  words: string[];
}

export interface SpellFix {
  title: string;
  from: number;
  to: number;
  text: string;
}

export interface SpellProblem {
  from: number;
  to: number;
  kind: "typo" | "grammar";
  context: SpellContext;
  /** `Typo: In word 'recieve'`, or the grammar rule's message. */
  message: string;
  word?: string;
  /** The identifier a misspelled word is part of (Rename). */
  identifier?: { from: number; to: number; text: string };
  rule?: string;
  fixes?: SpellFix[];
}

export interface SpellActionsRequest {
  problems: SpellProblem[];
  words: string[];
  hasProject: boolean;
  limit?: number;
}

export interface DictionaryInfo {
  name: string;
  path?: string;
  words: number;
  builtin: boolean;
  error?: string;
}

/** `data` of "Typo: Change to..." / "Typo: Rename to...". */
export interface ChangeToData {
  from: number;
  to: number;
  word: string;
  /** The identifier's current name (Rename). */
  name?: string;
  suggestions: string[];
}

/** `data` of "Save 'word' to project-level / application-level dictionary". */
export interface SaveToData {
  word: string;
  level: "project" | "application";
}

/** `data` of a grammar fix: replace `from..to` with `text`. */
export interface ReplaceData {
  from: number;
  to: number;
  text: string;
}

export const spellApi = {
  inspect: (request: SpellInspectRequest) => invoke<SpellProblem[]>("spell_inspect", { request }),
  contextActions: (request: SpellActionsRequest) => invoke<ContextAction[]>("spell_context_actions", { request }),
  dictionaries: () => invoke<DictionaryInfo[]>("spell_dictionaries"),
};
