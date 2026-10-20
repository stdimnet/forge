/**
 * Git state shared by the Git feature: repository status (also mirrored into
 * `state/vcs.ts` for project tree and tab colors), refresh scheduling, notifications and
 * buffer reloads after rollbacks.
 */
import { createRoot, createSignal, createEffect, on } from "solid-js";
import { listen } from "@/ipc";
import { setCurrentBranch, setFileStatuses, type FileStatus } from "@/state/vcs";
import { editor } from "@/state/editor";
import { onFsChanged } from "@/state/fsWatch";
import { workspaceApi } from "@/ipc/workspace";
import { gitApi, displayStatus, GIT_CHANGED_EVENT, type RepoStatus } from "./api";

const [status, setStatus] = createSignal<RepoStatus | null>(null);
const [repoAvailable, setRepoAvailable] = createSignal(true);
/** Bumped on every repository change so views (log, blame, markers) can refetch. */
const [revision, setRevision] = createSignal(0);

export { status, repoAvailable, revision };

let refreshing: Promise<void> | null = null;
let queued = false;

/** Re-read `git status` (coalesces concurrent calls). */
export async function refreshStatus(): Promise<void> {
  if (refreshing) {
    queued = true;
    return refreshing;
  }
  refreshing = (async () => {
    try {
      const st = await gitApi.status();
      setRepoAvailable(true);
      setStatus(st);
      setCurrentBranch(st.branch ?? (st.head ? st.head.slice(0, 8) : "main"));
      const map: Record<string, FileStatus> = {};
      for (const e of st.entries) map[e.path] = displayStatus(e);
      setFileStatuses(map);
    } catch (err) {
      setRepoAvailable(false);
      setStatus(null);
      setFileStatuses({});
      console.debug("git status unavailable:", err);
    } finally {
      refreshing = null;
    }
    if (queued) {
      queued = false;
      await refreshStatus();
    }
  })();
  return refreshing;
}

/** Refresh status and notify dependent views (log, blame, gutter markers). */
export async function repositoryChanged(): Promise<void> {
  await refreshStatus();
  setRevision((r) => r + 1);
}

// ---- notifications (PHPStorm balloons) ------------------------------------------------

export interface Notice {
  id: number;
  kind: "info" | "success" | "error";
  title: string;
  body?: string;
}
const [notices, setNotices] = createSignal<Notice[]>([]);
export { notices };
let noticeId = 0;

export function notify(kind: Notice["kind"], title: string, body?: string, timeout = 6000): void {
  const id = ++noticeId;
  setNotices((n) => [...n, { id, kind, title, body }]);
  if (timeout > 0) setTimeout(() => dismissNotice(id), timeout);
}

export const dismissNotice = (id: number) => setNotices((n) => n.filter((x) => x.id !== id));

/** Run a git operation, reporting failures as an error balloon. */
export async function runGit<T>(title: string, op: () => Promise<T>): Promise<T | undefined> {
  try {
    const res = await op();
    return res;
  } catch (err) {
    notify("error", title, String(err instanceof Error ? err.message : err), 10000);
    return undefined;
  } finally {
    await repositoryChanged();
  }
}

// ---- buffer reloads -------------------------------------------------------------------

export interface FileReload {
  path: string;
  content: string;
  seq: number;
}
const [fileReload, setFileReload] = createSignal<FileReload | null>(null);
export { fileReload };
let reloadSeq = 0;

/** Re-read open editor buffers from disk (after rollback, checkout, pull, diff edits). */
export async function reloadOpenFiles(paths?: string[]): Promise<void> {
  const open = editor.tabs.map((t) => t.path).filter((p) => !paths || paths.includes(p));
  for (const path of open) {
    try {
      const content = await workspaceApi.readFile(path);
      setFileReload({ path, content, seq: ++reloadSeq });
    } catch {
      /* file may have been removed */
    }
  }
}

// ---- wiring -----------------------------------------------------------------------------

let started = false;

/** Start listening for repository changes; called once by the feature module. */
export function startGitStore(): void {
  if (started) return;
  started = true;
  void listen(GIT_CHANGED_EVENT, () => void repositoryChanged());
  if (typeof window !== "undefined") {
    window.addEventListener("focus", () => void repositoryChanged());
  }
  // Changes made outside Forge (terminal, other tools) arrive from the file watcher.
  onFsChanged((c) => {
    if (c.git) void repositoryChanged();
    else void refreshStatus();
  });
  createRoot(() => {
    // Refresh after saves: watch the saved snapshot of every tab.
    createEffect(
      on(
        () => editor.tabs.map((t) => [t.path, t.saved]),
        () => void repositoryChanged(),
        { defer: true },
      ),
    );
  });
  void repositoryChanged();
}
