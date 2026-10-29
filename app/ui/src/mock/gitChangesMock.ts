/**
 * Browser-mode changelists, partial commits and patches (`git_changes.rs`). The models are the
 * Rust ones through WebAssembly (`forge_vcs::{changelists, hunks, patch}`); this mock only
 * keeps the state of the demo repository (`gitMock.ts`) and the in-memory workspace.
 */
import { recordMockCommit } from "./gitExtrasMock";
import { invoke, registerMock } from "@/ipc";
import { callWasm, forgeWasm } from "@/wasm";
import type { ChangedFile } from "@/features/git/api";
import type { ApplyPatchResult, ChangeHunk, ChangelistView, CommitFileSpec, CommitOptions, PatchPreviewFile } from "@/features/git/changesApi";
import { unifiedDiff } from "@/features/git/textDiff";
import { mockRepo, mockRepoOps } from "./gitMock";

const { head } = mockRepo;
const { index, changed, readWorking, working, newCommit, merging } = mockRepoOps;

/** The stored changelists (`changelists.json`): a second list for the demo. */
let stored: string | null = JSON.stringify({
  lists: [
    { id: "default", name: "Changes", comment: "" },
    { id: "cl1", name: "Checkout refactoring", comment: "Authorize order access in OrderController" },
  ],
  active: "default",
  files: { "app/Http/Controllers/OrderController.php": "cl1" },
  hunks: {},
});

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function hunksOf(path: string, content?: string | null): Promise<ChangeHunk[]> {
  const current = content ?? (await readWorking(path)) ?? "";
  return callWasm<ChangeHunk[]>((await forgeWasm()).vcs_change_hunks, { base: head[path] ?? "", current });
}

/** Tracked changed files, like `git_changelists` takes them from `git status`. */
async function changedPaths(): Promise<string[]> {
  const st = await mockRepo.computeStatus();
  return st.entries.filter((e) => !e.conflict && e.worktree !== "untracked" && e.worktree !== "ignored").map((e) => e.path);
}

interface Answer {
  state: string;
  view: ChangelistView[];
  error: string | null;
}

async function changelists(op: Record<string, unknown>): Promise<ChangelistView[]> {
  const paths = await changedPaths();
  const hunks: Record<string, string[]> = {};
  for (const p of paths) hunks[p] = (await hunksOf(p)).map((h) => h.id);
  const res = await callWasm<Answer>((await forgeWasm()).vcs_changelists, { state: stored, changed: paths, hunks, op });
  if (res.error) throw new Error(res.error);
  stored = res.state;
  return res.view;
}

const mutate = async (op: Record<string, unknown>) => {
  const view = await changelists(op);
  changed();
  return view;
};

registerMock("git_changelists", () => changelists({ kind: "view" }));
registerMock("git_changelist_create", ({ name, comment, active }) => mutate({ kind: "create", name, comment, active }));
registerMock("git_changelist_edit", ({ id, name, comment, active }) => mutate({ kind: "edit", id, name, comment, active }));
registerMock("git_changelist_set_active", ({ id }) => mutate({ kind: "setActive", id }));
registerMock("git_changelist_delete", ({ id }) => mutate({ kind: "delete", id }));
registerMock("git_changelist_move", ({ paths, hunks, target, create }) =>
  mutate({ kind: "move", paths, hunks: hunks ?? null, target, create: !!create }),
);
registerMock("git_file_hunks", ({ path, content }) => hunksOf(String(path), content as string | null));

registerMock("git_commit_files", async ({ message, files, options }) => {
  await delay(250);
  const msg = String(message ?? "").trim();
  const opts = options as CommitOptions;
  const specs = (files as CommitFileSpec[]) ?? [];
  if (!msg && !opts.amend) throw new Error("git commit: commit message is empty");
  if (merging.conflicts.size) throw new Error("git commit: Committing is not possible because you have unmerged files.");
  if (opts.author && !/<.+>$/.test(opts.author)) throw new Error(`the author must look like "Name <email>", not "${opts.author}"`);
  const committed: ChangedFile[] = [];
  for (const f of specs) {
    const w = await readWorking(f.path);
    const status = head[f.path] === undefined ? "A" : w === null ? "D" : "M";
    let text = w;
    if (f.hunks && w !== null) {
      text = await callWasm<string>((await forgeWasm()).vcs_change_hunks, { base: head[f.path] ?? "", current: w, include: f.hunks });
    }
    committed.push({ path: f.path, origPath: null, status });
    if (text === null) {
      delete head[f.path];
      delete index[f.path];
    } else {
      head[f.path] = text;
      index[f.path] = text;
    }
  }
  const [subject, ...rest] = msg.split("\n");
  const body = [rest.join("\n").trim(), opts.signOff ? "Signed-off-by: Marlon <marlon@acme.dev>" : ""].filter(Boolean).join("\n\n") || undefined;
  let sha: string;
  if (opts.amend) {
    const prev = mockRepo.bySha.get(mockRepo.state.head)!;
    mockRepo.commits.splice(mockRepo.commits.indexOf(prev), 1);
    sha = newCommit(subject || prev.subject, body ?? prev.body, prev.parents, [...prev.files, ...committed]);
  } else sha = newCommit(subject, body, [mockRepo.state.head], committed);
  recordMockCommit(msg, sha);
  changed();
  return sha;
});

registerMock("git_create_patch", async ({ paths, reverse }) => {
  let out = "";
  for (const p of (paths as string[]) ?? []) {
    const before = head[p] ?? null;
    const after = await readWorking(p);
    out += reverse ? unifiedDiff(p, after, before) : unifiedDiff(p, before, after);
  }
  return out;
});

const patchFiles = new Map<string, string>();
registerMock("git_read_patch", async ({ path }) => {
  const p = String(path);
  return patchFiles.get(p) ?? invoke<string>("fs_read_file", { path: p });
});
registerMock("git_write_patch", async ({ path, patch }) => {
  const p = String(path);
  // Absolute paths stand for files outside the demo project: kept in memory only.
  if (p.startsWith("/") || /^[A-Za-z]:[\\/]/.test(p)) patchFiles.set(p, String(patch));
  else await invoke("fs_write_file", { path: p, contents: String(patch) });
});

async function preview(patch: string, base: string): Promise<PatchPreviewFile[]> {
  return callWasm<PatchPreviewFile[]>((await forgeWasm()).vcs_patch_preview, { patch, base, files: await working() });
}

registerMock("git_patch_preview", ({ patch, base }) => preview(String(patch), String(base ?? "")));

registerMock("git_apply_patch", async ({ patch, base, paths }): Promise<ApplyPatchResult> => {
  await delay(200);
  const wanted = paths as string[] | null;
  const files = (await preview(String(patch), String(base ?? ""))).filter((f) => !wanted || wanted.includes(f.path));
  const res: ApplyPatchResult = { applied: [], conflicts: [], failed: [] };
  for (const f of files) {
    if (!f.applies || f.binary) {
      res.failed.push([f.path, f.message ?? "patch does not apply"]);
      continue;
    }
    if (f.status === "deleted") await invoke("fs_delete", { path: f.path });
    else await invoke("fs_write_file", { path: f.path, contents: f.after ?? "" });
    if (f.status === "added") index[f.path] = f.after ?? "";
    res.applied.push(f.path);
  }
  changed();
  return res;
});
