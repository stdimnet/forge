/** Git actions shared by the Commit/Git tool windows, branches popup and commands. */
import { editor } from "@/state/editor";
import { workspaceApi } from "@/ipc/workspace";
import { showToolWindow } from "@/state/layout";
import { gitApi, type StatusEntry } from "./api";
import { notify, reloadOpenFiles, repositoryChanged, runGit, status } from "./store";
import { setBranchesOpen, setDiffRequest, setInputDialog } from "./overlays";

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
    left: { title: isNew ? "Empty (new file)" : `${shortSha(status()?.head)} (Read-only)`, content: head },
    right: { title: deleted ? "Deleted" : "Your version", content: right },
    editable: !deleted,
    files,
  });
}

/** A revision or branch (`rev`, shown as `label`) vs the working copy: Compare with Revision / Branch. */
export async function showRevisionDiff(path: string, rev: string, label: string): Promise<void> {
  const [before, after] = await Promise.all([gitApi.show(rev, path).catch(() => null), workingContent(path)]);
  setDiffRequest({
    path,
    left: { title: before === null ? `${label} (file does not exist)` : `${label} (Read-only)`, content: before ?? "" },
    right: { title: "Your version", content: after },
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
    title: "Rollback Changes",
    label:
      paths.length === 1
        ? `Rollback changes in ${paths[0]}? Local modifications will be lost.`
        : `Rollback changes in ${paths.length} files? Local modifications will be lost.`,
    value: "",
    noInput: true,
    okLabel: "Rollback",
    onOk: async () => {
      await runGit("Rollback failed", () => gitApi.rollback(paths));
      await reloadOpenFiles(paths);
      notify("success", "Rollback", `${paths.length} file${paths.length > 1 ? "s" : ""} rolled back`);
    },
  });
}

export async function commitChanges(opts: {
  message: string;
  amend: boolean;
  paths: string[];
  push: boolean;
}): Promise<boolean> {
  const sha = await runGit("Commit failed", () => gitApi.commit(opts.message, opts.amend, opts.paths));
  if (!sha) return false;
  const n = opts.paths.length;
  notify("success", `${n} file${n === 1 ? "" : "s"} committed: ${opts.message.split("\n")[0]}`);
  if (opts.push) await push();
  return true;
}

export async function push(): Promise<void> {
  const branch = status()?.branch;
  const res = await runGit("Push failed", () => gitApi.push());
  if (res) notify("success", "Pushed", `${branch ?? "HEAD"} to ${status()?.upstream ?? "origin"}`);
}

/** "Update Project" (Ctrl+T): pull with the update method of the Git settings (merge, rebase or the branch default). */
export async function updateProject(): Promise<void> {
  const res = await runGit("Update failed", () => gitApi.pull());
  if (res) {
    await reloadOpenFiles();
    notify("success", "Project updated", res.output.split("\n")[0] || "All files are up to date");
  }
}

export async function fetchAll(): Promise<void> {
  const res = await runGit("Fetch failed", () => gitApi.fetch());
  if (res) notify("info", "Fetch successful");
}

export async function checkout(name: string, remote = false): Promise<void> {
  setBranchesOpen(false);
  const ok = await runGit(`Checkout of ${name} failed`, () => gitApi.checkout(name, remote).then(() => true));
  if (ok) {
    await reloadOpenFiles();
    notify("info", `Checked out ${remote ? name.split("/").slice(1).join("/") : name}`);
  }
}

export function newBranchDialog(start: string | null = null): void {
  setBranchesOpen(false);
  setInputDialog({
    title: start ? `Create Branch from ${start}` : "Create New Branch",
    label: "New branch name:",
    value: "",
    okLabel: "Create",
    checkbox: { label: "Checkout branch", checked: true },
    onOk: async (name, co) => {
      const n = name.trim();
      if (!n) return;
      const ok = await runGit("Branch creation failed", () => gitApi.createBranch(n, start, co).then(() => true));
      if (ok) notify("info", co ? `Checked out new branch ${n}` : `Created branch ${n}`);
    },
  });
}

export function checkoutRevisionDialog(): void {
  setBranchesOpen(false);
  setInputDialog({
    title: "Checkout Tag or Revision",
    label: "Enter reference (branch, tag) name or commit hash:",
    value: "",
    okLabel: "Checkout",
    onOk: (rev) => (rev.trim() ? checkout(rev.trim()) : undefined),
  });
}

export async function mergeIntoCurrent(name: string): Promise<void> {
  setBranchesOpen(false);
  const out = await runGit(`Merge of ${name} failed`, () => gitApi.merge(name));
  if (out !== undefined) {
    await reloadOpenFiles();
    notify("success", `Merged ${name} into ${status()?.branch ?? "HEAD"}`, out.split("\n")[0]);
  }
}

export async function rebaseOnto(name: string): Promise<void> {
  setBranchesOpen(false);
  const out = await runGit(`Rebase onto ${name} failed`, () => gitApi.rebase(name));
  if (out !== undefined) {
    await reloadOpenFiles();
    notify("success", `Rebased ${status()?.branch ?? "HEAD"} onto ${name}`);
  }
}

export function deleteBranch(name: string): void {
  setBranchesOpen(false);
  setInputDialog({
    title: "Delete Branch",
    label: `Delete branch ${name}?`,
    value: "",
    noInput: true,
    okLabel: "Delete",
    checkbox: { label: "Force delete (branch may not be fully merged)", checked: false },
    onOk: async (_v, force) => {
      const ok = await runGit(`Could not delete ${name}`, () => gitApi.deleteBranch(name, force).then(() => true));
      if (ok) notify("info", `Deleted branch ${name}`);
    },
  });
}

export function openCommitWindow(): void {
  showToolWindow("left", "commit");
  void repositoryChanged();
  // Focus the message editor once the panel is rendered.
  requestAnimationFrame(() => document.querySelector<HTMLTextAreaElement>(".commit-message textarea")?.focus());
}
