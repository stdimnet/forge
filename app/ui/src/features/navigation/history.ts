/**
 * Navigation history: recent files (Ctrl+E), edited files, Back/Forward (Ctrl+Alt+Left/Right)
 * and recently closed tabs.
 *
 * Back/Forward work on places (file and caret), like PHPStorm: a place is recorded when the
 * active file changes and before every jump to a position (`openFileAt`: Ctrl+Click, Go to
 * Declaration, search results), also inside the same file.
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { openFileAt, scrollStateOf, viewFor, type ScrollState } from "@/editor/views";
import { editor, isCustomTab, isDirty, onBeforeNavigate, onTabsReset, openFile } from "@/state/editor";

const MAX_RECENT = 50;
const MAX_PLACES = 100;

interface Place {
  path: string;
  /** 1-based caret line and column. */
  line: number;
  col: number;
  /** Where the editor was scrolled; Back / Forward restore it like PHPStorm. */
  scroll?: ScrollState;
}

const [recentFiles, setRecentFiles] = createSignal<string[]>([]);
const [editedFiles, setEditedFiles] = createSignal<string[]>([]);
const [closedTabs, setClosedTabs] = createSignal<string[]>([]);
const back: Place[] = [];
const forward: Place[] = [];
let navigating = false;

export { recentFiles, editedFiles, closedTabs };

const bump = (list: string[], path: string) => [path, ...list.filter((p) => p !== path)].slice(0, MAX_RECENT);

/** Where the caret of `path` is (the start of the file when it is not open any more). */
function placeOf(path: string): Place {
  const cursor = editor.tabs.find((t) => t.path === path)?.cursor;
  return { path, line: cursor?.line ?? 1, col: cursor?.col ?? 1, scroll: scrollStateOf(viewFor(path)) };
}

const samePlace = (a: Place, b: Place) => a.path === b.path && a.line === b.line;

/** Push `place` onto the Back list; a new place drops the Forward list. */
function remember(place: Place): void {
  if (navigating) return;
  const top = back[back.length - 1];
  if (top && samePlace(top, place)) return;
  back.push(place);
  if (back.length > MAX_PLACES) back.shift();
  forward.length = 0;
}

onBeforeNavigate(({ path, line }) => {
  const current = editor.active;
  if (!current || isCustomTab(current)) return;
  const here = placeOf(current);
  // Already there: nothing to come back to.
  if (path === current && (!line || line === here.line)) return;
  remember(here);
});

createRoot(() => {
  createEffect(
    on(
      () => editor.active,
      (path, prev) => {
        if (!path || isCustomTab(path)) return;
        setRecentFiles((l) => bump(l, path));
        if (prev && prev !== path && !isCustomTab(prev)) remember(placeOf(prev));
      },
    ),
  );
  createEffect(() => {
    for (const t of editor.tabs) if (isDirty(t) && !editedFiles().includes(t.path)) setEditedFiles((l) => bump(l, t.path));
  });
  createEffect(
    on(
      () => editor.tabs.map((t) => t.path),
      (now, before) => {
        if (!before) return;
        const closed = before.filter((p) => !now.includes(p));
        if (closed.length) setClosedTabs((l) => [...closed, ...l].slice(0, MAX_RECENT));
      },
    ),
  );
});

// The paths belong to the project that was open; another project (a Code With Me host's, an
// SSH host's) must not inherit them.
onTabsReset(() => {
  setRecentFiles([]);
  setEditedFiles([]);
  setClosedTabs([]);
  back.length = 0;
  forward.length = 0;
});

async function go(from: Place[], to: Place[]): Promise<void> {
  while (from.length) {
    const target = from.pop()!;
    const current = editor.active && !isCustomTab(editor.active) ? placeOf(editor.active) : null;
    if (current && samePlace(current, target)) continue;
    navigating = true;
    try {
      // Tabs without a text editor (SQL consoles, table editors) have no caret to restore.
      if (editor.tabs.find((t) => t.path === target.path)?.view) await openFile(target.path);
      else await openFileAt(target.path, target.line, target.col, { flash: false, scroll: target.scroll });
    } catch {
      continue;
    } finally {
      navigating = false;
    }
    if (current) to.push(current);
    return;
  }
}

export const navigateBack = () => go(back, forward);
export const navigateForward = () => go(forward, back);

export async function reopenClosedTab(): Promise<void> {
  const [first, ...rest] = closedTabs();
  if (!first) return;
  setClosedTabs(rest);
  await openFile(first);
}

export function forgetRecent(path: string): void {
  setRecentFiles((l) => l.filter((p) => p !== path));
  setEditedFiles((l) => l.filter((p) => p !== path));
}
