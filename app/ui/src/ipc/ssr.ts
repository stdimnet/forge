/**
 * Structural Search and Replace (Rust: `crates/forge-ssr`, run through the project's backend
 * so remote projects are parsed on their host). Lines are 0-based, columns UTF-16.
 */
import { invoke } from "./index";
import type { ScopeFilter } from "./scopes";
import type { ReplaceDone } from "./find";

export type SsrFileType = "php" | "javaScript" | "typeScript";

/** The modifiers of a template variable. */
export interface SsrConstraint {
  name: string;
  minCount?: number;
  /** `null`: unlimited. */
  maxCount?: number | null;
  /** A regular expression the whole text must match. */
  text?: string;
  invertText?: boolean;
  /** A regular expression for the expression's type (class names, `string`, `int`, …). */
  type?: string;
  invertType?: boolean;
}

export interface SsrRequest {
  template: string;
  fileType: SsrFileType;
  constraints?: SsrConstraint[];
  matchCase?: boolean;
  replacement?: string;
  scope?: ScopeFilter;
  /** Unsaved editor contents by path. */
  buffers?: Record<string, string>;
  limit?: number;
}

export interface SsrBinding {
  name: string;
  text: string;
}

export interface SsrMatch {
  line: number;
  column: number;
  endLine: number;
  endColumn: number;
  start: number;
  end: number;
  /** The first line of the match, as listed. */
  text: string;
  ranges: [number, number][];
  matched: string;
  bindings: SsrBinding[];
  replacement?: string;
}

export interface SsrResult {
  files: { path: string; matches: SsrMatch[] }[];
  matchCount: number;
  searchedFiles: number;
  truncated: boolean;
  error?: string;
}

export interface SsrTemplateCheck {
  variables: string[];
  unknownInReplacement?: string[];
  error?: string;
}

export interface SsrSavedTemplate {
  name: string;
  category: string;
  fileType: SsrFileType;
  template: string;
  constraints: SsrConstraint[];
  replacement?: string;
}

export const ssrApi = {
  templates: () => invoke<SsrSavedTemplate[]>("ssr_templates"),
  check: (request: SsrRequest) => invoke<SsrTemplateCheck>("ssr_check", { request }),
  search: (request: SsrRequest) => invoke<SsrResult>("ssr_search", { request }),
  /** Replace every match; `findApi.undoReplace` puts the files back. */
  replace: (request: SsrRequest) => invoke<ReplaceDone>("ssr_replace", { request }),
};
