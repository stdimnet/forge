/**
 * The Git menu dialogs, like PHPStorm: Stash, Unstash, Pull, Merge, Rebase, Reset HEAD, New
 * Tag, Manage Remotes, Shelve Changes and Unshelve. This module holds the open dialog and the
 * actions behind the buttons; `GitDialogs.tsx` renders them. Everything git does happens in
 * Rust (`forge_vcs::ops`, `forge_vcs::shelf`); operations that stop with conflicts hand over to
 * the conflict resolver through `runMerging`.
 */
import { createSignal } from "solid-js";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { gitApi, type StashEntry } from "./api";
import { opsApi, type MergeOptions, type PullOptions, type RebaseOptions, type ResetMode, type ShelvedChangelist, type TagOptions } from "./opsApi";
import type { RebasePlan } from "./logApi";
import type { ChangelistView, PatchPreviewFile } from "./changesApi";
import { intoNewChangelist } from "./changesDialogs";
import { notify, reloadOpenFiles, runGit, status } from "./store";
import { runMerging } from "./conflicts";
import { setDiffRequest } from "./overlays";
import { refreshOperation } from "./operation";
import type { CloneRequest } from "./extras";
import { withLocalHistory } from "@/ipc/localHistory";

export type GitDialog =
  | { kind: "stash" }
  | { kind: "unstash"; stashes: StashEntry[] }
  | { kind: "pull" }
  | { kind: "merge" }
  | { kind: "rebase" }
  | { kind: "reset"; rev: string }
  | { kind: "tag"; commit: string }
  | { kind: "remotes" }
  | { kind: "shelve"; paths: string[] }
  | { kind: "unshelve"; changelist: ShelvedChangelist; paths: string[] }
  | { kind: "reword"; sha: string; message: string }
  | { kind: "squashCommits"; shas: string[]; message: string }
  | { kind: "interactiveRebase"; sha: string; plan: RebasePlan }
  | { kind: "changelist"; edit: ChangelistView | null; movePaths: string[] }
  | { kind: "moveChangelist"; paths: string[]; hunks: string[] | null; from: string | null }
  | { kind: "createPatch"; paths: string[]; shas: string[]; name: string }
  | { kind: "applyPatch"; patch: string; name: string; files: PatchPreviewFile[] }
  | { kind: "clone"; request: CloneRequest }
  | { kind: "newWorktree"; branch: string | null };

const [gitDialog, setGitDialog] = createSignal<GitDialog | null>(null);
export { gitDialog, setGitDialog };
export const closeGitDialog = () => setGitDialog(null);

/** "Git root" of the dialogs: the repository folder. */
const [gitRoot, setGitRoot] = createSignal("");
export { gitRoot };
async function loadRoot(): Promise<void> {
  const info = await gitApi.info().catch(() => null);
  setGitRoot(info?.root ?? "");
}

const currentBranch = () => status()?.branch ?? "HEAD";
const plural = (n: number, what: string) => `${n} ${what}${n === 1 ? "" : "s"}`;

function open(d: GitDialog): void {
  void loadRoot();
  setGitDialog(d);
}

/** Finish a git operation that may have changed files. */
async function done(title: string, body?: string): Promise<void> {
  await reloadOpenFiles();
  await refreshOperation();
  notify("success", title, body);
}

// ---- stash ---------------------------------------------------------------------------------

/** Stash Changes… (Git.Stash). */
export function stashDialog(): void {
  if (!(status()?.entries ?? []).some((e) => e.worktree !== "untracked")) return notify("info", "No local changes to stash");
  open({ kind: "stash" });
}

export async function createStash(message: string, keepIndex: boolean): Promise<void> {
  const out = await runGit("Stash failed", () => withLocalHistory("vcs", "Stash changes", () => gitApi.stash(message.trim(), keepIndex)));
  if (out !== undefined) await done("Changes stashed", out.split("\n")[0]);
}

/** Unstash Changes… (Git.Unstash). */
export async function unstashDialog(): Promise<void> {
  const stashes = await runGit("Could not list stashes", () => gitApi.stashList());
  if (stashes) open({ kind: "unstash", stashes });
}

export async function reloadStashes(): Promise<StashEntry[]> {
  return (await gitApi.stashList().catch(() => [])) ?? [];
}

/** Apply (or pop) a stash; `branch` unstashes onto a new branch instead. */
export async function applyStash(entry: StashEntry, opts: { pop: boolean; reinstateIndex: boolean; branch?: string }): Promise<boolean> {
  const branch = opts.branch?.trim();
  const out = branch
    ? await runMerging("Unstash failed", () => withLocalHistory("vcs", "Unstash changes", () => opsApi.stashBranch(entry.index, branch)))
    : await runMerging("Unstash failed", () =>
        withLocalHistory("vcs", "Unstash changes", () => gitApi.unstash(entry.index, opts.pop, opts.reinstateIndex)),
      );
  if (out === undefined) return false;
  await done(branch ? `Unstashed to branch ${branch}` : opts.pop ? "Stash popped" : "Stash applied", entry.message);
  return true;
}

export async function dropStash(entry: StashEntry): Promise<boolean> {
  const ok = await askConfirm({ title: "Drop Stash", message: `Do you want to remove stash@{${entry.index}}: ${entry.message}?`, confirm: "Drop" });
  if (!ok) return false;
  return (await runGit("Could not drop the stash", () => gitApi.dropStash(entry.index).then(() => true))) ?? false;
}

export async function clearStashes(): Promise<boolean> {
  const ok = await askConfirm({ title: "Clear Stashes", message: "Do you want to remove all stashes? This cannot be undone.", confirm: "Clear" });
  if (!ok) return false;
  return (await runGit("Could not clear the stashes", () => opsApi.stashClear().then(() => true))) ?? false;
}

const nameOf = (p: string) => p.slice(p.lastIndexOf("/") + 1);

/** Diff of one file of a stash: its base commit vs the stashed version. */
export async function showStashDiff(entry: StashEntry, path: string): Promise<void> {
  const c = await runGit(`Cannot show ${nameOf(path)}`, () => opsApi.stashFile(entry.index, path));
  if (!c) return;
  setDiffRequest({
    path,
    left: { title: c.before === null ? "Empty (new file)" : "Base version (Read-only)", content: c.before ?? "" },
    right: { title: c.after === null ? "Deleted" : `stash@{${entry.index}} (Read-only)`, content: c.after ?? "" },
    editable: false,
  });
}

// ---- pull, merge, rebase ----------------------------------------------------------------------

/** Pull… (Git.Pull). */
export function pullDialog(): void {
  open({ kind: "pull" });
}

export async function pull(options: PullOptions): Promise<void> {
  const res = await runMerging("Pull failed", () => withLocalHistory("vcs", "Update from VCS", () => opsApi.pull(options)));
  if (res) await done("Pulled", res.output.split("\n")[0] || "All files are up to date");
}

/** Merge… (Git.Merge). */
export function mergeDialog(): void {
  open({ kind: "merge" });
}

export async function merge(options: MergeOptions): Promise<void> {
  const out = await runMerging(`Merge of ${options.branches.join(", ")} failed`, () =>
    withLocalHistory("vcs", `Merge ${options.branches.join(", ")}`, () => opsApi.merge(options)),
  );
  if (out !== undefined) await done(`Merged ${options.branches.join(", ")} into ${currentBranch()}`, out.split("\n")[0]);
}

/** Rebase… (Git.Rebase). */
export function rebaseDialog(): void {
  open({ kind: "rebase" });
}

export async function rebase(options: RebaseOptions): Promise<void> {
  const branch = options.branch || currentBranch();
  const out = await runMerging(`Rebase of ${branch} failed`, () => withLocalHistory("vcs", `Rebase ${branch}`, () => opsApi.rebase(options)));
  await refreshOperation();
  if (out !== undefined) await done(`Rebased ${branch} onto ${options.onto || "root"}`);
}

// ---- reset and tag ---------------------------------------------------------------------------

/** Reset HEAD… (Git.Reset). */
export function resetDialog(rev = "HEAD"): void {
  open({ kind: "reset", rev });
}

export async function resetHead(rev: string, mode: ResetMode): Promise<void> {
  const ok = await runGit("Reset failed", () => withLocalHistory("vcs", `Reset to ${rev || "HEAD"}`, () => opsApi.reset(rev, mode).then(() => true)));
  if (ok) await done("Reset successful", `${currentBranch()} was reset to ${rev || "HEAD"} (${mode})`);
}

/** New Tag… (Git.Tag / Git.CreateNewTag). */
export function newTagDialog(commit = ""): void {
  open({ kind: "tag", commit });
}

export async function createTag(options: TagOptions): Promise<boolean> {
  const ok = await runGit("Tag creation failed", () => opsApi.tag(options).then(() => true));
  if (ok) notify("info", `Created tag ${options.name.trim()}`);
  return !!ok;
}

// ---- remotes ---------------------------------------------------------------------------------

/** Manage Remotes… (Git.Configure.Remotes). */
export function manageRemotes(): void {
  open({ kind: "remotes" });
}

// ---- shelf -----------------------------------------------------------------------------------

/** Shelve Changes… (ChangesView.Shelve): every change, or the given files preselected. */
export function shelveDialog(paths?: string[]): void {
  const changed = (status()?.entries ?? []).filter((e) => !e.conflict && e.worktree !== "ignored").map((e) => e.path);
  if (!changed.length) return notify("info", "No changes to shelve");
  const selected = paths?.filter((p) => changed.includes(p)) ?? [];
  open({ kind: "shelve", paths: selected.length ? selected : changed });
}

export async function shelve(paths: string[], name: string): Promise<boolean> {
  const cl = await runGit("Shelve failed", () => withLocalHistory("vcs", "Shelve changes", () => opsApi.shelve(paths, name), paths));
  if (!cl) return false;
  await reloadOpenFiles();
  notify("success", "Changes shelved", `${plural(cl.files.length, "file")} in "${cl.name}"`);
  return true;
}

/** Unshelve… (ShelveChanges.UnshelveWithDialog): `paths` preselects files of the changelist. */
export function unshelveDialog(changelist: ShelvedChangelist, paths?: string[]): void {
  open({ kind: "unshelve", changelist, paths: paths?.length ? paths : changelist.files.map((f) => f.path) });
}

export async function unshelve(cl: ShelvedChangelist, paths: string[] | null, remove: boolean): Promise<boolean> {
  const touched = paths ?? cl.files.map((f) => f.path);
  const out = await runMerging("Unshelve failed", () => withLocalHistory("vcs", `Unshelve ${cl.name}`, () => opsApi.unshelve(cl.id, paths, remove), touched));
  if (out === undefined) return false;
  await done("Changes unshelved", `${out} from "${cl.name}"`);
  // "Create changelists automatically": the changes come back in a list named after the shelf.
  await intoNewChangelist(cl.name, paths ?? cl.files.map((f) => f.path));
  return true;
}

/** Show Diff of a shelved file: the base version vs the shelved one. */
export async function showShelfDiff(cl: ShelvedChangelist, path: string): Promise<void> {
  const c = await runGit(`Cannot show ${nameOf(path)}`, () => opsApi.shelfFile(cl.id, path));
  if (!c) return;
  if (c.binary) return notify("info", `${nameOf(path)} is a binary file`);
  setDiffRequest({
    path,
    left: { title: c.before === null ? "Empty (new file)" : "Base version (Read-only)", content: c.before ?? "" },
    right: { title: c.after === null ? "Deleted" : `Shelved: ${cl.name} (Read-only)`, content: c.after ?? "" },
    editable: false,
  });
}
