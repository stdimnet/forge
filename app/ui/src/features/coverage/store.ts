/**
 * Coverage state of the UI: the session's suites (Run with Coverage runs and imported
 * reports), the suite the editor and the Project view show, Show / Hide, and the rows and
 * line marks of the active suite. Everything is computed in Rust (`forge-testing`,
 * `commands/testing.rs`); this module only caches and notifies.
 */
import { createSignal } from "solid-js";
import { testingApi, type CoverageNode, type CoverageSuite, type CoverageView, type LineMark } from "@/ipc/testing";
import { showToolWindow } from "@/state/layout";

const [suites, setSuites] = createSignal<CoverageSuite[]>([]);
const [view, setView] = createSignal<CoverageView | null>(null);
const [visible, setVisible] = createSignal(true);
/** Bumped whenever what the editor shows changes. */
const [revision, setRevision] = createSignal(0);

export { suites, view, visible, revision };

/** Rows by project path of the shown suite. */
let byPath = new Map<string, CoverageNode>();
const marks = new Map<string, Promise<LineMark[]>>();

type Listener = () => void;
const listeners: Listener[] = [];

/** Called when the shown suite or its visibility changes (editor gutters re-render). */
export function onCoverageChange(l: Listener): void {
  listeners.push(l);
}

function changed() {
  marks.clear();
  setRevision((r) => r + 1);
  listeners.forEach((l) => l());
}

export const activeSuite = () => view()?.suite ?? null;

/** Show a suite (the newest without `id`) in the tool window, the editor and the Project view. */
export async function showSuite(id?: string, reveal = true): Promise<void> {
  const [list, v] = await Promise.all([testingApi.coverageSuites(), testingApi.coverageView(id)]);
  setSuites(list);
  setView(v);
  byPath = new Map((v?.nodes ?? []).map((n) => [n.path, n]));
  setVisible(true);
  changed();
  if (reveal && v) showToolWindow("coverage");
}

/** Show / hide the coverage of the editor and the Project view (the suite stays). */
export function setCoverageVisible(on: boolean): void {
  setVisible(on);
  changed();
}

/** Remove a suite; the next newest one is shown. */
export async function closeSuite(id: string): Promise<void> {
  await testingApi.coverageRemove(id);
  const left = await testingApi.coverageSuites();
  setSuites(left);
  if (view()?.suite.id === id) {
    if (left.length) await showSuite(left[0].id, false);
    else {
      setView(null);
      byPath = new Map();
      changed();
    }
  }
}

/** The line marks of a project file in the shown suite (none while hidden). */
export function marksFor(path: string): Promise<LineMark[]> {
  const v = view();
  if (!v || !visible() || !byPath.has(path)) return Promise.resolve([]);
  let p = marks.get(path);
  if (!p) {
    p = testingApi.coverageFile(path, v.suite.id).catch(() => []);
    marks.set(path, p);
  }
  return p;
}

/** The coverage row of a project path (file or directory) while coverage is shown. */
export function coverageOf(path: string): CoverageNode | undefined {
  revision();
  return visible() ? byPath.get(path) : undefined;
}

/** "62% lines covered" for the Project view. */
export function projectHint(path: string): string | undefined {
  const n = coverageOf(path);
  if (!n || !n.lines.total) return undefined;
  return `${Math.floor((100 * n.lines.covered) / n.lines.total)}% lines covered`;
}
