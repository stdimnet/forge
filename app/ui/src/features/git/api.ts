/** Typed wrappers for the git IPC commands (`app/src-tauri/src/commands/git.rs`). */
import { invoke } from "@/ipc";

export type FileState =
  | "unmodified"
  | "modified"
  | "added"
  | "deleted"
  | "renamed"
  | "copied"
  | "typechanged"
  | "untracked"
  | "ignored"
  | "conflicted";

export interface StatusEntry {
  path: string;
  origPath: string | null;
  index: FileState;
  worktree: FileState;
  conflict: boolean;
}

export interface RepoStatus {
  branch: string | null;
  head: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  entries: StatusEntry[];
}

export interface RepoInfo {
  root: string;
  branch: string | null;
  head: string | null;
}

export type RefKind = "head" | "local" | "remote" | "tag";
export interface RefLabel {
  name: string;
  kind: RefKind;
  current: boolean;
}

export interface Commit {
  sha: string;
  parents: string[];
  refs: RefLabel[];
  author: string;
  email: string;
  /** Unix seconds. */
  time: number;
  subject: string;
}

export interface ChangedFile {
  path: string;
  origPath: string | null;
  status: string;
}

export interface CommitDetails {
  sha: string;
  parents: string[];
  author: string;
  email: string;
  time: number;
  committer: string;
  commitTime: number;
  message: string;
  files: ChangedFile[];
}

export interface LogQuery {
  skip?: number;
  limit?: number;
  all?: boolean;
  rev?: string | null;
  path?: string | null;
  text?: string | null;
  author?: string | null;
}

export interface BlameCommit {
  sha: string;
  author: string;
  email: string;
  time: number;
  summary: string;
  uncommitted: boolean;
}

export interface Blame {
  commits: BlameCommit[];
  /** Per 0-based line: index into `commits`. */
  lines: number[];
}

export interface BranchInfo {
  name: string;
  sha: string;
  upstream: string | null;
  ahead: number;
  behind: number;
  time: number;
  subject: string;
  current: boolean;
}

export interface Branches {
  current: string | null;
  local: BranchInfo[];
  remote: BranchInfo[];
  tags: BranchInfo[];
  recent: string[];
}

export interface RemoteResult {
  output: string;
}

export const GIT_CHANGED_EVENT = "git://changed";

export const gitApi = {
  info: () => invoke<RepoInfo | null>("git_info"),
  status: () => invoke<RepoStatus>("git_status"),
  show: (rev: string, path: string) => invoke<string | null>("git_show", { rev, path }),
  diff: (paths: string[], staged: boolean) => invoke<string>("git_diff", { paths, staged }),
  diffHead: (paths: string[]) => invoke<string>("git_diff_head", { paths }),
  blame: (path: string, contents?: string) => invoke<Blame>("git_blame", { path, contents: contents ?? null }),
  log: (query: LogQuery) => invoke<Commit[]>("git_log", { query }),
  commitDetails: (sha: string) => invoke<CommitDetails>("git_commit_details", { sha }),
  branches: () => invoke<Branches>("git_branches"),
  checkout: (rev: string, remote = false) => invoke<void>("git_checkout", { rev, remote }),
  createBranch: (name: string, start: string | null, checkout: boolean) =>
    invoke<void>("git_create_branch", { name, start, checkout }),
  deleteBranch: (name: string, force = false) => invoke<void>("git_delete_branch", { name, force }),
  merge: (rev: string) => invoke<string>("git_merge", { rev }),
  rebase: (onto: string) => invoke<string>("git_rebase", { onto }),
  stage: (paths: string[]) => invoke<void>("git_stage", { paths }),
  unstage: (paths: string[]) => invoke<void>("git_unstage", { paths }),
  rollback: (paths: string[]) => invoke<void>("git_rollback", { paths }),
  commit: (message: string, amend: boolean, paths: string[] | null) =>
    invoke<string>("git_commit", { message, amend, paths }),
  push: (force = false) => invoke<RemoteResult>("git_push", { force }),
  /** Without `rebase` the update method of the Git settings applies. */
  pull: (rebase?: boolean) => invoke<RemoteResult>("git_pull", { rebase: rebase ?? null }),
  /** Files among `paths` that would commit CRLF line separators. */
  crlfFiles: (paths: string[]) => invoke<string[]>("git_crlf_files", { paths }),
  fetch: () => invoke<RemoteResult>("git_fetch"),
};

/** Single status for display (tree/tab colors, commit window icons). */
export function displayStatus(e: StatusEntry): "modified" | "added" | "deleted" | "untracked" | "conflict" {
  if (e.conflict) return "conflict";
  if (e.worktree === "untracked") return "untracked";
  if (e.index === "deleted" || e.worktree === "deleted") return "deleted";
  if (e.index === "added" || e.index === "copied") return "added";
  return "modified";
}
