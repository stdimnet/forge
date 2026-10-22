/**
 * Git actions on files and folders (Project view Git submenu, Git > Current File) and the
 * repository dialogs of that menu: New Tag, Stash, Unstash and Manage Remotes. File actions
 * act on `vcsTargets()`, so the same command serves menus and shortcuts.
 */
import { openFile } from "@/state/editor";
import { gitApi } from "./api";
import { checkOnly } from "./CommitPanel";
import { diffSiblings, targetState, vcsTargets, type VcsTarget } from "./context";
import { historyApi } from "./historyApi";
import { openFileHistory, openSelectionHistory } from "./history";
import { openBranchChanges } from "./BranchChanges";
import { activeView } from "@/features/navigation/editorCommands";
import { activeTab } from "@/state/editor";
import { logDate, timeAgo } from "./format";
import { notify, reloadOpenFiles, runGit, status } from "./store";
import { setChooserDialog, setInputDialog, toggleAnnotate } from "./overlays";
import { confirmRollback, openCommitWindow, showDiff, showRevisionDiff } from "./actions";

const stateOf = (targets: VcsTarget[]) => targetState(targets, status()?.entries ?? []);
const plural = (n: number, what: string) => `${n} ${what}${n === 1 ? "" : "s"}`;

/** Commit File… / Commit Directory… (CheckinFiles): the Commit window with just these changes. */
export function commitTargets(): void {
  const targets = vcsTargets();
  const s = stateOf(targets);
  openCommitWindow();
  if (targets.length) checkOnly([...s.changed, ...s.unversioned]);
}

/** Add (Git.Add, Ctrl+Alt+A): stage the unversioned files below the targets. */
export async function addTargets(): Promise<void> {
  const { unversioned } = stateOf(vcsTargets());
  if (!unversioned.length) return notify("info", "No unversioned files to add");
  await runGit("Adding files to Git failed", () => gitApi.stage(unversioned));
}

/** Add to .gitignore (Git.Ignore.File): unversioned files and folders only. */
export async function ignoreTargets(): Promise<void> {
  const targets = vcsTargets();
  if (!stateOf(targets).allUnversioned) return notify("info", "Only unversioned files can be added to .gitignore");
  const patterns = await runGit("Adding to .gitignore failed", () => gitApi.addToGitignore(targets.map((t) => t.path)));
  if (patterns) notify("success", "Added to .gitignore", patterns.join("\n"));
}

/** Annotate with Git Blame: one tracked file, opened in the editor. */
export async function annotateTarget(): Promise<void> {
  const { file, fileTracked } = stateOf(vcsTargets());
  if (!file || !fileTracked) return;
  await openFile(file);
  toggleAnnotate(file);
}

/** Show Diff (Compare.SameVersion, Ctrl+D): HEAD vs working copy of the changed files. */
export async function showTargetsDiff(): Promise<void> {
  const s = stateOf(vcsTargets());
  const files = [...s.changed, ...s.unversioned];
  if (!files.length) return notify("info", "No changes to show");
  await showDiff(files[0], diffSiblings(files));
}

/** Compare with Revision… (Compare.Selected): pick a commit that touched the file (renames followed). */
export async function compareWithRevision(): Promise<void> {
  const { file, fileTracked } = stateOf(vcsTargets());
  if (!file || !fileTracked) return;
  const revs = (await runGit("Could not load the file history", () => historyApi.fileHistory(file, false, { limit: 200 }))) ?? [];
  const pathAt = new Map(revs.map((r) => [r.sha, r.files[0]?.path ?? file]));
  setChooserDialog({
    title: `Compare ${file.split("/").pop()} with Revision`,
    empty: "No revisions of this file",
    okLabel: "Compare",
    items: revs.map((c) => ({ id: c.sha, label: c.subject, detail: `${c.sha.slice(0, 8)} · ${c.author} · ${logDate(c.time)}` })),
    onOk: (item) => showRevisionDiff(file, item.id, item.id.slice(0, 8), pathAt.get(item.id)),
  });
}

/**
 * Compare with Branch… (Git.CompareWithBranch): a file on another branch vs the working copy,
 * or for a folder the files that differ between the branch and the working tree.
 */
export async function compareWithBranch(): Promise<void> {
  const [target] = vcsTargets();
  const { file, fileTracked, ignored } = stateOf(vcsTargets());
  const folder = target?.dir && !ignored ? target.path : null;
  if (folder === null && (!file || !fileTracked)) return;
  const name = folder === null ? file!.split("/").pop() : folder.split("/").pop() || "Project";
  const branches = await runGit("Could not load branches", () => gitApi.branches());
  if (!branches) return;
  const items = [
    ...branches.local.filter((b) => !b.current).map((b) => ({ id: b.name, label: b.name, detail: "Local" })),
    ...branches.remote.map((b) => ({ id: b.name, label: b.name, detail: "Remote" })),
    ...branches.tags.map((b) => ({ id: b.name, label: b.name, detail: "Tag" })),
  ];
  setChooserDialog({
    title: `Compare ${name} with Branch`,
    empty: "No other branches",
    okLabel: "Compare",
    items,
    onOk: (item) => (folder === null ? showRevisionDiff(file!, item.id, item.label) : openBranchChanges(item.id, folder)),
  });
}

/** Show History (Vcs.ShowTabbedFileHistory): a "History: <name>" tab in the Git tool window. */
export function showTargetHistory(): void {
  const [target] = vcsTargets();
  if (!target || target.ignored) return;
  const { fileTracked } = stateOf([target]);
  if (!target.dir && !fileTracked) return notify("info", "The file is not under version control");
  openFileHistory(target.path, target.dir);
}

/** Show History for Selection (Vcs.ShowHistoryForBlock): the history of the selected lines. */
export function showSelectionHistory(): void {
  const tab = activeTab();
  const view = activeView();
  if (!tab || !view || tab.view || tab.path.includes("://")) return;
  if (!stateOf([{ path: tab.path, dir: false }]).fileTracked) return notify("info", "The file is not under version control");
  const sel = view.state.selection.main;
  const first = view.state.doc.lineAt(sel.from).number;
  // A selection ending at the start of a line does not include that line.
  const lastLine = view.state.doc.lineAt(sel.to);
  const last = sel.to > sel.from && lastLine.from === sel.to ? Math.max(first, lastLine.number - 1) : lastLine.number;
  openSelectionHistory(tab.path, first, last);
}

/** Rollback… (ChangesView.Rollback, Ctrl+Alt+Z): discard the tracked changes below the targets. */
export function rollbackTargets(): void {
  const { changed } = stateOf(vcsTargets());
  if (!changed.length) return notify("info", "No changes to roll back");
  confirmRollback(changed);
}

// ---- repository dialogs -----------------------------------------------------------------

/** New Tag… (Git.Tag): a lightweight tag on HEAD. */
export function newTagDialog(): void {
  setInputDialog({
    title: "Create New Tag on HEAD",
    label: "Enter the name of new tag:",
    value: "",
    okLabel: "Create Tag",
    onOk: async (name) => {
      const n = name.trim();
      const ok = await runGit("Tag creation failed", () => gitApi.createTag(n).then(() => true));
      if (ok) notify("info", `Created tag ${n}`);
    },
  });
}

/** Stash Changes… (Git.Stash). */
export function stashDialog(): void {
  const tracked = (status()?.entries ?? []).filter((e) => e.worktree !== "untracked");
  if (!tracked.length) return notify("info", "No local changes to stash");
  setInputDialog({
    title: "Stash",
    label: `Stash ${plural(tracked.length, "changed file")} on ${status()?.branch ?? "HEAD"}. Message:`,
    value: "",
    optional: true,
    okLabel: "Create Stash",
    checkbox: { label: "Keep index", checked: false },
    onOk: async (message, keepIndex) => {
      const out = await runGit("Stash failed", () => gitApi.stash(message.trim(), keepIndex));
      if (out === undefined) return;
      await reloadOpenFiles();
      notify("success", "Changes stashed", out.split("\n")[0]);
    },
  });
}

/** Unstash Changes… (Git.Unstash): apply or pop a stash, or drop one. */
export async function unstashDialog(): Promise<void> {
  const stashes = await runGit("Could not list stashes", () => gitApi.stashList());
  if (!stashes) return;
  setChooserDialog({
    title: "Unstash Changes",
    empty: "No stashes",
    okLabel: "Apply Stash",
    items: stashes.map((s) => ({
      id: String(s.index),
      label: s.message,
      detail: `stash@{${s.index}}${s.branch ? ` · On ${s.branch}` : ""} · ${timeAgo(s.time)}`,
    })),
    checkboxes: [
      { label: "Pop stash", checked: false },
      { label: "Reinstate index", checked: false },
    ],
    extra: [
      {
        label: "Drop",
        needsSelection: true,
        run: async (item) => {
          if (!item) return;
          setChooserDialog(null);
          await runGit("Could not drop the stash", () => gitApi.dropStash(Number(item.id)));
          await unstashDialog();
        },
      },
    ],
    onOk: async (item, [pop, reinstate]) => {
      const out = await runGit("Unstash failed", () => gitApi.unstash(Number(item.id), pop, reinstate));
      if (out === undefined) return;
      await reloadOpenFiles();
      notify("success", pop ? "Stash popped" : "Stash applied", item.label);
    },
  });
}

function addRemoteDialog(suggested: string): void {
  setInputDialog({
    title: "Define Remote",
    label: "Name:",
    value: suggested,
    okLabel: "Next",
    onOk: (name) => {
      setInputDialog({
        title: `Define Remote ${name.trim()}`,
        label: "URL:",
        value: "",
        okLabel: "OK",
        onOk: async (url) => {
          await runGit("Could not add the remote", () => gitApi.addRemote(name.trim(), url.trim()));
          await manageRemotes();
        },
      });
    },
  });
}

/** Manage Remotes… (Git.Configure.Remotes): list, add and remove remotes. */
export async function manageRemotes(): Promise<void> {
  const remotes = await runGit("Could not list remotes", () => gitApi.remotes());
  if (!remotes) return;
  setChooserDialog({
    title: "Git Remotes",
    empty: "No remotes defined",
    okLabel: "Close",
    items: remotes.map((r) => ({ id: r.name, label: r.name, detail: r.pushUrl ? `${r.fetchUrl} (push: ${r.pushUrl})` : r.fetchUrl })),
    extra: [
      {
        label: "Add…",
        run: () => {
          setChooserDialog(null);
          addRemoteDialog(remotes.some((r) => r.name === "origin") ? "" : "origin");
        },
      },
      {
        label: "Remove",
        needsSelection: true,
        run: (item) => {
          if (!item) return;
          setChooserDialog(null);
          setInputDialog({
            title: "Remove Remote",
            label: `Remove remote ${item.id}?`,
            value: "",
            noInput: true,
            okLabel: "Remove",
            onOk: async () => {
              await runGit("Could not remove the remote", () => gitApi.removeRemote(item.id));
              await manageRemotes();
            },
          });
        },
      },
    ],
  });
}
