/**
 * The Git Log context menu, like PHPStorm's: Copy Revision Number, Create Patch, Cherry-Pick,
 * Checkout Revision, Show Repository at Revision, Compare with Local, Reset Current Branch to
 * Here, Revert Commit, Undo Commit, Edit Commit Message, Fixup / Squash Into, Squash Commits,
 * Drop Commits, Interactively Rebase from Here, New Branch / Tag and Go to Child / Parent
 * Commit. Which actions a selection allows (and why not) comes from Rust
 * (`forge_vcs::rebase::log_action_states`); operations that stop with conflicts hand over to
 * the conflict resolver, `edit` steps to Continue Rebase.
 */
import { showBalloon } from "@/keymap";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { item, separator, showContextMenu, type ItemOptions, type MenuEntry } from "@/features/contextMenu/state";
import { openFile } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { gitApi, type Commit } from "./api";
import { logApi, type ActionState, type LogActionStates, type RewriteResult } from "./logApi";
import { notify, reloadOpenFiles, runGit, VCS_GROUP } from "./store";
import { createCommitsPatchDialog } from "./changesDialogs";
import { runMerging } from "./conflicts";
import { refreshOperation } from "./operation";
import { newBranchDialog } from "./actions";
import { checkoutRevision, copyRevisionNumber, shortSha } from "./history";
import { newTagDialog, resetDialog, setGitDialog } from "./gitDialogs";
import { setChooserDialog } from "./overlays";
import { revisionPath } from "./revisionFiles";
import { openBranchChanges } from "./BranchChanges";
import { setCommitMessage } from "./CommitPanel";
import { t } from "@/i18n";

/** What the Log tab offers to the actions: its commits (log order) and the selection. */
export interface LogView {
  commits: () => Commit[];
  /** Selected shas in log order (newest first). */
  selected: () => string[];
  select: (sha: string) => void;
}

let view: LogView | null = null;
export const setLogView = (v: LogView | null) => (view = v);

const plural = (n: number, what: string) => `${n} ${what}${n === 1 ? "" : "s"}`;
const errorText = (err: unknown) => String(err instanceof Error ? err.message : err);

function selection(): { shas: string[]; commits: Commit[] } {
  if (!view) return { shas: [], commits: [] };
  const shas = view.selected();
  const all = view.commits();
  return { shas, commits: shas.map((s) => all.find((c) => c.sha === s)).filter((c): c is Commit => !!c) };
}

const subjectOf = (sha: string) => view?.commits().find((c) => c.sha === sha)?.subject ?? shortSha(sha);

/** Ask before rewriting commits that are already pushed (not to a protected branch). */
async function confirmPushed(state: ActionState, title: string): Promise<boolean> {
  if (!state.warning) return true;
  return askConfirm({ title, message: t("git.logActions.warningContinue", { warning: state.warning }), confirm: t("git.logActions.continue") });
}

/** Finish a rewrite: reload buffers and offer Undo, or explain the stop at an `edit` step. */
async function afterRewrite(res: RewriteResult | undefined, title: string, body?: string): Promise<void> {
  await refreshOperation();
  if (!res) return;
  await reloadOpenFiles();
  if (res.stopped) {
    notify("info", t("git.logActions.rebaseStoppedForEditing"), t("tools.git.makeYourChangesAndAmend"), 10000);
    return;
  }
  const newHead = res.newHead;
  showBalloon(title, body, 8000, newHead ? [{ label: t("git.logActions.undo"), run: () => void undoRewrite(res.oldHead, newHead, title) }] : undefined, {
    group: VCS_GROUP,
  });
}

async function undoRewrite(oldHead: string, newHead: string, title: string): Promise<void> {
  const ok = await runGit("Undo failed", () => logApi.undoRewrite(oldHead, newHead).then(() => true));
  if (!ok) return;
  await reloadOpenFiles();
  notify("info", t("git.logActions.undoneTitle", { title }));
}

// ---- actions ---------------------------------------------------------------------------------

export async function cherryPick(shas: string[]): Promise<void> {
  const out = await runMerging("Cherry-pick failed", () => logApi.cherryPick(shas));
  await refreshOperation();
  if (out === undefined) return;
  await reloadOpenFiles();
  const skipped = out.split("\n").find((l) => l.includes("skipped"));
  notify(
    "success",
    t("tools.git.cherryPickSuccessful"),
    shas.length === 1 ? `${shortSha(shas[0])} ${subjectOf(shas[0])}${skipped ? ` (${skipped})` : ""}` : `${plural(shas.length, "commit")}${skipped ? `, ${skipped}` : ""}`,
  );
}

export async function revertCommits(shas: string[]): Promise<void> {
  const out = await runMerging("Revert failed", () => logApi.revert(shas));
  await refreshOperation();
  if (out === undefined) return;
  await reloadOpenFiles();
  notify("success", shas.length === 1 ? t("git.logActions.revertedSha", { sha: shortSha(shas[0]) }) : `Reverted ${plural(shas.length, "commit")}`, shas.length === 1 ? subjectOf(shas[0]) : undefined);
}

/** Undo Commit…: the changes go back to the Commit tool window, the message to its editor. */
export async function undoCommit(sha: string): Promise<void> {
  const ok = await askConfirm({
    title: t("git.logActions.undoCommit"),
    message: t("git.logActions.theChangesOfSubjectofWill", { subjectOf: subjectOf(sha) }),
    confirm: "OK",
  });
  if (!ok) return;
  const undone = await runGit("Undo Commit failed", () => logApi.undoCommit(sha));
  if (!undone) return;
  setCommitMessage(undone.message);
  showToolWindow("commit");
  notify("success", t("git.logActions.commitUndone"), `${shortSha(sha)} ${subjectOf(sha)}`);
}

/** Edit Commit Message…: the dialog starts with the full message. */
export async function editMessage(sha: string, state: ActionState): Promise<void> {
  if (!(await confirmPushed(state, "Edit Commit Message"))) return;
  const details = await runGit("Cannot read the commit", () => gitApi.commitDetails(sha));
  if (details) setGitDialog({ kind: "reword", sha, message: details.message });
}

export async function reword(sha: string, message: string): Promise<void> {
  const res = await runMerging("Edit Commit Message failed", () => logApi.reword(sha, message));
  await afterRewrite(res, "Commit message changed", message.split("\n")[0]);
}

/** Squash Commits…: the dialog starts with the messages, oldest first. */
export async function squashDialog(shas: string[], state: ActionState): Promise<void> {
  if (!(await confirmPushed(state, "Squash Commits"))) return;
  const details = await runGit("Cannot read the commits", () => Promise.all(shas.map((s) => gitApi.commitDetails(s))));
  if (!details) return;
  const message = details
    .reverse()
    .map((d) => d.message.trim())
    .join("\n\n");
  setGitDialog({ kind: "squashCommits", shas, message });
}

export async function squashCommits(shas: string[], message: string): Promise<void> {
  const res = await runMerging("Squash failed", () => logApi.squash(shas, message));
  await afterRewrite(res, `Squashed ${plural(shas.length, "commit")}`, message.split("\n")[0]);
}

export async function dropCommits(shas: string[], state: ActionState): Promise<void> {
  if (!(await confirmPushed(state, "Drop Commits"))) return;
  const res = await runMerging("Drop failed", () => logApi.drop(shas));
  await afterRewrite(res, `Dropped ${plural(shas.length, "commit")}`, shas.length === 1 ? subjectOf(shas[0]) : undefined);
}

/** Fixup… / Squash Into…: commit the local changes as `fixup! <subject>` / `squash! <subject>`. */
export function fixupInto(sha: string, kind: "fixup" | "squash"): void {
  setCommitMessage(`${kind}! ${subjectOf(sha)}`);
  showToolWindow("commit");
  requestAnimationFrame(() => document.querySelector<HTMLTextAreaElement>(".commit-message textarea")?.focus());
}

/** Interactively Rebase from Here…: the Rebasing Commits dialog. */
export async function interactiveRebase(sha: string, state: ActionState): Promise<void> {
  if (!(await confirmPushed(state, "Interactive Rebase"))) return;
  const plan = await runGit("Cannot start the rebase", () => logApi.rebasePlan(sha));
  if (plan) setGitDialog({ kind: "interactiveRebase", sha, plan });
}

export async function startRebase(sha: string, entries: import("./logApi").RebaseEntry[]): Promise<void> {
  const res = await runMerging("Rebase failed", () => logApi.interactiveRebase(sha, entries));
  await afterRewrite(res, "Rebase successful");
}

/** Create Patch…: the selected commits as one patch, to a file or the clipboard. */
export async function createPatch(shas: string[]): Promise<void> {
  const name = (subjectOf(shas[shas.length - 1]) || "changes").replace(/[^\w.-]+/g, "_").slice(0, 60);
  createCommitsPatchDialog(shas, name);
}

/** Show Repository at Revision: the files of the revision, opened read-only. */
export async function showRepositoryAt(sha: string): Promise<void> {
  const files = await runGit("Cannot list the files", () => logApi.treeFiles(sha));
  if (!files) return;
  setChooserDialog({
    title: t("git.logActions.repositoryAtSha", { sha: shortSha(sha) }),
    items: files.map((p) => ({ id: p, label: p.split("/").pop() || p, detail: p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "" })),
    empty: "No files",
    okLabel: t("git.logActions.open"),
    onOk: async (it) => {
      try {
        await openFile(revisionPath(sha, it.id));
      } catch (e) {
        notify("error", t("git.logActions.cannotOpenLabel", { label: it.label }), errorText(e));
      }
    },
  });
}

/** Go to Child Commit / Go to Parent Commit in the log. */
export function goToRelative(sha: string, parent: boolean): void {
  if (!view) return;
  const commits = view.commits();
  const target = parent ? commits.find((c) => c.sha === sha)?.parents[0] : commits.find((c) => c.parents.includes(sha))?.sha;
  if (target && commits.some((c) => c.sha === target)) view.select(target);
  else notify("info", parent ? t("git.logActions.theParentCommitIsNot") : t("git.logActions.theCommitHasNoChild"));
}

// ---- the menu and the keymap actions ----------------------------------------------------------

const off = (s: ActionState | undefined): ItemOptions => ({ disabled: !s?.enabled, hint: s?.enabled ? (s.warning ?? undefined) : (s?.reason ?? t("git.logActions.notAvailable")) });

/** The Log context menu for the selection (log order) and its action states. */
export function logMenuEntries(shas: string[], states: LogActionStates | null): MenuEntry[] {
  const one = shas.length === 1;
  const single: ItemOptions = one ? {} : { disabled: true, hint: t("git.logActions.selectASingleCommit") };
  const [sha] = shas;
  const s = states;
  return [
    item(t("git.logActions.copyRevisionNumber"), () => void copyRevisionNumber(shas.join(" ")), "Vcs.CopyRevisionNumberAction"),
    item(t("git.logActions.createPatch"), () => void createPatch(shas), "Vcs.Log.CreatePatch"),
    item(shas.length > 1 ? t("git.logActions.cherryPickCountCommits", { count: shas.length }) : "Cherry-Pick", () => void cherryPick(shas), "Vcs.CherryPick", off(s?.cherryPick)),
    separator,
    item(t("git.logActions.checkoutRevision"), () => void checkoutRevision(sha), "Git.CheckoutRevision", single),
    item(t("git.logActions.showRepositoryAtRevision"), () => void showRepositoryAt(sha), "Git.BrowseRepoAtRevision", single),
    item(t("git.logActions.compareWithLocal"), () => openBranchChanges(sha, "", shortSha(sha)), "Vcs.Log.CompareWithLocal", single),
    separator,
    item(t("git.logActions.resetCurrentBranchToHere"), () => resetDialog(sha), "Git.Reset.In.Log", single),
    item(t("git.logActions.revertCommit"), () => void revertCommits(shas), "Git.Revert.In.Log", off(s?.revert)),
    item(t("git.logActions.undoCommit2"), () => void undoCommit(sha), "Git.Uncommit", off(s?.undoCommit)),
    separator,
    item(t("git.logActions.editCommitMessage"), () => void editMessage(sha, s!.editMessage), "Git.Reword.Commit", off(s?.editMessage)),
    item(t("git.logActions.fixup"), () => fixupInto(sha, "fixup"), "Git.Fixup.To.Commit", off(s?.fixup)),
    item(t("git.logActions.squashInto"), () => fixupInto(sha, "squash"), "Git.Squash.Into.Commit", off(s?.squashInto)),
    ...(shas.length > 1 ? [item(t("git.logActions.squashCommits"), () => void squashDialog(shas, s!.squashCommits), "Git.Squash.Commits", off(s?.squashCommits))] : []),
    item(t("git.logActions.dropCommits"), () => void dropCommits(shas, s!.dropCommits), "Git.Drop.Commits", off(s?.dropCommits)),
    item(t("git.logActions.interactivelyRebaseFromHere"), () => void interactiveRebase(sha, s!.interactiveRebase), "Git.Interactive.Rebase", off(s?.interactiveRebase)),
    separator,
    item(t("git.logActions.newBranch"), () => newBranchDialog(sha), "Git.CreateNewBranch", single),
    item(t("git.logActions.newTag"), () => newTagDialog(sha), "Git.CreateNewTag", single),
    separator,
    item(t("git.logActions.goToChildCommit"), () => goToRelative(sha, false), "Vcs.Log.GoToChild", single),
    item(t("git.logActions.goToParentCommit"), () => goToRelative(sha, true), "Vcs.Log.GoToParent", single),
  ];
}

/** Right-click in the log: the menu for the current selection. */
export async function openLogMenu(x: number, y: number): Promise<void> {
  const { shas } = selection();
  if (!shas.length) return;
  const states = await logApi.actions(shas).catch(() => null);
  showContextMenu(logMenuEntries(shas, states), x, y);
}

/**
 * A Log action run from the keymap or Find Action: the same as the menu item, on the current
 * selection; a disabled action explains why.
 */
export async function runLogAction(key: keyof LogActionStates | null, run: (shas: string[], state: ActionState) => void | Promise<void>): Promise<void> {
  const { shas } = selection();
  if (!shas.length) return notify("info", t("git.logActions.selectACommitInThe"));
  let state: ActionState = { enabled: true, reason: null, warning: null };
  if (key) {
    const states = await logApi.actions(shas).catch((e) => {
      notify("error", "Git", errorText(e));
      return null;
    });
    if (!states) return;
    state = states[key];
  }
  if (!state.enabled) return notify("info", state.reason ?? t("tools.git.notAvailable"));
  await run(shas, state);
}
