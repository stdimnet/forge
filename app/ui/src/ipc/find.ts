/**
 * Find in Files (Rust: `crates/forge-find`, walked by `forge-core` through the project's
 * backend, so remote projects are searched on their host). Columns and ranges are UTF-16.
 */
import { invoke } from "./index";

export interface FindRequest {
  query: string;
  caseSensitive?: boolean;
  wholeWords?: boolean;
  regex?: boolean;
  /** Comma-separated globs, `!` excludes: `*.php, !*Test.php`. */
  fileMask?: string;
  /** Project-relative directory ("Directory" scope); empty = whole project. */
  directory?: string;
  /** Maximum number of matches. */
  limit?: number;
}

export interface LineMatch {
  /** 0-based line. */
  line: number;
  /** Column of the first match in the line. */
  column: number;
  length: number;
  /** The line as listed (trimmed, long lines cut around the match). */
  text: string;
  /** `[start, end)` of every match inside `text`. */
  ranges: [number, number][];
}

export interface FileMatches {
  path: string;
  matches: LineMatch[];
}

export interface FindResult {
  files: FileMatches[];
  matchCount: number;
  truncated: boolean;
  /** An invalid query (bad regular expression). */
  error?: string;
}

export const findApi = {
  search: (request: FindRequest) => invoke<FindResult>("find_in_files", { request }),
};
