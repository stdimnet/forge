/**
 * Get from VCS, worktrees and Commit Message History: the open dialogs and the actions
 * behind their buttons. Git work (clone, ls-remote, `git worktree`, the history) happens in
 * Rust (`forge_vcs::{clone, worktree, message_history}`), on the project's machine.
 */
import { createSignal } from "solid-js";
import { askChoice, askConfirm } from "@/features/contextMenu/dialogs";
import { setInputDialog } from "./overlays";
import { setGitDialog } from "./gitDialogs";
import { openRemoteProject, workspace, isRemoteProject } from "@/state/workspace";
import { openProject } from "@/features/windows/projectWindows";
import { joinDir, suggestWorktreePath } from "./extrasModel";
export { joinDir, suggestWorktreePath };
import { extrasApi, type CloneProgress, type NewWorktree, type Worktree } from "./extrasApi";
import { notify, repositoryChanged, runGit } from "./store";
import { setCommitMessage } from "./CommitPanel";
import { t } from "@/i18n";

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
  setCloneProgress({ phase: "Cloning", percent: null, fraction: 0, text: t("git.extras.cloningUrl", { url }) });
  const off = await extrasApi.onCloneProgress((p) => setCloneProgress(p));
  try {
    const { path } = await extrasApi.clone(url, directory, accountId, target.connectionId);
    return path;
  } catch (e) {
    const message = String((e as Error)?.message ?? e);
    if (/cancel/i.test(message)) notify("info", t("git.extras.cloneWasCancelled"));
    else notify("error", t("git.extras.cloneFailed"), message, 12000);
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
  if (workspace() && connectionId) {
    const name = path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() ?? path;
    const choice = await askChoice({
      title: t("git.extras.openProject"),
      message: t("git.extras.wouldYouLikeToOpen", { what: what === "the worktree" ? "worktree" : "project", name }),
      choices: [
        { id: "new", label: t("git.extras.newWindow"), disabled: true, hint: t("git.extras.remoteProjectsOpenInThis") },
        { id: "this", label: t("git.extras.thisWindow") },
      ],
    });
    if (choice !== "this") return;
  }
  try {
    if (connectionId) await openRemoteProject(connectionId, path);
    else await openProject(path, what);
  } catch (e) {
    notify("error", t("git.extras.cannotOpenTheProject"), String((e as Error)?.message ?? e));
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
  notify("success", t("git.extras.worktreeCreated"), options.path);
  if (open) await openWorktree(options.path);
  return true;
}

/** Open a worktree as the project (on the same host for remote projects). */
export async function openWorktree(path: string): Promise<void> {
  const connectionId = isRemoteProject() ? await extrasApi.projectConnection().catch(() => null) : null;
  await offerToOpen(path, connectionId, "the worktree");
}

export async function removeWorktree(w: Worktree): Promise<void> {
  if (w.main || w.current) return notify("info", w.main ? t("git.extras.theMainWorktreeCannotBe") : t("git.extras.theWorktreeOfThisProject"));
  const ok = await askConfirm({
    title: t("git.extras.removeWorktree"),
    message: `Remove the worktree ${w.path}${w.branch ? ` (${w.branch})` : ""}? Its directory is deleted; the branch is kept.`,
    confirm: t("common.remove"),
  });
  if (!ok) return;
  try {
    await extrasApi.removeWorktree(w.path, false);
  } catch (e) {
    const message = String((e as Error)?.message ?? e);
    if (!/modified or untracked|locked|--force/i.test(message)) return notify("error", t("git.extras.cannotRemoveTheWorktree"), message);
    const force = await askConfirm({
      title: t("git.extras.removeWorktree"),
      message: `${message}\n\nRemove it anyway and discard its local changes?`,
      confirm: t("git.extras.forceRemove"),
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
  notify("success", t("git.extras.worktreesPruned"), out || "Nothing to prune");
  await refreshWorktrees();
}

export async function toggleWorktreeLock(w: Worktree): Promise<void> {
  if (w.locked) {
    await runGit("Cannot unlock the worktree", () => extrasApi.unlockWorktree(w.path));
  } else {
    setInputDialog({
      title: t("git.extras.lockWorktree"),
      label: t("git.extras.reasonOptional"),
      value: "",
      okLabel: t("git.extras.lock"),
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
