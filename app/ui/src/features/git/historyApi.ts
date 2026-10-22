/** Typed wrappers for the git history commands (`app/src-tauri/src/commands/git_history.rs`). */
import { invoke } from "@/ipc";
import type { Blame, ChangedFile } from "./api";

/** One revision in the history of a file or folder. */
export interface FileRevision {
  sha: string;
  parents: string[];
  author: string;
  email: string;
  /** Unix seconds. */
  time: number;
  subject: string;
  /** What the commit changed below the path; for a file its name in that revision. */
  files: ChangedFile[];
}

/** One revision of a block of lines (Show History for Selection). */
export interface BlockRevision {
  sha: string;
  parents: string[];
  author: string;
  email: string;
  time: number;
  subject: string;
  /** The file's path in this revision. */
  path: string;
  /** The block before the commit, `null` when the commit added it. */
  before: string | null;
  after: string;
  /** First line (1-based) of the block after the commit. */
  startLine: number;
}

export interface HistoryQuery {
  skip?: number;
  limit?: number;
}

export const historyApi = {
  /** Show History: commits that changed a file (following renames) or a folder. */
  fileHistory: (path: string, dir: boolean, query: HistoryQuery = {}) =>
    invoke<FileRevision[]>("git_file_history", { path, dir, query }),
  /** Show History for Selection: lines `start..end` (1-based) as in HEAD. */
  blockHistory: (path: string, start: number, end: number) => invoke<BlockRevision[]>("git_block_history", { path, start, end }),
  /** Annotate Revision: blame of the file as it was in `rev`. */
  blameRevision: (rev: string, path: string) => invoke<Blame>("git_blame_revision", { rev, path }),
  /** Files below a folder that differ between `rev` and the working tree. */
  changesSince: (rev: string, path: string) => invoke<ChangedFile[]>("git_changes_since", { rev, path }),
};
