/**
 * Merge conflicts, like PHPStorm: the "Conflicts" dialog (Git ▸ Resolve Conflicts…), the
 * three-way merge window, the "Merge conflicts" balloon and the choices for deleted and
 * binary files. Reading the stage versions and the merge itself happen in Rust
 * (`forge_vcs::merge`, `Repo::conflicts`).
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { dismissBalloon, showBalloon } from "@/keymap";
import { askChoice } from "@/features/contextMenu/dialogs";
import { gitApi, type ConflictSide, type MergeFile, type MergeOperation } from "./api";
import { notify, reloadOpenFiles, repositoryChanged, status, VCS_GROUP } from "./store";
import { t } from "@/i18n";

const [conflictsOpen, setConflictsOpen] = createSignal(false);
const [mergeFile, setMergeFile] = createSignal<MergeFile | null>(null);
export { conflictsOpen, mergeFile, setMergeFile };

/** Workspace paths with unresolved conflicts, from the last `git status`. */
export const conflictedPaths = (): string[] => (status()?.entries ?? []).filter((e) => e.conflict).map((e) => e.path);
export const hasConflict = (path: string): boolean => conflictedPaths().includes(path);

/** What the open merge window offers to keymap actions (NextDiff, Diff.ApplyNonConflicts, …). */
export interface MergeActions {
  step: (forward: boolean) => void;
  applyAll: (which: "left" | "right" | "all") => void;
  magic: () => void;
}
let mergeActions: MergeActions | null = null;
export const setMergeActions = (a: MergeActions | null) => (mergeActions = a);
export const activeMerge = () => mergeActions;

const errorText = (err: unknown) => String(err instanceof Error ? err.message : err);

/** Git ▸ Resolve Conflicts… (Git.ResolveConflicts): the Conflicts dialog. */
export function resolveConflicts(): void {
  if (!conflictedPaths().length) {
    notify("info", t("git.conflicts.noConflictsToResolve"));
    return;
  }
  hideBalloon();
  setConflictsOpen(true);
}

export function closeConflicts(): void {
  setConflictsOpen(false);
  // Closed with conflicts left: remind like PHPStorm.
  if (conflictedPaths().length) showConflictsBalloon();
}

/** After the last conflict is resolved: what finishes the operation. */
function finishHint(op: MergeOperation): string {
  switch (op) {
    case "merge":
      return "Commit the changes to conclude the merge.";
    case "rebase":
      return "Continue the rebase to apply the remaining commits.";
    case "cherryPick":
      return "Commit the changes to conclude the cherry-pick.";
    case "revert":
      return "Commit the changes to conclude the revert.";
    default:
      return "The stash was applied; drop it once you no longer need it.";
  }
}

async function afterResolve(paths: string[], op?: MergeOperation): Promise<void> {
  await repositoryChanged();
  await reloadOpenFiles(paths);
  if (!conflictedPaths().length) {
    setConflictsOpen(false);
    hideBalloon();
    notify("success", t("git.conflicts.allConflictsResolved"), op ? finishHint(op) : undefined);
  }
}

/** Accept Yours / Accept Theirs for whole files. */
export async function acceptSide(paths: string[], side: ConflictSide, op?: MergeOperation): Promise<boolean> {
  if (!paths.length) return false;
  try {
    await gitApi.acceptConflictSide(paths, side);
  } catch (err) {
    notify("error", t("git.conflicts.couldNotResolveTheConflict"), errorText(err), 10000);
    await repositoryChanged();
    return false;
  }
  await afterResolve(paths, op);
  return true;
}

/** Apply in the merge window: write the merged text and mark the file resolved. */
export async function applyMergeResult(file: MergeFile, text: string): Promise<boolean> {
  try {
    await gitApi.resolveMerge(file.path, text, file.crlf);
  } catch (err) {
    notify("error", t("git.conflicts.couldNotSaveTheMerge"), errorText(err), 10000);
    return false;
  }
  await afterResolve([file.path], (await gitApi.conflicts().catch(() => null))?.operation);
  return true;
}

const nameOf = (path: string) => path.slice(path.lastIndexOf("/") + 1);

/**
 * Merge… for one file: the merge window for text files, a choice for files deleted on
 * one side ("Keep File" / "Accept Deletion") and for binary files (one side or the other).
 */
export async function openMerge(path: string): Promise<void> {
  let file: MergeFile;
  try {
    file = await gitApi.mergeFile(path);
  } catch (err) {
    notify("error", t("git.conflicts.cannotMergeName", { name: nameOf(path) }), errorText(err), 10000);
    return;
  }
  const set = await gitApi.conflicts().catch(() => null);
  const yours = set ? `yours (${set.yoursName})` : "yours";
  const theirs = set ? `theirs (${set.theirsName})` : "theirs";
  const op = set?.operation;
  if (file.yours === null || file.theirs === null) {
    if (file.yours === null && file.theirs === null) {
      const choice = await askChoice({
        title: t("git.conflicts.mergeRevisions"),
        message: `${path} was deleted in both ${yours} and ${theirs}.`,
        choices: [{ id: "delete", label: t("git.conflicts.acceptDeletion") }],
      });
      if (choice) await acceptSide([path], "yours", op);
      return;
    }
    const deletedIn: ConflictSide = file.yours === null ? "yours" : "theirs";
    const message =
      deletedIn === "yours"
        ? `${path} was deleted in ${yours}, but modified in ${theirs}.`
        : `${path} was modified in ${yours}, but deleted in ${theirs}.`;
    const choice = await askChoice({
      title: t("git.conflicts.mergeRevisions"),
      message,
      choices: [
        { id: "delete", label: t("git.conflicts.acceptDeletion") },
        { id: "keep", label: t("git.conflicts.keepFile") },
      ],
    });
    if (choice === "delete") await acceptSide([path], deletedIn, op);
    else if (choice === "keep") await acceptSide([path], deletedIn === "yours" ? "theirs" : "yours", op);
    return;
  }
  if (file.binary || !file.model) {
    const choice = await askChoice({
      title: t("git.conflicts.mergeRevisions"),
      message: `${path} is a binary file and cannot be merged. Choose the version to keep.`,
      choices: [
        { id: "yours", label: t("git.conflicts.acceptYours") },
        { id: "theirs", label: t("git.conflicts.acceptTheirs") },
      ],
    });
    if (choice === "yours" || choice === "theirs") await acceptSide([path], choice, op);
    return;
  }
  setMergeFile(file);
}

// ---- operations that may stop with conflicts ----------------------------------------------

/**
 * Run a merge, rebase, pull, cherry-pick or unstash. When it stops with conflicts, open the
 * Conflicts dialog (as PHPStorm does) instead of an error balloon. `undefined` on failure.
 */
export async function runMerging<T>(title: string, op: () => Promise<T>): Promise<T | undefined> {
  try {
    return await op();
  } catch (err) {
    await repositoryChanged();
    if (conflictedPaths().length) {
      await reloadOpenFiles();
      hideBalloon();
      setConflictsOpen(true);
    } else {
      notify("error", title, errorText(err), 10000);
    }
    return undefined;
  } finally {
    await repositoryChanged();
  }
}

// ---- "Merge conflicts" balloon ----------------------------------------------------------------

let balloon: number | null = null;

function hideBalloon(): void {
  if (balloon !== null) dismissBalloon(balloon);
  balloon = null;
}

function showConflictsBalloon(): void {
  hideBalloon();
  const n = conflictedPaths().length;
  balloon = showBalloon(
    t("tools.git.mergeConflicts"),
    t("tools.git.fileUnresolvedConflicts", { n, value: n === 1 ? " has" : "s have" }),
    0,
    [{ label: t("git.conflicts.resolve"), run: resolveConflicts }],
    { group: VCS_GROUP, type: "warning" },
  );
}

/** Conflicts that appear while Forge runs (also from the terminal) get the balloon. */
export function watchConflicts(): void {
  createRoot(() =>
    createEffect(
      on(
        () => conflictedPaths().length,
        (n, prev) => {
          if (n === 0) hideBalloon();
          else if (!prev && !conflictsOpen() && !mergeFile()) showConflictsBalloon();
        },
      ),
    ),
  );
}
