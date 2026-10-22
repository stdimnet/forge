/**
 * Browser-mode Git Log actions for the demo repository of `gitMock.ts`: action states (the Rust
 * rules through WebAssembly), cherry-pick, revert, undo, edit message, squash, drop and the
 * interactive rebase (the todo is planned in Rust, the mock replays it on the in-memory
 * history), patches and the files of a revision. `edit` steps do not stop here. Only the
 * in-memory repository changes, never a real project.
 */
import { invoke, registerMock } from "@/ipc";
import type { ChangedFile } from "@/features/git/api";
import type { OperationState } from "@/features/git/opsApi";
import type { LogActionStates, RebaseCommit, RebaseEntry, RebasePlan, RewriteResult } from "@/features/git/logApi";
import { unifiedDiff } from "@/features/git/textDiff";
import { callWasm, forgeWasm } from "@/wasm";
import { mockRepo, mockRepoOps, revisionContent, trackedPaths } from "./gitMock";

const { state, bySha, ancestors } = mockRepo;
const { changed, newCommit } = mockRepoOps;
const PROTECTED = ["main", "master"];

const commitOf = (sha: string) => {
  const c = bySha.get(sha);
  if (!c) throw new Error(`git: bad revision '${sha}'`);
  return c;
};
const messageOf = (sha: string) => {
  const c = commitOf(sha);
  return c.body ? `${c.subject}\n\n${c.body}` : c.subject;
};

/** Remote branches that contain `sha`. */
function pushedTo(sha: string): string[] {
  return [...state.remote].filter(([, tip]) => ancestors(tip).has(sha)).map(([name]) => name);
}

/** First-parent chain from HEAD down to `sha` (inclusive), oldest first. */
function rangeFrom(sha: string): string[] {
  const chain: string[] = [];
  let at: string | undefined = state.head;
  while (at) {
    chain.push(at);
    if (at === sha) return chain.reverse();
    at = bySha.get(at)?.parents[0];
  }
  throw new Error("The selected commit is not in the current branch");
}

registerMock("git_log_actions", async ({ shas }): Promise<LogActionStates> => {
  const list = shas as string[];
  const reachable = ancestors(state.head);
  const commits = list.map((sha) => {
    const c = commitOf(sha);
    const pushed = pushedTo(sha);
    const protectedBranch = pushed.find((r) => PROTECTED.includes(r.split("/").slice(1).join("/"))) ?? null;
    return {
      sha,
      inCurrentBranch: reachable.has(sha),
      head: sha === state.head,
      merge: c.parents.length > 1,
      root: c.parents.length === 0,
      pushedTo: protectedBranch ?? pushed[0] ?? null,
      protectedBranch,
    };
  });
  let mergesAfter = false;
  if (commits.every((c) => c.inCurrentBranch)) {
    const oldest = list.reduce((a, b) => (ancestors(b).has(a) ? a : b));
    const before = ancestors(oldest);
    mergesAfter = [...reachable].some((s) => !before.has(s) && commitOf(s).parents.length > 1);
  }
  const op = await invoke<OperationState>("git_operation_state");
  const facts = { commits, mergesAfter, operation: op.operation, detached: state.current === null };
  return callWasm<LogActionStates>((await forgeWasm()).vcs_log_action_states, facts);
});

registerMock("git_cherry_pick", ({ shas }) => {
  const list = (shas as string[]).slice().reverse();
  for (const sha of list) {
    const c = commitOf(sha);
    const suffix = pushedTo(sha).some((r) => PROTECTED.includes(r.split("/").slice(1).join("/")));
    const body = [c.body, suffix ? `(cherry picked from commit ${sha})` : ""].filter(Boolean).join("\n\n") || undefined;
    newCommit(c.subject, body, [state.head], c.files);
  }
  changed();
  return "";
});

registerMock("git_revert_commits", ({ shas }) => {
  for (const sha of shas as string[]) {
    const c = commitOf(sha);
    newCommit(`Revert "${c.subject}"`, `This reverts commit ${sha}.`, [state.head], c.files);
  }
  changed();
  return "";
});

registerMock("git_undo_commit", ({ sha }) => {
  if (sha !== state.head) throw new Error("Undo Commit is available only for the last commit of the current branch");
  const message = messageOf(String(sha));
  state.head = commitOf(String(sha)).parents[0];
  if (state.current) state.local.set(state.current, state.head);
  changed();
  return { message };
});

registerMock("git_undo_rewrite", ({ oldHead, newHead }) => {
  if (state.head !== newHead) throw new Error("the branch has changed since; cannot undo");
  state.head = String(oldHead);
  if (state.current) state.local.set(state.current, state.head);
  changed();
});

type Step = { kind: "pick" | "edit" | "squash" | "fixup" | "drop"; value: string } | { kind: "amend"; value: number };

/** Replay a plan (oldest first) on top of `base`, like `git rebase -i`. */
async function replay(base: string | undefined, entries: RebaseEntry[]): Promise<RewriteResult> {
  const { steps, messages } = await callWasm<{ steps: Step[]; messages: string[] }>((await forgeWasm()).vcs_rebase_todo, entries);
  const oldHead = state.head;
  type Pending = { subject: string; body?: string; files: ChangedFile[] };
  let current = null as Pending | null;
  let parent = base;
  const flush = () => {
    if (!current) return;
    parent = newCommit(current.subject, current.body, parent ? [parent] : [], current.files);
    current = null;
  };
  for (const step of steps) {
    if (step.kind === "amend") {
      const [subject, ...rest] = messages[step.value].split("\n");
      if (current) current = { ...current, subject, body: rest.join("\n").trim() || undefined };
      continue;
    }
    if (step.kind === "drop") continue;
    const c = commitOf(step.value);
    if (step.kind === "pick" || step.kind === "edit") {
      flush();
      current = { subject: c.subject, body: c.body, files: c.files };
    } else if (current) {
      const into: Pending = current;
      const files = [...into.files, ...c.files.filter((f) => !into.files.some((x) => x.path === f.path))];
      const body = step.kind === "squash" ? [into.body, messageOf(c.sha)].filter(Boolean).join("\n\n") : into.body;
      current = { ...into, body, files };
    }
  }
  flush();
  if (!parent) throw new Error("the mock cannot drop every commit");
  state.head = parent;
  if (state.current) state.local.set(state.current, parent);
  changed();
  return { output: "Successfully rebased and updated the branch.", oldHead, newHead: state.head, stopped: false };
}

function picks(range: string[]): RebaseEntry[] {
  return range.map((sha) => ({ sha, action: "pick" }));
}

registerMock("git_rebase_plan", async ({ sha }): Promise<RebasePlan> => {
  const range = rangeFrom(String(sha));
  const commits: RebaseCommit[] = range.map((s) => {
    const c = commitOf(s);
    return { sha: s, subject: c.subject, message: messageOf(s), author: c.author, time: c.time };
  });
  const entries = await callWasm<RebaseEntry[]>((await forgeWasm()).vcs_rebase_autosquash, commits);
  return { base: commitOf(range[0]).parents[0] ?? null, branch: state.current, commits, entries };
});

registerMock("git_interactive_rebase", ({ sha, entries }) => {
  const range = rangeFrom(String(sha));
  const plan = entries as RebaseEntry[];
  if (plan.length !== range.length || !plan.every((e) => range.includes(e.sha)))
    throw new Error("the branch has changed since the rebase was planned; start the rebase again");
  return replay(commitOf(range[0]).parents[0], plan);
});

registerMock("git_reword", ({ sha, message }) => {
  const range = rangeFrom(String(sha));
  const entries = picks(range).map((e) => (e.sha === sha ? { ...e, action: "reword" as const, message: String(message) } : e));
  return replay(commitOf(range[0]).parents[0], entries);
});

registerMock("git_drop_commits", ({ shas }) => {
  const list = shas as string[];
  const oldest = list.reduce((a, b) => (ancestors(b).has(a) ? a : b));
  const range = rangeFrom(oldest);
  return replay(
    commitOf(range[0]).parents[0],
    picks(range).map((e) => (list.includes(e.sha) ? { ...e, action: "drop" as const } : e)),
  );
});

registerMock("git_squash_commits", ({ shas, message }) => {
  const list = shas as string[];
  const oldest = list.reduce((a, b) => (ancestors(b).has(a) ? a : b));
  const range = rangeFrom(oldest);
  const moved = range.filter((s) => list.includes(s) && s !== oldest);
  const rest = range.filter((s) => !moved.includes(s));
  const entries: RebaseEntry[] = rest.flatMap((s): RebaseEntry[] =>
    s === oldest
      ? [{ sha: s, action: "pick" }, ...moved.map((m, i): RebaseEntry => ({ sha: m, action: "squash", message: i === moved.length - 1 ? String(message) : null }))]
      : [{ sha: s, action: "pick" }],
  );
  return replay(commitOf(range[0]).parents[0], entries);
});

registerMock("git_commits_patch", ({ shas }) => {
  const out: string[] = [];
  for (const sha of (shas as string[]).slice().reverse()) {
    const c = commitOf(sha);
    const parent = c.parents[0];
    for (const f of c.files) {
      const before = parent && f.status !== "A" ? revisionContent(parent, f.path) : null;
      const after = f.status === "D" ? null : revisionContent(sha, f.path);
      out.push(unifiedDiff(f.path, before, after ?? `<?php\n// ${c.subject}\n`));
    }
  }
  return out.filter(Boolean).join("");
});

registerMock("git_tree_files", () => trackedPaths().sort());
