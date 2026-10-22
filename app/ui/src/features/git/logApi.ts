/**
 * Typed wrappers for the Git Log context menu (`app/src-tauri/src/commands/git_log.rs`,
 * `forge_vcs::rebase`, `Repo::cherry_pick` and friends). Commit lists are in log order
 * (newest first) unless noted.
 */
import { invoke } from "@/ipc";

export interface ActionState {
  enabled: boolean;
  /** Why the action is disabled. */
  reason: string | null;
  /** The action may run after the user confirms (commits already pushed). */
  warning: string | null;
}

export interface LogActionStates {
  cherryPick: ActionState;
  revert: ActionState;
  undoCommit: ActionState;
  editMessage: ActionState;
  fixup: ActionState;
  squashInto: ActionState;
  squashCommits: ActionState;
  dropCommits: ActionState;
  interactiveRebase: ActionState;
}

export type RebaseAction = "pick" | "edit" | "reword" | "squash" | "fixup" | "drop";

export interface RebaseEntry {
  sha: string;
  action: RebaseAction;
  /** New message of a reworded commit or of the commit a squash group results in. */
  message?: string | null;
}

export interface RebaseCommit {
  sha: string;
  subject: string;
  message: string;
  author: string;
  time: number;
}

export interface RebasePlan {
  /** Parent of the oldest commit; null rebases from the root. */
  base: string | null;
  branch: string | null;
  /** Oldest first. */
  commits: RebaseCommit[];
  /** Oldest first, fixup!/squash! commits already placed after their targets. */
  entries: RebaseEntry[];
}

export interface RewriteResult {
  output: string;
  oldHead: string;
  /** null while the rebase waits at an `edit` step. */
  newHead: string | null;
  stopped: boolean;
}

export const logApi = {
  actions: (shas: string[]) => invoke<LogActionStates>("git_log_actions", { shas }),
  cherryPick: (shas: string[]) => invoke<string>("git_cherry_pick", { shas }),
  revert: (shas: string[]) => invoke<string>("git_revert_commits", { shas }),
  undoCommit: (sha: string) => invoke<{ message: string }>("git_undo_commit", { sha }),
  undoRewrite: (oldHead: string, newHead: string) => invoke<void>("git_undo_rewrite", { oldHead, newHead }),
  reword: (sha: string, message: string) => invoke<RewriteResult>("git_reword", { sha, message }),
  squash: (shas: string[], message: string) => invoke<RewriteResult>("git_squash_commits", { shas, message }),
  drop: (shas: string[]) => invoke<RewriteResult>("git_drop_commits", { shas }),
  rebasePlan: (sha: string) => invoke<RebasePlan>("git_rebase_plan", { sha }),
  /** `entries` oldest first. */
  interactiveRebase: (sha: string, entries: RebaseEntry[]) => invoke<RewriteResult>("git_interactive_rebase", { sha, entries }),
  patch: (shas: string[]) => invoke<string>("git_commits_patch", { shas }),
  treeFiles: (rev: string) => invoke<string[]>("git_tree_files", { rev }),
};
