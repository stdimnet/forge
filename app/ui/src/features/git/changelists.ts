/**
 * Changelists and commit inclusion of the Commit tool window, like PHPStorm:
 * - the changelists with their files come from Rust (`git_changelists`, stored per project in
 *   Forge's data directory); new changes go to the active list;
 * - inclusion: which files are checked for the next commit and which of their change hunks
 *   are excluded (partial commits from the commit diff and the editor gutter).
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { changesApi, type ChangeHunk, type ChangelistView, type CommitFileSpec } from "./changesApi";
import { notify, revision, status } from "./store";

const [lists, setLists] = createSignal<ChangelistView[]>([]);
export { lists as changelists };

let refreshing: Promise<void> | null = null;
let queued = false;

/** Re-read the changelists (after every repository change; concurrent calls coalesce). */
export async function refreshChangelists(): Promise<void> {
  if (refreshing) {
    queued = true;
    return refreshing;
  }
  refreshing = changesApi
    .changelists()
    .then(
      (l) => void setLists(l),
      () => void setLists([]),
    )
    .finally(() => (refreshing = null));
  await refreshing;
  if (queued) {
    queued = false;
    await refreshChangelists();
  }
}

createRoot(() => createEffect(on(revision, () => void refreshChangelists())));

export const activeChangelist = (): ChangelistView | undefined => lists().find((l) => l.active);

/** The lists that hold (part of) `path`. */
export const listsOf = (path: string): ChangelistView[] => lists().filter((l) => l.files.some((f) => f.path === path));

/** Run a changelist operation; it answers with the new lists. */
export async function changelistOp(title: string, op: () => Promise<ChangelistView[]>): Promise<boolean> {
  try {
    setLists(await op());
    return true;
  } catch (err) {
    notify("error", title, String(err instanceof Error ? err.message : err), 10000);
    return false;
  }
}

// ---- hunks ------------------------------------------------------------------------------

const [hunks, setHunks] = createSignal<Record<string, ChangeHunk[]>>({});
export { hunks as fileHunks };

/** Fetch the change hunks of a file (against HEAD); `content` is an unsaved buffer. */
export async function loadHunks(path: string, content: string | null = null): Promise<ChangeHunk[]> {
  const list = await changesApi.fileHunks(path, content).catch(() => [] as ChangeHunk[]);
  setHunks((h) => ({ ...h, [path]: list }));
  return list;
}

/** All hunk ids of a file: from the changelists when it is split, else the fetched hunks. */
function allHunkIds(path: string, known?: string[]): string[] | null {
  if (known) return known;
  const split = lists().flatMap((l) => l.files.filter((f) => f.path === path && f.hunks).flatMap((f) => f.hunks!));
  if (split.length) return split;
  const fetched = hunks()[path];
  return fetched ? fetched.map((h) => h.id) : null;
}

// ---- inclusion ----------------------------------------------------------------------------

const [checked, setChecked] = createSignal<Set<string>>(new Set());
/** Excluded hunk ids per checked file. */
const [excluded, setExcluded] = createSignal<Record<string, string[]>>({});
const [known, setKnown] = createSignal<Set<string>>(new Set());
export { checked as includedFiles, excluded as excludedHunks };

/** Include exactly these files (Commit File…). */
export function setIncluded(paths: string[]): void {
  setChecked(new Set(paths));
  setExcluded({});
}

export type Inclusion = "all" | "some" | "none";

/**
 * Inclusion of a file as one changelist shows it: `hunks` are the hunks of the file in that
 * list, `null` for the whole file.
 */
export function inclusionOf(path: string, hunkIds: string[] | null): Inclusion {
  if (!checked().has(path)) return "none";
  const ex = excluded()[path] ?? [];
  if (!ex.length) return "all";
  const ids = hunkIds ?? allHunkIds(path);
  if (!ids) return "some";
  const n = ids.filter((id) => !ex.includes(id)).length;
  return n === 0 ? "none" : n === ids.length ? "all" : "some";
}

/** Check or uncheck a file, or with `hunkIds` those hunks of it. */
export function setInclusion(path: string, hunkIds: string[] | null, on: boolean, all?: string[]): void {
  const ex = new Set(excluded()[path] ?? []);
  const isChecked = checked().has(path);
  let nextChecked = isChecked;
  if (hunkIds === null) {
    nextChecked = on;
    ex.clear();
  } else if (on) {
    if (!isChecked) {
      // Only these hunks: exclude the file's other hunks.
      ex.clear();
      for (const id of allHunkIds(path, all) ?? []) if (!hunkIds.includes(id)) ex.add(id);
    } else hunkIds.forEach((id) => ex.delete(id));
    nextChecked = true;
  } else {
    hunkIds.forEach((id) => ex.add(id));
    const ids = allHunkIds(path, all);
    if (ids && ids.every((id) => ex.has(id))) {
      nextChecked = false;
      ex.clear();
    }
  }
  setChecked((prev) => {
    const next = new Set(prev);
    nextChecked ? next.add(path) : next.delete(path);
    return next;
  });
  setExcluded((prev) => {
    const next = { ...prev };
    if (ex.size && nextChecked) next[path] = [...ex];
    else delete next[path];
    return next;
  });
}

/** Whether one hunk is included (gutter and diff checkboxes). */
export const isHunkIncluded = (path: string, id: string): boolean => checked().has(path) && !(excluded()[path] ?? []).includes(id);

/** Toggle one hunk; `all` are the file's current hunk ids. */
export function setHunkIncluded(path: string, id: string, on: boolean, all: string[]): void {
  setInclusion(path, [id], on, all);
}

/** Toggle many files at once (group checkboxes). */
export function setManyIncluded(items: { path: string; hunks: string[] | null }[], on: boolean): void {
  for (const i of items) setInclusion(i.path, i.hunks, on);
}

/**
 * The files of the next commit: checked files, partly where hunks are excluded. Hunks of
 * files with exclusions are refreshed first so edited hunks are not lost.
 */
export async function commitSpecs(paths: string[]): Promise<CommitFileSpec[]> {
  const out: CommitFileSpec[] = [];
  for (const path of paths) {
    if (!checked().has(path)) continue;
    const ex = excluded()[path] ?? [];
    if (!ex.length) {
      out.push({ path, hunks: null });
      continue;
    }
    const current = (await loadHunks(path)).map((h) => h.id);
    const included = current.filter((id) => !ex.includes(id));
    if (!included.length) continue;
    out.push({ path, hunks: included.length === current.length ? null : included });
  }
  return out;
}

// New tracked changes of the active changelist start checked, unversioned files unchecked
// (PHPStorm default); files without changes leave the inclusion.
createRoot(() =>
  createEffect(
    on(status, (st) => {
      if (!st) return;
      const prev = known();
      const next = new Set(checked());
      const all = new Set(st.entries.map((e) => e.path));
      const active = activeChangelist();
      for (const e of st.entries) {
        if (prev.has(e.path) || e.worktree === "untracked" || e.conflict) continue;
        const owner = lists().find((l) => l.files.some((f) => f.path === e.path));
        if (!owner || !active || owner.id === active.id) next.add(e.path);
      }
      for (const p of next) if (!all.has(p)) next.delete(p);
      setKnown(all);
      setChecked(next);
      setExcluded((ex) => Object.fromEntries(Object.entries(ex).filter(([p]) => next.has(p))));
    }),
  ),
);
