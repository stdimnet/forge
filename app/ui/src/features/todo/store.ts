/**
 * TODO items of the open project. Scanning runs in Rust (`todo_scan`: `forge-find::todo`
 * through the project's backend, so remote projects are scanned on their host); this store
 * keeps the last answer and refreshes single files when they change on disk or in an editor
 * (unsaved contents are sent as `buffers`).
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { todoApi, type TodoFile, type TodoPattern, type TodoResult } from "@/ipc/workbench";
import { getSetting } from "@/state/settings";
import { workspace } from "@/state/workspace";
import { onFsChanged } from "@/state/fsWatch";
import { activeTab, editor, isDirty, onFileSaved, onPathMoved, onTabClosed, type EditorTab } from "@/state/editor";
import { layout } from "@/state/layout";

/** PHPStorm's default patterns, also the default of the `todo.patterns` setting. */
export const DEFAULT_PATTERNS = ["\\btodo\\b.*", "\\bfixme\\b.*"];

/**
 * `todo.patterns` entries: a regular expression, optionally followed by a tab and `case`
 * (case-sensitive). Empty entries are skipped.
 */
export function parsePatterns(entries: readonly string[]): TodoPattern[] {
  return entries
    .map((e) => {
      const [pattern, flag] = e.split("\t");
      return { pattern: pattern.trim(), caseSensitive: flag?.trim().toLowerCase() === "case" };
    })
    .filter((p) => p.pattern.length > 0);
}

/** A readable name for a pattern (`\btodo\b.*` → `TODO`). */
export function patternLabel(pattern: string): string {
  const word = pattern.replace(/\\b/g, "").replace(/\.\*$/, "").replace(/[\\^$()[\]{}|?+*]/g, "");
  return word ? word.toUpperCase() : pattern;
}

/** Replace the files of `scanned` paths in `base` with the files of `update`. */
export function mergeTodos(base: TodoResult, update: TodoResult, scanned: readonly string[]): TodoResult {
  const drop = new Set(scanned);
  const files: TodoFile[] = [...base.files.filter((f) => !drop.has(f.path)), ...update.files].sort((a, b) =>
    a.path < b.path ? -1 : a.path > b.path ? 1 : 0,
  );
  return { files, count: files.reduce((n, f) => n + f.items.length, 0), truncated: base.truncated || update.truncated, error: update.error };
}

/** Only the items of the patterns not in `hidden` (the Filter menu). */
export function filterTodos(result: TodoResult, hidden: ReadonlySet<number>, path?: string): TodoResult {
  const files = result.files
    .filter((f) => path === undefined || f.path === path)
    .map((f) => ({ path: f.path, items: f.items.filter((i) => !hidden.has(i.pattern)) }))
    .filter((f) => f.items.length > 0);
  return { ...result, files, count: files.reduce((n, f) => n + f.items.length, 0) };
}

export const patterns = (): TodoPattern[] => parsePatterns(getSetting<string[]>("todo.patterns", DEFAULT_PATTERNS) ?? DEFAULT_PATTERNS);

const [result, setResult] = createSignal<TodoResult | null>(null);
const [scanning, setScanning] = createSignal(false);
/** Pattern indexes hidden by the Filter menu. */
const [hiddenPatterns, setHiddenPatterns] = createSignal<ReadonlySet<number>>(new Set());
export { result as todoResult, scanning as todoScanning, hiddenPatterns, setHiddenPatterns };

let generation = 0;

/** Project files in an editor tab (not views, not library files). */
const isFileTab = (t: EditorTab) => !t.view && !t.path.includes("://") && !t.path.startsWith("forge:");

/** Unsaved contents of open file tabs, by path. */
function buffers(paths?: readonly string[]): Record<string, string> {
  const out: Record<string, string> = {};
  for (const t of editor.tabs) {
    if (isFileTab(t) && isDirty(t) && (!paths || paths.includes(t.path))) out[t.path] = t.content;
  }
  return out;
}

/** Scan the whole project (Refresh, first show, other patterns). */
export async function scanProject(): Promise<void> {
  if (!workspace()) return;
  const gen = ++generation;
  setScanning(true);
  try {
    const full = await todoApi.scan({ patterns: patterns() });
    // Open documents with unsaved changes are scanned as they are on screen.
    const dirty = Object.keys(buffers());
    const live = dirty.length ? await todoApi.scan({ patterns: patterns(), paths: dirty, buffers: buffers() }) : null;
    if (gen === generation) setResult(live ? mergeTodos(full, live, dirty) : full);
  } catch (e) {
    if (gen === generation) setResult({ files: [], count: 0, truncated: false, error: String(e instanceof Error ? e.message : e) });
  } finally {
    if (gen === generation) setScanning(false);
  }
}

/** Rescan some files (they changed) and merge them into the last answer. */
export async function rescanPaths(paths: readonly string[], contents?: Record<string, string>): Promise<void> {
  const base = result();
  if (!base || !paths.length) return;
  const gen = generation;
  try {
    const update = await todoApi.scan({ patterns: patterns(), paths: [...paths], buffers: { ...buffers(paths), ...contents } });
    const current = result();
    if (gen === generation && current) setResult(mergeTodos(current, update, paths));
  } catch {
    // The next full scan picks the files up.
  }
}

const todoVisible = () => layout.active.bottom === "todo";

createRoot(() => {
  // A new project forgets the old answer; the window scans when it is shown.
  createEffect(
    on(
      () => workspace()?.root,
      () => {
        generation++;
        setResult(null);
        setScanning(false);
      },
    ),
  );
  createEffect(() => {
    if (todoVisible() && workspace() && !result() && !scanning()) void scanProject();
  });
  // Other patterns: scan again (the filter refers to pattern indexes).
  createEffect(
    on(
      () => JSON.stringify(patterns()),
      () => {
        setHiddenPatterns(new Set<number>());
        if (result()) void scanProject();
      },
      { defer: true },
    ),
  );
  // Typing in the editor: the active file is rescanned with its unsaved contents.
  let timer: ReturnType<typeof setTimeout> | undefined;
  createEffect(
    on(
      () => {
        const t = activeTab();
        return t && isFileTab(t) ? `${t.path}\n${t.content}` : null;
      },
      (key) => {
        clearTimeout(timer);
        const t = activeTab();
        if (!key || !t || !result()) return;
        const { path, content } = t;
        timer = setTimeout(() => void rescanPaths([path], { [path]: content }), 400);
      },
      { defer: true },
    ),
  );
});

onFsChanged((c) => void rescanPaths([...c.changed, ...c.removed]));
onFileSaved((path, content) => void rescanPaths([path], { [path]: content }));
// A tab closed without saving shows the file on disk again.
onTabClosed((tab) => {
  if (isFileTab(tab)) void rescanPaths([tab.path]);
});
onPathMoved((from, to) => void rescanPaths(to ? [from, to] : [from]));
