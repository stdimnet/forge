/**
 * Typed wrappers for changelists, partial commits and patches
 * (`app/src-tauri/src/commands/git_changes.rs`, `forge_vcs::{changelists, hunks, patch}`).
 */
import { invoke } from "@/ipc";

/** A run of changed lines of a file against HEAD; lines are 0-based. */
export interface ChangeHunk {
  id: string;
  oldStart: number;
  oldLen: number;
  newStart: number;
  newLen: number;
  removed: string[];
  added: string[];
}

export interface ChangelistFile {
  path: string;
  /** The hunks of the file in this list, `null` for the whole file. */
  hunks: string[] | null;
}

export interface ChangelistView {
  id: string;
  name: string;
  comment: string;
  active: boolean;
  files: ChangelistFile[];
}

/** A file to commit: all of it, or only these hunks. */
export interface CommitFileSpec {
  path: string;
  hunks: string[] | null;
}

export interface CommitOptions {
  amend: boolean;
  signOff: boolean;
  /** "Run Git hooks" unchecked. */
  noVerify: boolean;
  /** `Name <email>`, or null for the configured identity. */
  author: string | null;
}

export type PatchStatus = "added" | "modified" | "deleted";

export interface PatchPreviewFile {
  path: string;
  oldPath: string | null;
  status: PatchStatus;
  binary: boolean;
  additions: number;
  deletions: number;
  before: string | null;
  after: string | null;
  applies: boolean;
  message: string | null;
}

export interface ApplyPatchResult {
  applied: string[];
  conflicts: string[];
  failed: [string, string][];
}

export const changesApi = {
  changelists: () => invoke<ChangelistView[]>("git_changelists"),
  createChangelist: (name: string, comment: string, active: boolean) =>
    invoke<ChangelistView[]>("git_changelist_create", { name, comment, active }),
  editChangelist: (id: string, name: string, comment: string, active: boolean) =>
    invoke<ChangelistView[]>("git_changelist_edit", { id, name, comment, active }),
  setActive: (id: string) => invoke<ChangelistView[]>("git_changelist_set_active", { id }),
  deleteChangelist: (id: string) => invoke<ChangelistView[]>("git_changelist_delete", { id }),
  /** Move files (or, with `hunks`, single hunks of `paths[0]`) to the list `target`; with
   * `create`, `target` is the name of a list to create. */
  move: (paths: string[], target: string, hunks: string[] | null = null, create = false) =>
    invoke<ChangelistView[]>("git_changelist_move", { paths, hunks, target, create }),
  fileHunks: (path: string, content: string | null = null) => invoke<ChangeHunk[]>("git_file_hunks", { path, content }),
  commitFiles: (message: string, files: CommitFileSpec[], options: CommitOptions) =>
    invoke<string>("git_commit_files", { message, files, options }),
  createPatch: (paths: string[], reverse = false) => invoke<string>("git_create_patch", { paths, reverse }),
  readPatch: (path: string) => invoke<string>("git_read_patch", { path }),
  writePatch: (path: string, patch: string) => invoke<void>("git_write_patch", { path, patch }),
  patchPreview: (patch: string, base: string) => invoke<PatchPreviewFile[]>("git_patch_preview", { patch, base }),
  applyPatch: (patch: string, base: string, paths: string[] | null) =>
    invoke<ApplyPatchResult>("git_apply_patch", { patch, base, paths }),
};
