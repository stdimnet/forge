/**
 * Get from VCS, worktrees and Commit Message History: the open dialogs and the actions
 * behind their buttons. Git work (clone, ls-remote, `git worktree`, the history) happens in
 * Rust (`forge_vcs::{clone, worktree, message_history}`), on the project's machine.
 */
import { createSignal } from "solid-js";
import { askChoice, askConfirm } from "@/features/contextMenu/dialogs";
import { setInputDialog } from "./overlays";
import { setGitDialog } from "./gitDialogs";
import { openRemoteProject, openWorkspace, workspace, isRemoteProject } from "@/state/workspace";
import { joinDir, suggestWorktreePath } from "./extrasModel";
export { joinDir, suggestWorktreePath };
import { extrasApi, type CloneProgress, type NewWorktree, type Worktree } from "./extrasApi";
import { notify, repositoryChanged, runGit } from "./store";
import { setCommitMessage } from "./CommitPanel";

// ---- Get from VCS --------------------------------------------------------------------------

/** Where the clone goes: this machine, or the host of an SSH connection. */
export interface CloneTarget {
  connectionId: string | null;
  /** `user@host` for remote targets. */
  label: string | null;
}

export interface CloneRequest {
  target: CloneTarget;
  /** Repository URL to start with (e.g. from a link). */
  url?: string;
}


/** A clone in progress: shown in the dialog with Cancel. */
const [cloneProgress, setCloneProgress] = createSignal<CloneProgress | null>(null);
export { cloneProgress };

/** Git ▸ Clone… / Get from VCS: clones on the open remote project's host by default. */
export async function openCloneDialog(url?: string): Promise<void> {
  let target: CloneTarget = { connectionId: null, label: null };
  if (isRemoteProject()) {
    const id = await extrasApi.projectConnection().catch(() => null);
    const r = workspace()?.remote;
    if (id && r) target = { connectionId: id, label: r.label };
  }
  setGitDialog({ kind: "clone", request: { target, url } });
}

/** Run the clone; resolves with the new working copy, or null when it failed / was cancelled. */
export async function runClone(url: string, directory: string, accountId: string | null, target: CloneTarget): Promise<string | null> {
  setCloneProgress({ phase: "Cloning", percent: null, fraction: 0, text: `Cloning ${url}…` });
  const off = await extrasApi.onCloneProgress((p) => setCloneProgress(p));
  try {
    const { path } = await extrasApi.clone(url, directory, accountId, target.connectionId);
    return path;
  } catch (e) {
    const message = String((e as Error)?.message ?? e);
    if (/cancel/i.test(message)) notify("info", "Clone was cancelled");
    else notify("error", "Clone failed", message, 12000);
    return null;
  } finally {
    off();
    setCloneProgress(null);
  }
}

export const cancelClone = () => void extrasApi.cancelClone();

/**
 * After a clone or for a worktree: open the folder as the project. From the Welcome screen it
 * opens right away; with a project open, PHPStorm asks where to open it.
 */
export async function offerToOpen(path: string, connectionId: string | null, what = "the project"): Promise<void> {
  if (workspace()) {
    const name = path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;
    const choice = await askChoice({
      title: "Open Project",
      message: `Would you like to open ${what} ${name} in this window?`,
      choices: [
        { id: "new", label: "New Window", disabled: true, hint: "Forge shows one project per window for now" },
        { id: "this", label: "This Window" },
      ],
    });
    if (choice !== "this") return;
  }
  try {
    if (connectionId) await openRemoteProject(connectionId, path);
    else await openWorkspace(path);
  } catch (e) {
    notify("error", "Cannot open the project", String((e as Error)?.message ?? e));
  }
}

// ---- worktrees -----------------------------------------------------------------------------

const [worktrees, setWorktrees] = createSignal<Worktree[]>([]);
export { worktrees };

export async function refreshWorktrees(): Promise<void> {
  setWorktrees(await extrasApi.worktrees().catch(() => []));
}

/** New Worktree… (optionally for a branch, from the branches popup). */
export async function newWorktreeDialog(branch: string | null = null): Promise<void> {
  await refreshWorktrees();
  setGitDialog({ kind: "newWorktree", branch });
}

export async function createWorktree(options: NewWorktree, open: boolean): Promise<boolean> {
  const out = await runGit("Cannot create the worktree", () => extrasApi.addWorktree(options));
  if (out === undefined) return false;
  await refreshWorktrees();
  await repositoryChanged();
  notify("success", "Worktree created", options.path);
  if (open) await openWorktree(options.path);
  return true;
}

/** Open a worktree as the project (on the same host for remote projects). */
export async function openWorktree(path: string): Promise<void> {
  const connectionId = isRemoteProject() ? await extrasApi.projectConnection().catch(() => null) : null;
  await offerToOpen(path, connectionId, "the worktree");
}

export async function removeWorktree(w: Worktree): Promise<void> {
  if (w.main || w.current) return notify("info", w.main ? "The main worktree cannot be removed" : "The worktree of this project cannot be removed");
  const ok = await askConfirm({
    title: "Remove Worktree",
    message: `Remove the worktree ${w.path}${w.branch ? ` (${w.branch})` : ""}? Its directory is deleted; the branch is kept.`,
    confirm: "Remove",
  });
  if (!ok) return;
  try {
    await extrasApi.removeWorktree(w.path, false);
  } catch (e) {
    const message = String((e as Error)?.message ?? e);
    if (!/modified or untracked|locked|--force/i.test(message)) return notify("error", "Cannot remove the worktree", message);
    const force = await askConfirm({
      title: "Remove Worktree",
      message: `${message}\n\nRemove it anyway and discard its local changes?`,
      confirm: "Force Remove",
    });
    if (!force) return;
    const done = await runGit("Cannot remove the worktree", () => extrasApi.removeWorktree(w.path, true));
    if (done === undefined) return;
  }
  await refreshWorktrees();
  await repositoryChanged();
}

export async function pruneWorktrees(): Promise<void> {
  const out = await runGit("Prune failed", () => extrasApi.pruneWorktrees());
  if (out === undefined) return;
  notify("success", "Worktrees pruned", out || "Nothing to prune");
  await refreshWorktrees();
}

export async function toggleWorktreeLock(w: Worktree): Promise<void> {
  if (w.locked) {
    await runGit("Cannot unlock the worktree", () => extrasApi.unlockWorktree(w.path));
  } else {
    setInputDialog({
      title: "Lock Worktree",
      label: "Reason (optional):",
      value: "",
      okLabel: "Lock",
      optional: true,
      onOk: async (reason) => {
        await runGit("Cannot lock the worktree", () => extrasApi.lockWorktree(w.path, reason.trim() || null));
        await refreshWorktrees();
      },
    });
    return;
  }
  await refreshWorktrees();
}

// ---- Commit Message History ----------------------------------------------------------------

const [messageHistory, setMessageHistory] = createSignal<string[] | null>(null);
export { messageHistory, setMessageHistory };

/** Ctrl+M in the commit message (Vcs.ShowMessageHistory). */
export async function showMessageHistory(): Promise<void> {
  if (!document.querySelector(".commit-message textarea")) return;
  const list = await extrasApi.messageHistory().catch(() => [] as string[]);
  setMessageHistory(list);
}

/** Enter in the popup: the message replaces the commit message, like PHPStorm. */
export function useHistoryMessage(message: string): void {
  setMessageHistory(null);
  setCommitMessage(message);
  queueMicrotask(() => {
    const ta = document.querySelector<HTMLTextAreaElement>(".commit-message textarea");
    ta?.focus();
    ta?.setSelectionRange(message.length, message.length);
  });
}
