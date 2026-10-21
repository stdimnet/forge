/**
 * Bookmarks of the open project. The model (lists, mnemonics, line tracking through edits)
 * lives in Rust (`forge-bookmarks`); this store holds the last answer and sends changes.
 * Edits of open documents are batched per path and sent as `applyEdits` operations, so the
 * stored lines follow the text.
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { bookmarksApi, type Bookmark, type BookmarkOp, type Bookmarks, type LineEdit } from "@/ipc/workbench";
import { workspace } from "@/state/workspace";
import { onFsChanged } from "@/state/fsWatch";
import { onPathMoved } from "@/state/editor";
import { showBalloon } from "@/keymap";

const EMPTY: Bookmarks = { lists: [{ name: "Bookmarks", bookmarks: [] }], defaultList: "Bookmarks", nextId: 1 };

const [bookmarks, setBookmarks] = createSignal<Bookmarks>(EMPTY);
export { bookmarks };

/** Every bookmark with its list name. */
export const allBookmarks = (): (Bookmark & { list: string })[] =>
  bookmarks().lists.flatMap((l) => l.bookmarks.map((b) => ({ ...b, list: l.name })));

export const bookmarksIn = (path: string) => allBookmarks().filter((b) => b.path === path);

export const bookmarkAt = (path: string, line: number) => allBookmarks().find((b) => b.path === path && b.line === line);

export const bookmarkByMnemonic = (m: string) => allBookmarks().find((b) => b.mnemonic === m.toUpperCase());

let chain: Promise<unknown> = Promise.resolve();

/** Apply operations in order; the answer replaces the store. */
export function applyBookmarks(ops: BookmarkOp[]): Promise<void> {
  if (!ops.length) return Promise.resolve();
  const run = chain.then(async () => {
    try {
      setBookmarks(await bookmarksApi.apply(ops));
    } catch (e) {
      showBalloon("Bookmarks could not be saved", String(e instanceof Error ? e.message : e));
    }
  });
  chain = run;
  return run;
}

// ---- Edits of open documents, batched

const pending = new Map<string, LineEdit[][]>();
let timer: ReturnType<typeof setTimeout> | undefined;

/** Paths with edits that are not sent yet (their gutter keeps its own mapped positions). */
export const hasPendingEdits = (path: string) => pending.has(path);

/** Queue the line edits of one transaction of `path`. */
export function queueEdits(path: string, edits: LineEdit[]): void {
  if (!edits.length || !bookmarksIn(path).length) return;
  pending.set(path, [...(pending.get(path) ?? []), edits]);
  clearTimeout(timer);
  timer = setTimeout(flushEdits, 300);
}

export function flushEdits(): Promise<void> {
  clearTimeout(timer);
  const ops: BookmarkOp[] = [];
  for (const [path, batches] of pending) for (const edits of batches) ops.push({ op: "applyEdits", path, edits });
  pending.clear();
  return applyBookmarks(ops);
}

export async function reloadBookmarks(): Promise<void> {
  try {
    setBookmarks(await bookmarksApi.get());
  } catch {
    setBookmarks(EMPTY);
  }
}

createRoot(() => {
  createEffect(
    on(
      () => workspace()?.root,
      (root) => {
        pending.clear();
        if (root) void reloadBookmarks();
        else setBookmarks(EMPTY);
      },
    ),
  );
});

onPathMoved((from, to) => void applyBookmarks([to === null ? { op: "removePath", path: from } : { op: "renamePath", from, to }]));
onFsChanged((c) => {
  const gone = c.removed.filter((p) => allBookmarks().some((b) => b.path === p || b.path.startsWith(`${p}/`)));
  void applyBookmarks(gone.map((path) => ({ op: "removePath", path })));
});
