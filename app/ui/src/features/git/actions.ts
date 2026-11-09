/** Git actions shared by the Commit/Git tool windows, branches popup and commands. */
import { editor } from "@/state/editor";
import { workspaceApi } from "@/ipc/workspace";
import { showToolWindow } from "@/state/layout";
import { gitApi, type StatusEntry } from "./api";
import { notify, reloadOpenFiles, repositoryChanged, runGit, status } from "./store";
import { setBranchesOpen, setDiffRequest, setInputDialog } from "./overlays";
import { runMerging } from "./conflicts";
import { withLocalHistory } from "@/ipc/localHistory";
import { t as tr } from "@/i18n";

const shortSha = (sha?: string | null) => (sha ? sha.slice(0, 8) : "HEAD");

async function workingContent(path: string): Promise<string> {
  const tab = editor.tabs.find((t) => t.path === path);
  if (tab) return tab.content;
  try {
    return await workspaceApi.readFile(path);
  } catch {
    return "";
  }
}

/** Show Diff (Ctrl+D): HEAD vs working copy, side by side. */
export async function showDiff(path: string, files?: string[]): Promise<void> {
  const entry: StatusEntry | undefined = status()?.entries.find((e) => e.path === path);
  const isNew = entry && (entry.worktree === "untracked" || entry.index === "added");
  const head = isNew ? "" : ((await gitApi.show("HEAD", path).catch(() => null)) ?? "");
  const deleted = entry && (entry.worktree === "deleted" || entry.index === "deleted");
  const right = deleted ? "" : await workingContent(path);
  setDiffRequest({
    path,
    left: { title: isNew ? tr("git.actions.emptyNewFile") : `${shortSha(status()?.head)} (Read-only)`, content: head },
    right: { title: deleted ? tr("git.actions.deleted") : tr("git.actions.yourVersion"), content: right },
    editable: !deleted,
    files,
    partial: !!entry && !isNew && !deleted && !entry.conflict,
  });
}

/**
 * A revision or branch (`rev`, shown as `label`) vs the working copy: Compare with Revision /
 * Branch. `revPath` is the file's path in `rev` when it was renamed since.
 */
export async function showRevisionDiff(path: string, rev: string, label: string, revPath = path): Promise<void> {
  const [before, after] = await Promise.all([gitApi.show(rev, revPath).catch(() => null), workingContent(path)]);
  setDiffRequest({
    path,
    left: { title: before === null ? `${label} (file does not exist)` : `${label} (Read-only)`, content: before ?? "" },
    right: { title: tr("git.actions.yourVersion"), content: after },
    editable: true,
  });
}

/** Diff of one file in a commit against its first parent (Git log details pane). */
export async function showCommitDiff(sha: string, parent: string | undefined, path: string): Promise<void> {
  const [before, after] = await Promise.all([
    parent ? gitApi.show(parent, path).catch(() => null) : Promise.resolve(null),
    gitApi.show(sha, path).catch(() => null),
  ]);
  setDiffRequest({
    path,
    left: { title: parent ? shortSha(parent) : "Empty", content: before ?? "" },
    right: { title: shortSha(sha), content: after ?? "" },
    editable: false,
  });
}

export function confirmRollback(paths: string[]): void {
  if (!paths.length) return;
  setInputDialog({
    title: tr("git.actions.rollbackChanges"),
    label:
      paths.length === 1
        ? tr("git.actions.rollbackChangesInPathsLocal", { paths: paths[0] })
        : tr("git.actions.rollbackChangesInCountFiles", { count: paths.length }),
    value: "",
    noInput: true,
    okLabel: tr("git.actions.rollback"),
    onOk: async () => {
      await runGit("Rollback failed", () => withLocalHistory("vcs", "Rollback", () => gitApi.rollback(paths), paths));
      await reloadOpenFiles(paths);
      notify("success", tr("git.actions.rollback"), tr("tools.git.fileRolledBack", { count: paths.length }));
    },
  });
}

export async function push(): Promise<void> {
  const branch = status()?.branch;
  const res = await runGit("Push failed", () => gitApi.push());
  if (res) notify("success", tr("git.actions.pushed"), `${branch ?? "HEAD"} to ${status()?.upstream ?? "origin"}`);
}

/** "Update Project" (Ctrl+T): pull with the update method of the Git settings (merge, rebase or the branch default). */
export async function updateProject(): Promise<void> {
  const res = await runMerging("Update failed", () => withLocalHistory("vcs", "Update from VCS", () => gitApi.pull()));
  if (res) {
    await reloadOpenFiles();
    notify("success", tr("git.actions.projectUpdated"), res.output.split("\n")[0] || tr("tools.git.allFilesAreUpTo"));
  }
}

export async function fetchAll(): Promise<void> {
  const res = await runGit("Fetch failed", () => gitApi.fetch());
  if (res) notify("info", tr("git.actions.fetchSuccessful"));
}

export async function checkout(name: string, remote = false): Promise<void> {
  setBranchesOpen(false);
  const ok = await runGit(tr("git.actions.checkoutOfNameFailed", { name }), () =>
    withLocalHistory("vcs", tr("git.actions.checkoutName", { name }), () => gitApi.checkout(name, remote).then(() => true)),
  );
  if (ok) {
    await reloadOpenFiles();
    notify("info", tr("tools.git.checkedOut", { value: remote ? name.split("/").slice(1).join("/") : name }));
  }
}

export function newBranchDialog(start: string | null = null): void {
  setBranchesOpen(false);
  setInputDialog({
    title: start ? `Create Branch from ${/^[0-9a-f]{40,64}$/.test(start) ? start.slice(0, 8) : start}` : tr("git.actions.createNewBranch"),
    label: tr("git.actions.newBranchName"),
    value: "",
    okLabel: tr("git.actions.create"),
    checkbox: { label: tr("git.actions.checkoutBranch"), checked: true },
    onOk: async (name, co) => {
      const n = name.trim();
      if (!n) return;
      const ok = await runGit("Branch creation failed", () => gitApi.createBranch(n, start, co).then(() => true));
      if (ok) notify("info", co ? tr("git.actions.checkedOutNewBranchN", { n }) : tr("git.actions.createdBranchN", { n }));
    },
  });
}

export function checkoutRevisionDialog(): void {
  setBranchesOpen(false);
  setInputDialog({
    title: tr("git.actions.checkoutTagOrRevision"),
    label: tr("git.actions.enterReferenceBranchTagName"),
    value: "",
    okLabel: tr("git.actions.checkout"),
    onOk: (rev) => (rev.trim() ? checkout(rev.trim()) : undefined),
  });
}

export async function mergeIntoCurrent(name: string): Promise<void> {
  setBranchesOpen(false);
  const out = await runMerging(tr("git.actions.mergeOfNameFailed", { name }), () => withLocalHistory("vcs", tr("git.actions.mergeName", { name }), () => gitApi.merge(name)));
  if (out !== undefined) {
    await reloadOpenFiles();
    notify("success", tr("tools.git.mergedInto", { name, value: status()?.branch ?? "HEAD" }), out.split("\n")[0]);
  }
}

export async function rebaseOnto(name: string): Promise<void> {
  setBranchesOpen(false);
  const out = await runMerging(tr("git.actions.rebaseOntoNameFailed", { name }), () => withLocalHistory("vcs", tr("git.actions.rebaseOntoName", { name }), () => gitApi.rebase(name)));
  if (out !== undefined) {
    await reloadOpenFiles();
    notify("success", tr("tools.git.rebasedOnto", { value: status()?.branch ?? "HEAD", name }));
  }
}

export function deleteBranch(name: string): void {
  setBranchesOpen(false);
  setInputDialog({
    title: tr("git.actions.deleteBranch"),
    label: tr("git.actions.deleteBranchName", { name }),
    value: "",
    noInput: true,
    okLabel: tr("git.actions.delete"),
    checkbox: { label: tr("git.actions.forceDeleteBranchMayNot"), checked: false },
    onOk: async (_v, force) => {
      const ok = await runGit(tr("git.actions.couldNotDeleteName", { name }), () => gitApi.deleteBranch(name, force).then(() => true));
      if (ok) notify("info", tr("git.actions.deletedBranchName", { name }));
    },
  });
}

export function openCommitWindow(): void {
  showToolWindow("commit");
  void repositoryChanged();
  // Focus the message editor once the panel is rendered.
  requestAnimationFrame(() => document.querySelector<HTMLTextAreaElement>(".commit-message textarea")?.focus());
}
