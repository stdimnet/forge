/**
 * Browser-mode Git menu dialogs and shelf for the demo repository of `gitMock.ts`: Pull,
 * Merge and Rebase with options (merging or rebasing onto `feature/checkout-flow` stops with
 * the demo conflicts), Continue/Abort/Skip, Reset HEAD, tags, remotes, stash details and an
 * in-memory shelf. Only the demo workspace mock is written, never a real project.
 * `?rebasing` opens the demo in the middle of a rebase (screenshots).
 */
import { invoke, registerMock } from "@/ipc";
import type { ChangedFile } from "@/features/git/api";
import type { OperationState, PatchStatus, ShelvedChangelist } from "@/features/git/opsApi";
import { mockRepo, mockRepoOps } from "./gitMock";

const { state, head } = mockRepo;
const { index, changed, readWorking, merging, startConflictMerge, CONFLICT_BRANCH } = mockRepoOps;
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const now = () => Math.floor(Date.now() / 1000);
const DAY = 86400;

/** The rebase in progress, if any (merges are `merging.theirs`). */
let rebasing: { branch: string; onto: string } | null = null;

/** `rev`, `rev~n` and `rev^`: follow first parents in the demo history. */
function resolve(rev: string): string | undefined {
  const m = /^(.*?)((?:[~^]\d*)*)$/.exec(rev.trim());
  let sha = mockRepo.resolveRev(m?.[1] || "HEAD");
  for (const step of (m?.[2] ?? "").match(/[~^]\d*/g) ?? []) {
    const n = step.length > 1 ? Number(step.slice(1)) : 1;
    for (let i = 0; i < (step[0] === "~" ? n : 1) && sha; i++) sha = mockRepo.bySha.get(sha)?.parents[step[0] === "^" ? n - 1 : 0];
  }
  return sha;
}

const isConflictRev = (rev: string) => rev === CONFLICT_BRANCH || rev === `origin/${CONFLICT_BRANCH}`;

async function startRebaseConflict(onto: string): Promise<never> {
  rebasing = { branch: state.current ?? "HEAD", onto: onto.replace(/^origin\//, "") };
  await startConflictMerge(CONFLICT_BRANCH);
  changed();
  throw new Error(
    `git rebase: could not apply 4f2a9c1... Use net prices in the calculator\nCONFLICT (content): Merge conflict in app/Services/PriceCalculator.php`,
  );
}

if (typeof location !== "undefined" && new URLSearchParams(location.search).has("rebasing")) {
  void Promise.resolve()
    .then(() => startRebaseConflict(CONFLICT_BRANCH))
    .catch(() => {});
}

registerMock("git_pull_with", async ({ options }) => {
  const o = options as { remote: string; branches: string[]; rebase?: boolean; ffOnly?: boolean; noFf?: boolean; squash?: boolean };
  await delay(500);
  if (o.rebase && (o.ffOnly || o.noFf || o.squash)) throw new Error("--rebase cannot be combined with --ff-only, --no-ff, --squash or --no-commit");
  if (o.branches.some(isConflictRev)) {
    if (o.rebase) return startRebaseConflict(CONFLICT_BRANCH);
    await invoke("git_merge", { rev: CONFLICT_BRANCH });
  }
  changed();
  return { output: "Already up to date." };
});

// "Update" on a local branch that is not checked out: fast-forward to its tracked branch.
registerMock("git_update_branch", async ({ name }) => {
  const n = String(name);
  await delay(300);
  const local = state.local.get(n);
  if (!local) throw new Error(`no local branch '${n}'`);
  if (n === state.current) throw new Error(`'${n}' is checked out: use Update Project`);
  const upstream = state.upstream.get(n);
  const target = upstream ? state.remote.get(upstream) : undefined;
  if (!upstream || !target) throw new Error(`'${n}' has no tracked branch`);
  if (local === target) return { output: "Already up to date." };
  if (!mockRepo.ancestors(target).has(local)) throw new Error(`'${n}' has commits that are not in its tracked branch. Check it out and update it there.`);
  state.local.set(n, target);
  changed();
  return { output: `${local.slice(0, 8)}..${target.slice(0, 8)}` };
});

registerMock("git_merge_with", async ({ options }) => {
  const o = options as { branches: string[]; ffOnly?: boolean; squash?: boolean; noFf?: boolean };
  if (!o.branches.length) throw new Error("choose the branch to merge");
  if (o.ffOnly && (o.squash || o.noFf)) throw new Error("--ff-only cannot be combined with --no-ff, --squash or -m");
  let out = "";
  for (const rev of o.branches) out = await invoke<string>("git_merge", { rev });
  return out;
});

registerMock("git_rebase_with", async ({ options }) => {
  const o = options as { onto: string; root?: boolean };
  await delay(300);
  if (isConflictRev(o.onto)) return startRebaseConflict(o.onto);
  if (!o.root && !mockRepo.resolveRev(o.onto)) throw new Error(`git rebase: invalid upstream '${o.onto}'`);
  changed();
  return `Successfully rebased and updated refs/heads/${state.current} onto ${o.onto || "root"}.`;
});

registerMock(
  "git_operation_state",
  (): OperationState =>
    rebasing
      ? { operation: "rebase", branch: rebasing.branch, onto: rebasing.onto }
      : merging.theirs
        ? { operation: "merge", branch: state.current, onto: null }
        : { operation: null, branch: null, onto: null },
);

registerMock("git_operation_action", async ({ action }) => {
  const op = rebasing ? "rebase" : merging.theirs ? "merge" : null;
  if (!op) throw new Error("no operation is in progress");
  await delay(200);
  if (action === "continue" && merging.conflicts.size)
    throw new Error(`git ${op}: you must edit all merge conflicts and then mark them as resolved using git add`);
  if (action === "skip" && op === "merge") throw new Error("a merge cannot skip commits");
  if (action === "abort" || action === "skip") {
    for (const [path, c] of merging.conflicts) if (!c.binary && c.ours !== null) await invoke("fs_write_file", { path, contents: c.ours });
    merging.conflicts.clear();
  }
  if (op === "merge" && action === "continue") await invoke("git_commit", { message: `Merge branch '${merging.theirs}'`, amend: false, paths: null });
  merging.theirs = null;
  rebasing = null;
  changed();
  return "";
});

registerMock("git_reset_head", async ({ rev, mode }) => {
  const sha = resolve(String(rev || "HEAD"));
  if (!sha) throw new Error(`git reset: ambiguous argument '${rev}': unknown revision`);
  state.head = sha;
  if (state.current) state.local.set(state.current, sha);
  if (mode === "hard") for (const [path, text] of Object.entries(head)) await invoke("fs_write_file", { path, contents: text });
  changed();
});

registerMock("git_tag_with", async ({ options }) => {
  const o = options as { name: string; commit?: string | null; force?: boolean };
  if (o.force) state.tags.delete(o.name.trim());
  await invoke("git_create_tag", { name: o.name.trim(), rev: o.commit || null, message: null });
});

registerMock("git_resolve_commit", ({ rev }) => resolve(String(rev)) ?? null);

registerMock("git_edit_remote", ({ oldName, name, url }) => {
  const r = state.remotes.find((x) => x.name === oldName);
  if (!r) throw new Error(`git remote: No such remote: '${oldName}'`);
  if (name !== oldName && state.remotes.some((x) => x.name === name)) throw new Error(`git remote: remote ${name} already exists.`);
  r.name = String(name);
  r.fetchUrl = String(url);
  changed();
});

// ---- stashes ------------------------------------------------------------------------------

// The demo stash changes two files (the generic stash mock keeps only the stashed texts).
const demoStash = state.stashes[0];
if (demoStash && !Object.keys(demoStash.files).length) {
  const order = head["app/Models/Order.php"];
  if (order !== undefined)
    demoStash.files["app/Models/Order.php"] = order.replace(/\n}\s*$/, "\n\n    public function invoicePdf(): string\n    {\n        return view('invoices.pdf', ['order' => $this])->render();\n    }\n}\n");
  demoStash.files["resources/views/invoices/pdf.blade.php"] = "<h1>Invoice {{ $order->number }}</h1>\n<table class=\"items\"></table>\n";
}

const stashAt = (i: unknown) => {
  const s = state.stashes[Number(i)];
  if (!s) throw new Error(`git stash: stash@{${i}} is not a valid reference`);
  return s;
};

registerMock("git_stash_files", ({ index: i }): ChangedFile[] =>
  Object.keys(stashAt(i).files).map((path) => ({ path, origPath: null, status: head[path] === undefined ? "A" : "M" })),
);
registerMock("git_stash_file", ({ index: i, path }) => ({ before: head[String(path)] ?? null, after: stashAt(i).files[String(path)] ?? null }));
registerMock("git_stash_clear", () => {
  state.stashes.splice(0);
  changed();
});
registerMock("git_stash_branch", async ({ index: i, branch }) => {
  stashAt(i);
  await invoke("git_create_branch", { name: branch, start: null, checkout: true });
  await invoke("git_unstash", { index: i, pop: true, reinstateIndex: false });
  return `Switched to a new branch '${branch}'`;
});

// ---- shelf --------------------------------------------------------------------------------

interface MockShelved {
  id: string;
  name: string;
  date: number;
  branch: string | null;
  files: { path: string; status: PatchStatus; before: string | null; after: string | null }[];
}

const shelf: MockShelved[] = [];
{
  const user = head["app/Models/User.php"];
  const routes = head["routes/web.php"];
  if (user !== undefined && routes !== undefined)
    shelf.push({
      id: "1760000000",
      name: "Experiment: customer loyalty points",
      date: now() - 2 * DAY,
      branch: "main",
      files: [
        { path: "app/Models/User.php", status: "modified", before: user, after: user.replace("class User extends Authenticatable\n{", "class User extends Authenticatable\n{\n    public int $loyaltyPoints = 0;\n") },
        { path: "app/Services/Loyalty.php", status: "added", before: null, after: "<?php\n\nnamespace App\\Services;\n\nclass Loyalty\n{\n    public function pointsFor(int $cents): int\n    {\n        return intdiv($cents, 100);\n    }\n}\n" },
        { path: "routes/web.php", status: "modified", before: routes, after: `${routes}Route::get('/loyalty', fn () => view('loyalty'));\n` },
      ],
    });
}

const shelfView = (s: MockShelved): ShelvedChangelist => ({
  id: s.id,
  name: s.name,
  date: s.date,
  branch: s.branch,
  files: s.files.map((f) => ({ path: f.path, status: f.status, binary: false })),
});
const shelvedAt = (id: unknown) => {
  const s = shelf.find((x) => x.id === id);
  if (!s) throw new Error(`invalid shelf id "${id}"`);
  return s;
};

registerMock("git_shelf_list", () => shelf.map(shelfView).sort((a, b) => b.date - a.date));

registerMock("git_shelve", async ({ paths, name }) => {
  const files: MockShelved["files"] = [];
  for (const path of paths as string[]) {
    const before = head[path] ?? null;
    const after = await readWorking(path);
    if (before === after) continue;
    files.push({ path, status: before === null ? "added" : after === null ? "deleted" : "modified", before, after });
    if (before === null) {
      await invoke("fs_delete", { path }).catch(() => {});
      delete index[path];
    } else {
      await invoke("fs_write_file", { path, contents: before });
      index[path] = before;
    }
  }
  if (!files.length) throw new Error("no changes to shelve");
  const s: MockShelved = { id: String(Date.now()), name: String(name).trim() || "Changes", date: now(), branch: state.current, files };
  shelf.push(s);
  changed();
  return shelfView(s);
});

registerMock("git_unshelve", async ({ id, paths, remove }) => {
  const s = shelvedAt(id);
  const files = s.files.filter((f) => !paths || (paths as string[]).includes(f.path));
  if (!files.length) throw new Error("nothing to unshelve");
  for (const f of files) {
    if (f.after === null) await invoke("fs_delete", { path: f.path }).catch(() => {});
    else await invoke("fs_write_file", { path: f.path, contents: f.after });
    if (f.status === "added" && f.after !== null) index[f.path] = f.after;
  }
  if (remove) {
    s.files = s.files.filter((f) => !files.includes(f));
    if (!s.files.length) shelf.splice(shelf.indexOf(s), 1);
  }
  changed();
  return `${files.length} file${files.length === 1 ? "" : "s"} unshelved`;
});

registerMock("git_shelf_delete", ({ id }) => {
  shelf.splice(shelf.indexOf(shelvedAt(id)), 1);
  changed();
});

registerMock("git_shelf_rename", ({ id, name }) => {
  const s = shelvedAt(id);
  if (!String(name).trim()) throw new Error("the name must not be empty");
  s.name = String(name).trim();
  changed();
  return shelfView(s);
});

registerMock("git_shelf_file", ({ id, path }) => {
  const f = shelvedAt(id).files.find((x) => x.path === path);
  if (!f) throw new Error(`${path} is not in this shelved changelist`);
  return { before: f.before, after: f.after, binary: false };
});
