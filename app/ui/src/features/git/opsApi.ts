/**
 * Typed wrappers for the Git menu dialogs and the shelf
 * (`app/src-tauri/src/commands/git_ops.rs`, `forge_vcs::ops`, `forge_vcs::shelf`).
 */
import { invoke } from "@/ipc";
import type { ChangedFile, RemoteResult } from "./api";

export interface PullOptions {
  remote: string;
  /** Branches of the remote (`main` or `origin/main`); empty pulls the upstream. */
  branches: string[];
  rebase?: boolean;
  ffOnly?: boolean;
  noFf?: boolean;
  squash?: boolean;
  noCommit?: boolean;
  noVerify?: boolean;
}

export interface MergeOptions {
  branches: string[];
  noFf?: boolean;
  ffOnly?: boolean;
  squash?: boolean;
  /** `-m`: the merge commit message. */
  message?: string | null;
  noCommit?: boolean;
  allowUnrelatedHistories?: boolean;
  noVerify?: boolean;
}

export interface RebaseOptions {
  /** Branch to rebase; empty means the current branch. */
  branch?: string | null;
  onto: string;
  /** `--onto` mode: move the commits after `from` onto `onto`. */
  from?: string | null;
  root?: boolean;
  rebaseMerges?: boolean;
  keepEmpty?: boolean;
  updateRefs?: boolean;
}

export type ResetMode = "soft" | "mixed" | "hard" | "keep";

export interface TagOptions {
  name: string;
  commit?: string | null;
  message?: string | null;
  force?: boolean;
}

export type OngoingOperation = "rebase" | "merge" | "cherryPick" | "revert";
export type OperationAction = "continue" | "abort" | "skip";

export interface OperationState {
  operation: OngoingOperation | null;
  branch: string | null;
  onto: string | null;
}

export interface FileContents {
  before: string | null;
  after: string | null;
}

export type PatchStatus = "added" | "modified" | "deleted";

export interface ShelvedFile {
  path: string;
  status: PatchStatus;
  binary: boolean;
}

export interface ShelvedChangelist {
  id: string;
  name: string;
  /** Unix seconds. */
  date: number;
  branch: string | null;
  files: ShelvedFile[];
}

export interface ShelfFileContents extends FileContents {
  binary: boolean;
}

export const opsApi = {
  pull: (options: PullOptions) => invoke<RemoteResult>("git_pull_with", { options }),
  /** Pull a remote branch (`origin/main`) into the current branch, using merge or rebase. */
  pullInto: (branch: string, rebase: boolean) => invoke<RemoteResult>("git_pull_into", { branch, rebase }),
  /** Fetch and fast-forward a local branch that is not checked out. */
  updateBranch: (name: string) => invoke<RemoteResult>("git_update_branch", { name }),
  merge: (options: MergeOptions) => invoke<string>("git_merge_with", { options }),
  rebase: (options: RebaseOptions) => invoke<string>("git_rebase_with", { options }),
  operationState: () => invoke<OperationState>("git_operation_state"),
  operationAction: (action: OperationAction) => invoke<string>("git_operation_action", { action }),
  reset: (rev: string, mode: ResetMode) => invoke<void>("git_reset_head", { rev, mode }),
  tag: (options: TagOptions) => invoke<void>("git_tag_with", { options }),
  /** The full hash of `rev`, or null when it is not a commit. */
  resolveCommit: (rev: string) => invoke<string | null>("git_resolve_commit", { rev }),
  editRemote: (oldName: string, name: string, url: string) => invoke<void>("git_edit_remote", { oldName, name, url }),
  stashFiles: (index: number) => invoke<ChangedFile[]>("git_stash_files", { index }),
  stashFile: (index: number, path: string) => invoke<FileContents>("git_stash_file", { index, path }),
  stashClear: () => invoke<void>("git_stash_clear"),
  stashBranch: (index: number, branch: string) => invoke<string>("git_stash_branch", { index, branch }),
  shelfList: () => invoke<ShelvedChangelist[]>("git_shelf_list"),
  shelve: (paths: string[], name: string) => invoke<ShelvedChangelist>("git_shelve", { paths, name }),
  /** `paths` unshelves only those files; `remove` drops the applied files from the shelf. */
  unshelve: (id: string, paths: string[] | null, remove: boolean) => invoke<string>("git_unshelve", { id, paths, remove }),
  shelfDelete: (id: string) => invoke<void>("git_shelf_delete", { id }),
  shelfRename: (id: string, name: string) => invoke<ShelvedChangelist>("git_shelf_rename", { id, name }),
  shelfFile: (id: string, path: string) => invoke<ShelfFileContents>("git_shelf_file", { id, path }),
};
