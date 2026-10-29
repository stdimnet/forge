/**
 * Typed wrappers for Get from VCS, worktrees, Commit Message History and commit signing
 * (`app/src-tauri/src/commands/git_clone.rs`, `git_extras.rs`; `forge_vcs::{clone, worktree,
 * message_history, signing}`).
 */
import { invoke, listen } from "@/ipc";

export interface HostRepository {
  /** `owner/repo` (`group/sub/project` on GitLab). */
  path: string;
  name: string;
  description: string | null;
  private: boolean;
  fork: boolean;
  cloneUrl: string;
  sshUrl: string | null;
  webUrl: string;
  updatedAt: string;
}

export interface RemoteRef {
  sha: string;
  name: string;
}

export interface CloneProgress {
  phase: string;
  percent: number | null;
  /** The whole clone, 0–1. */
  fraction: number | null;
  text: string;
}

export interface Worktree {
  path: string;
  head: string | null;
  branch: string | null;
  detached: boolean;
  bare: boolean;
  main: boolean;
  current: boolean;
  locked: boolean;
  lockReason: string | null;
  prunable: boolean;
}

export interface NewWorktree {
  path: string;
  branch?: string | null;
  newBranch?: string | null;
  base?: string | null;
  detach?: boolean;
}

export interface GpgKey {
  id: string;
  fingerprint: string;
  userIds: string[];
  expires: number | null;
  canSign: boolean;
  invalid: boolean;
}

export interface SshKey {
  path: string;
  keyType: string;
  comment: string;
}

export interface SigningInfo {
  config: { signingKey: string | null; format: string | null; gpgSign: boolean };
  gpgKeys: GpgKey[];
  gpgError: string | null;
  sshKeys: SshKey[];
  remote: boolean;
}

/** Where git runs: this machine (`null`) or an SSH connection's host. */
type Conn = string | null | undefined;

export const extrasApi = {
  repositories: (accountId: string) => invoke<HostRepository[]>("forges_repositories", { accountId }),
  directoryName: (url: string) => invoke<string | null>("git_clone_directory_name", { url }),
  cloneHome: (connectionId: Conn) => invoke<string>("git_clone_home", { connectionId: connectionId ?? null }),
  testUrl: (url: string, accountId: string | null, connectionId: Conn) =>
    invoke<RemoteRef[]>("git_clone_test", { url, accountId, connectionId: connectionId ?? null }),
  clone: (url: string, directory: string, accountId: string | null, connectionId: Conn) =>
    invoke<{ path: string }>("git_clone", { url, directory, accountId, connectionId: connectionId ?? null }),
  cancelClone: () => invoke<void>("git_clone_cancel"),
  onCloneProgress: (cb: (p: CloneProgress) => void) => listen<CloneProgress>("git://clone-progress", cb),
  projectConnection: () => invoke<string | null>("remote_project_connection"),

  worktrees: () => invoke<Worktree[]>("git_worktrees"),
  addWorktree: (options: NewWorktree) => invoke<string>("git_worktree_add", { options }),
  removeWorktree: (path: string, force: boolean) => invoke<void>("git_worktree_remove", { path, force }),
  pruneWorktrees: () => invoke<string>("git_worktree_prune"),
  lockWorktree: (path: string, reason: string | null) => invoke<void>("git_worktree_lock", { path, reason }),
  unlockWorktree: (path: string) => invoke<void>("git_worktree_unlock", { path }),

  messageHistory: () => invoke<string[]>("git_message_history"),
  signingInfo: () => invoke<SigningInfo>("git_signing_info"),
};
