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
  /** GPG/SSH signature (`%G?`); absent when unsigned. */
  signature?: SignatureStatus;
}

/** `%G?` of a commit: good, bad, untrusted (unknown validity), expired, expiredKey, revoked, unverified (cannot check), none. */
export type SignatureStatus = "good" | "bad" | "untrusted" | "expired" | "expiredKey" | "revoked" | "unverified" | "none";

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
  signature?: SignatureStatus;
  /** The signer the signature names (`%GS`). */
  signer?: string | null;
  /** The key that made the signature (`%GK`). */
  signingKey?: string | null;
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
  /** Another worktree that has this branch checked out. */
  worktree?: string | null;
}

/** A node of the branches grouped into folders at each `/` (`forge_vcs::branch_tree`). */
export interface BranchNode {
  kind: "remote" | "folder" | "branch";
  /** The last segment of the name: `tabs` of `feat/ui/tabs`. */
  label: string;
  /** A branch: its name (`feat/ui/tabs`, `origin/main`); a folder or remote: its prefix (`feat/ui`, `origin`). */
  path: string;
  /** The current branch, or a folder that contains it. */
  current: boolean;
  children: BranchNode[];
}

export interface Branches {
  current: string | null;
  local: BranchInfo[];
  remote: BranchInfo[];
  tags: BranchInfo[];
  recent: string[];
  /** `local` in folders: folders first, then branches, the current one leading. */
  localTree: BranchNode[];
  /** `remote` below the remotes, then in folders. */
  remoteTree: BranchNode[];
}

export interface RemoteResult {
  output: string;
}

export interface StashEntry {
  /** 0 is `stash@{0}`, the most recent. */
  index: number;
  branch: string | null;
  message: string;
  time: number;
}

export interface Remote {
  name: string;
  fetchUrl: string;
  /** Only set when it differs from the fetch URL. */
  pushUrl: string | null;
}

// ---- merge conflicts (`forge_vcs::merge`, `Repo::conflicts`) ---------------------------

export type MergeOperation = "merge" | "rebase" | "cherryPick" | "revert" | "unstash";
export type SideChange = "modified" | "added" | "deleted";

export interface ConflictFile {
  path: string;
  yours: SideChange;
  theirs: SideChange;
}

export interface ConflictSet {
  operation: MergeOperation;
  /** "Merging branch feature into branch main". */
  description: string;
  /** For the column headers: "Yours (main)", "Theirs (feature)". */
  yoursName: string;
  theirsName: string;
  /** Merge window pane titles: "Changes from main", "Changes from feature". */
  yoursTitle: string;
  theirsTitle: string;
  /** Yours is git's `theirs` (rebase). */
  reversed: boolean;
  files: ConflictFile[];
}

/** 0-based lines `start..end` (end-exclusive). */
export interface LineRange {
  start: number;
  end: number;
}

/** `left`/`right`: only that side changed; `both`: the same change on both sides. */
export type ChunkKind = "left" | "right" | "both" | "conflict";

export interface MergeChunk {
  kind: ChunkKind;
  base: LineRange;
  left: LineRange;
  right: LineRange;
  baseText: string;
  leftText: string;
  rightText: string;
  /** Word-level merge of a simple conflict ("Resolve Simple Conflicts"). */
  simple: string | null;
}

export interface MergeModel {
  chunks: MergeChunk[];
  changes: number;
  conflicts: number;
  simpleConflicts: number;
}

/** A conflicted file for the merge window; texts use `\n` line breaks. */
export interface MergeFile {
  path: string;
  yoursTitle: string;
  theirsTitle: string;
  base: string | null;
  yours: string | null;
  theirs: string | null;
  binary: boolean;
  crlf: boolean;
  model: MergeModel | null;
}

export type ConflictSide = "yours" | "theirs";

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
  /** Append files or folders to the repository's `.gitignore`; returns the patterns. */
  addToGitignore: (paths: string[]) => invoke<string[]>("git_add_to_gitignore", { paths }),
  createTag: (name: string, rev: string | null = null, message: string | null = null) =>
    invoke<void>("git_create_tag", { name, rev, message }),
  stash: (message: string, keepIndex = false, includeUntracked = false) =>
    invoke<string>("git_stash", { message, keepIndex, includeUntracked }),
  stashList: () => invoke<StashEntry[]>("git_stash_list"),
  unstash: (index: number, pop: boolean, reinstateIndex = false) =>
    invoke<string>("git_unstash", { index, pop, reinstateIndex }),
  dropStash: (index: number) => invoke<void>("git_drop_stash", { index }),
  remotes: () => invoke<Remote[]>("git_remotes"),
  addRemote: (name: string, url: string) => invoke<void>("git_add_remote", { name, url }),
  removeRemote: (name: string) => invoke<void>("git_remove_remote", { name }),
  conflicts: () => invoke<ConflictSet>("git_conflicts"),
  mergeFile: (path: string) => invoke<MergeFile>("git_merge_file", { path }),
  /** Accept Yours / Accept Theirs; a side that deleted the file deletes it. */
  acceptConflictSide: (paths: string[], side: ConflictSide) => invoke<void>("git_accept_conflict_side", { paths, side }),
  /** Write the merged text and mark the file resolved. */
  resolveMerge: (path: string, text: string, crlf: boolean) => invoke<void>("git_resolve_merge", { path, text, crlf }),
};

/** Single status for display (tree/tab colors, commit window icons). */
export function displayStatus(e: StatusEntry): "modified" | "added" | "deleted" | "untracked" | "conflict" {
  if (e.conflict) return "conflict";
  if (e.worktree === "untracked") return "untracked";
  if (e.index === "deleted" || e.worktree === "deleted") return "deleted";
  if (e.index === "added" || e.index === "copied") return "added";
  return "modified";
}
