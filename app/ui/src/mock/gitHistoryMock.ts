/**
 * Browser-mode git history: Show History, Show History for Selection, Annotate Revision and
 * Compare with Branch on a folder, answered from the demo repository of `gitMock.ts`. Older
 * revisions of a file are its HEAD lines blamed on that commit or an older one, so history,
 * diffs and blame agree.
 */
import { registerMock } from "@/ipc";
import type { Blame, BlameCommit, ChangedFile } from "@/features/git/api";
import type { BlockRevision, FileRevision, HistoryQuery } from "@/features/git/historyApi";
import { mockRepo, revisionContent, revisionLines } from "./gitMock";

const under = (path: string, dir: boolean) => (p: string) => (dir ? path === "" || p === path || p.startsWith(`${path}/`) : p === path);

registerMock("git_file_history", ({ path, dir, query }): FileRevision[] => {
  const p = String(path ?? "");
  const q = (query ?? {}) as HistoryQuery;
  const match = under(p, !!dir);
  const reachable = mockRepo.ancestors(mockRepo.state.head);
  const revs = mockRepo.commits
    .filter((c) => reachable.has(c.sha) && c.files.some((f) => match(f.path)))
    .sort((a, b) => b.time - a.time)
    .map((c) => ({
      sha: c.sha,
      parents: c.parents,
      author: c.author,
      email: c.email,
      time: c.time,
      subject: c.subject,
      files: c.files.filter((f) => match(f.path)),
    }));
  const skip = q.skip ?? 0;
  return revs.slice(skip, skip + (q.limit || 200));
});

registerMock("git_block_history", ({ path, start, end }): BlockRevision[] => {
  const p = String(path);
  const [from, to] = [Number(start), Number(end)];
  if (!(from >= 1 && to >= from)) throw new Error(`invalid line range ${start}..${end}`);
  if (mockRepo.head[p] === undefined) throw new Error(`fatal: file ${p} has only 0 lines`);
  const inBlock = (time: number) => revisionLines(p, time).filter((l) => l.line >= from && l.line <= to);
  const reachable = mockRepo.ancestors(mockRepo.state.head);
  const out: BlockRevision[] = [];
  for (const c of [...mockRepo.commits].sort((a, b) => b.time - a.time)) {
    if (!reachable.has(c.sha) || !c.files.some((f) => f.path === p)) continue;
    const after = inBlock(c.time);
    const parent = c.parents[0] ? mockRepo.bySha.get(c.parents[0]) : undefined;
    const before = parent ? inBlock(parent.time) : [];
    const text = (ls: typeof after) => ls.map((l) => l.text).join("");
    if (text(after) === text(before) || !after.length) continue;
    const first = after[0].line;
    out.push({
      sha: c.sha,
      parents: c.parents,
      author: c.author,
      email: c.email,
      time: c.time,
      subject: c.subject,
      path: p,
      before: before.length ? text(before) : null,
      after: text(after),
      startLine: revisionLines(p, c.time).filter((l) => l.line < first).length + 1,
    });
  }
  return out;
});

registerMock("git_blame_revision", ({ rev, path }): Blame => {
  const p = String(path);
  const sha = mockRepo.resolveRev(String(rev));
  const at = sha ? mockRepo.bySha.get(sha) : undefined;
  if (!at || revisionContent(at.sha, p) === null) throw new Error(`fatal: no such path ${p} in ${rev}`);
  const out: Blame = { commits: [], lines: [] };
  const index = new Map<string, number>();
  for (const l of revisionLines(p, at.time)) {
    if (!index.has(l.sha)) {
      const c = mockRepo.bySha.get(l.sha)!;
      const bc: BlameCommit = { sha: c.sha, author: c.author, email: c.email, time: c.time, summary: c.subject, uncommitted: false };
      index.set(l.sha, out.commits.length);
      out.commits.push(bc);
    }
    out.lines.push(index.get(l.sha)!);
  }
  return out;
});

registerMock("git_changes_since", async ({ rev, path }): Promise<ChangedFile[]> => {
  const tip = mockRepo.resolveRev(String(rev));
  if (!tip) throw new Error(`fatal: bad revision '${rev}'`);
  const match = under(String(path ?? ""), true);
  const theirs = mockRepo.ancestors(tip);
  const changed = new Map<string, string>();
  for (const sha of mockRepo.ancestors(mockRepo.state.head)) {
    if (theirs.has(sha)) continue;
    for (const f of mockRepo.bySha.get(sha)?.files ?? []) if (match(f.path)) changed.set(f.path, f.status === "A" ? "A" : (changed.get(f.path) ?? "M"));
  }
  for (const e of (await mockRepo.computeStatus()).entries) {
    if (!match(e.path) || e.worktree === "untracked") continue;
    changed.set(e.path, e.index === "added" ? "A" : e.worktree === "deleted" || e.index === "deleted" ? "D" : (changed.get(e.path) ?? "M"));
  }
  return [...changed].sort(([a], [b]) => a.localeCompare(b)).map(([p, status]) => ({ path: p, origPath: null, status }));
});
