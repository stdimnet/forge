/**
 * Local History (`forge-localhistory`): revisions of files and folders recorded on save,
 * refactorings, VCS operations and external changes, labels, revert, selection history and
 * Recent Changes. A revision is the state *before* its change (for a label: at the label).
 */
import { invoke, listen } from "./index";

export type ChangeKind =
  | "save"
  | "edit"
  | "refactoring"
  | "external"
  | "vcs"
  | "create"
  | "delete"
  | "rename"
  | "move"
  | "copy"
  | "revert"
  | "label"
  | "baseline";

/** `current`: nothing changed the file since the revision. */
export type StateKind = "unknown" | "absent" | "dir" | "file" | "current";

export interface LhRevision {
  id: number;
  /** Milliseconds since the epoch. */
  time: number;
  kind: ChangeKind;
  name: string;
  /** The file's path in this revision (renames are followed); the folder for folders. */
  path: string;
  /** Folder history: the changed files (at most 100). */
  files: string[];
  fileCount: number;
}

export interface LhContent {
  state: StateKind;
  text?: string;
}

export interface LhFolderChange {
  path: string;
  then: StateKind;
  now: StateKind;
}

export interface LhChangeFile {
  path: string;
  from?: string;
  before: StateKind;
  after: StateKind;
}

export interface LhRecentChange {
  from: number;
  to: number;
  time: number;
  kind: ChangeKind;
  name: string;
  files: string[];
  fileCount: number;
}

export interface LhBlockRevision {
  id: number;
  time: number;
  kind: ChangeKind;
  name: string;
  path: string;
  text: string;
  startLine: number;
}

export interface LhSelectionHistory {
  current: string;
  revisions: LhBlockRevision[];
}

export type LhRevertTarget =
  | { kind: "file"; path: string; revPath: string; rev: number }
  | { kind: "folder"; path: string; rev: number }
  | { kind: "change"; from: number; to: number };

export interface LhRevertResult {
  changed: string[];
  skipped: string[];
  failed: string[];
}

export interface LhRecordedFile {
  path: string;
  before: string | null;
  after: string | null;
}

export const LOCAL_HISTORY_CHANGED = "local-history://changed";

export const localHistoryApi = {
  list: (path: string, dir: boolean, filter = "") => invoke<LhRevision[]>("local_history_list", { path, dir, filter }),
  content: (path: string, rev: number) => invoke<LhContent>("local_history_content", { path, rev }),
  folderChanges: (path: string, rev: number) => invoke<LhFolderChange[]>("local_history_folder_changes", { path, rev }),
  selection: (path: string, text: string, start: number, end: number) =>
    invoke<LhSelectionHistory>("local_history_selection", { path, text, start, end }),
  recent: (limit = 50) => invoke<LhRecentChange[]>("local_history_recent", { limit }),
  changeFiles: (from: number, to: number) => invoke<LhChangeFile[]>("local_history_change_files", { from, to }),
  putLabel: (name: string) => invoke<number>("local_history_put_label", { name }),
  patch: (path: string, dir: boolean, revPath: string, rev: number) =>
    invoke<string>("local_history_patch", { path, dir, revPath, rev }),
  revert: (target: LhRevertTarget, name: string) => invoke<LhRevertResult>("local_history_revert", { target, name }),
  /** Changes the UI made itself (open editors of a refactoring). */
  record: (kind: ChangeKind, name: string, files: LhRecordedFile[]) => invoke<void>("local_history_record", { kind, name, files }),
  /** Start an operation; what is recorded until `end(token)` becomes one change set. */
  begin: (kind: ChangeKind, name: string) => invoke<number>("local_history_begin", { kind, name }),
  end: (token: number) => invoke<void>("local_history_end", { token }),
  /** Keep the current content of files an operation is about to change. */
  snapshot: (paths: string[]) => invoke<void>("local_history_snapshot", { paths }),
  onChanged: (cb: () => void) => listen<null>(LOCAL_HISTORY_CHANGED, () => cb()),
};

/**
 * Run an operation as one Local History change set called `name` (e.g. "Update from VCS").
 * Recording problems never stop the operation.
 */
export async function withLocalHistory<T>(kind: ChangeKind, name: string, op: () => Promise<T>, snapshot?: string[]): Promise<T> {
  let token: number | null = null;
  try {
    if (snapshot?.length) await localHistoryApi.snapshot(snapshot);
    token = await localHistoryApi.begin(kind, name);
  } catch {
    token = null;
  }
  try {
    return await op();
  } finally {
    if (token !== null) void localHistoryApi.end(token).catch(() => undefined);
  }
}
