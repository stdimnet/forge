/**
 * Find in Files (Rust: `crates/forge-find`, walked by `forge-core` through the project's
 * backend, so remote projects are searched on their host). Columns and ranges are UTF-16.
 */
import { invoke } from "./index";

export type FindSort = "path" | "matches" | "modified";

export interface FindRequest {
  query: string;
  caseSensitive?: boolean;
  wholeWords?: boolean;
  regex?: boolean;
  /** Comma-separated globs, `!` excludes: `*.php, !*Test.php`. */
  fileMask?: string;
  /** Project-relative directory ("Directory" scope); empty = whole project. */
  directory?: string;
  /** Search only these project-relative files (open or changed files). */
  paths?: string[];
  /** Lines of context around each match (0 to 5). */
  context?: number;
  /** Order of the files in the result. */
  sort?: FindSort;
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
  /** Lines just before and after, when the request asked for `context`. */
  before?: string[];
  after?: string[];
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

/** The totals and first rows of a Find window result; later rows come from `findApi.page`. */
export interface FindPaged {
  id: number;
  matchCount: number;
  truncated: boolean;
  error?: string;
  fileCount: number;
  /** Matching lines over all files. */
  rowCount: number;
  files: FileMatches[];
}

export interface ReplaceLine {
  /** 0-based line. */
  line: number;
  before: string;
  after: string;
}

export interface ReplacePreviewFile {
  path: string;
  lines: ReplaceLine[];
  /** Matches in the file, also those on lines beyond the preview. */
  matches: number;
}

export interface ReplaceDone {
  files: number;
  replacements: number;
  skipped: string[];
}

export interface UndoneReplace {
  restored: number;
  /** Files left alone because they changed after the replace. */
  left: string[];
}

export const findApi = {
  search: (request: FindRequest) => invoke<FindResult>("find_in_files", { request }),
  /** Search once and get the first `pageSize` rows; the full result stays in the backend. */
  searchPaged: (request: FindRequest, pageSize: number) => invoke<FindPaged>("find_in_files_paged", { request, pageSize }),
  /** Rows `[offset, offset + count)` of the result a `searchPaged` call returned `id` for. */
  page: (id: number, offset: number, count: number) => invoke<FileMatches[]>("find_page", { id, offset, count }),
  /** The lines Replace in Files would change. */
  replacePreview: (request: FindRequest, withText: string) => invoke<ReplacePreviewFile[]>("replace_preview", { request, with: withText }),
  /** Replace every match; `undoReplace` puts the files back. */
  replaceAll: (request: FindRequest, withText: string) => invoke<ReplaceDone>("replace_in_files", { request, with: withText }),
  undoReplace: () => invoke<UndoneReplace>("undo_replace"),
};
