/**
 * Inspect Code, Run Inspection by Name and Code Cleanup runs: the request goes to the
 * backend (`inspections_run`), progress arrives as events, and each result becomes a tab of
 * the Inspection Results tool window. Quick fixes and suppressions from the results are
 * applied through the backend; open editors reload the changed files.
 */
import { createSignal } from "solid-js";
import {
  inspectionsApi,
  type CleanupPlan,
  type Fix,
  type InspectProgress,
  type InspectRequest,
  type InspectScope,
  type Report,
  type ReportProblem,
} from "@/ipc/inspections";
import { showBalloon } from "@/keymap";
import { showToolWindow } from "@/state/layout";
import { editor, reloadFile, saveAll } from "@/state/editor";
import { onProjectSwitch } from "@/state/workspace";
import { t as tr } from "@/i18n";

export const RESULTS_WINDOW = "inspectionResults";

export interface ResultsTab {
  id: number;
  title: string;
  report: Report;
  request: Omit<InspectRequest, "runId">;
  /** Problems fixed or suppressed since (index into `report.problems`). */
  resolved: number[];
}

const [tabs, setTabs] = createSignal<ResultsTab[]>([]);
const [activeTab, setActiveTab] = createSignal<number | null>(null);
const [running, setRunning] = createSignal<(InspectProgress & { title: string }) | null>(null);

// The results are about the project that was inspected.
onProjectSwitch(() => {
  setTabs([]);
  setActiveTab(null);
});

export { tabs as resultTabs, activeTab as activeResultsTab, setActiveTab as setActiveResultsTab, running as runningInspection };

let nextRun = Date.now();
let listening = false;

function listenProgress(): void {
  if (listening) return;
  listening = true;
  void inspectionsApi.onProgress((p) => {
    const r = running();
    if (r && r.runId === p.runId) setRunning({ ...p, title: r.title });
  });
}

/** The title PHPStorm gives a run: `Inspection Results: Project Default on Project 'shop'`. */
function titleOf(request: Omit<InspectRequest, "runId">, report: Report): string {
  if (request.only?.length === 1) return `${report.problems[0]?.inspectionName ?? request.only[0]} on ${request.scope.label}`;
  return `${report.profile} on ${request.scope.label}`;
}

async function withProgress<T>(title: string, start: (runId: number) => Promise<T>): Promise<T | null> {
  if (running()) {
    showBalloon(tr("tools.inspections.inspectionsAreRunning"), tr("tools.inspections.waitForTheCurrentRun"));
    return null;
  }
  listenProgress();
  await saveAll().catch(() => undefined);
  const runId = ++nextRun;
  setRunning({ runId, done: 0, total: 0, text: "Collecting files", title });
  try {
    return await start(runId);
  } catch (e) {
    showBalloon(`${title} failed`, e instanceof Error ? e.message : String(e), 6000, undefined, { type: "error" });
    return null;
  } finally {
    setRunning(null);
  }
}

/** Run inspections over a scope and show the results. */
export async function runInspections(request: Omit<InspectRequest, "runId">): Promise<void> {
  const report = await withProgress("Inspecting code", (runId) => inspectionsApi.run({ ...request, runId }));
  if (!report) return;
  if (!report.problems.length) {
    showBalloon(
      report.cancelled ? tr("tools.inspections.inspectionCancelled") : tr("tools.inspections.noSuspiciousCodeFound"),
      tr("tools.inspections.filesProcessedIn", { count: report.files, label: request.scope.label }),
    );
    if (report.cancelled || !report.errors.length) return;
  }
  const id = ++nextRun;
  setTabs((prev) => [...prev.slice(-7), { id, title: titleOf(request, report), report, request, resolved: [] }]);
  setActiveTab(id);
  showToolWindow(RESULTS_WINDOW);
}

/** Code Cleanup: the safe fixes of a scope, for the preview. */
export function cleanupPlan(request: Omit<InspectRequest, "runId">): Promise<CleanupPlan | null> {
  return withProgress("Code Cleanup", (runId) => inspectionsApi.cleanupPlan({ ...request, runId }));
}

export function cancelRun(): void {
  const r = running();
  if (r) void inspectionsApi.cancel(r.runId);
}

export function closeResultsTab(id: number): void {
  setTabs((prev) => prev.filter((t) => t.id !== id));
  if (activeTab() === id) setActiveTab(tabs().at(-1)?.id ?? null);
}

/** Rerun a tab's request (the Rerun button). */
export function rerun(tab: ResultsTab): void {
  closeResultsTab(tab.id);
  void runInspections(tab.request);
}

function markResolved(tabId: number, indexes: number[]): void {
  setTabs((prev) => prev.map((t) => (t.id === tabId ? { ...t, resolved: [...new Set([...t.resolved, ...indexes])] } : t)));
}

async function reloadOpen(paths: string[]): Promise<void> {
  for (const p of paths) if (editor.tabs.some((t) => t.path === p)) await reloadFile(p).catch(() => undefined);
}

/** Apply a quick fix from the results. */
export async function applyFix(tab: ResultsTab, index: number, fix: Fix): Promise<void> {
  const p = tab.report.problems[index];
  await saveAll().catch(() => undefined);
  try {
    const done = await inspectionsApi.applyFix(p.path, p.stamp, fix);
    if (done.skipped.length) {
      showBalloon(tr("tools.inspections.theFileChangedSinceIt"), tr("tools.inspections.rerunTheInspectionToFix", { skipped: done.skipped.join(", ") }), 5000, undefined, { type: "warning" });
      return;
    }
    await reloadOpen(done.written);
    // Other problems of the file have stale offsets now: they need a rerun to be fixed.
    markResolved(tab.id, [index]);
  } catch (e) {
    showBalloon(tr("tools.inspections.couldNotApplyTheFix"), e instanceof Error ? e.message : String(e), 5000, undefined, { type: "error" });
  }
}

/** Apply a suppression (a comment) from the results. */
export async function applySuppression(tab: ResultsTab, index: number, change: { start: number; end: number; text: string }): Promise<void> {
  const p = tab.report.problems[index];
  await applyFix(tab, index, { title: "Suppress", edits: [{ path: p.path, start: change.start, end: change.end, text: change.text }] });
}

/** Write the files of a Code Cleanup preview. */
export async function applyCleanup(plan: CleanupPlan): Promise<void> {
  try {
    const done = await inspectionsApi.cleanupApply(plan.files.map((f) => ({ path: f.path, stamp: f.stamp, after: f.after })));
    await reloadOpen(done.written);
    const fixes = plan.files.filter((f) => done.written.includes(f.path)).reduce((n, f) => n + f.fixes.length, 0);
    showBalloon(
      tr("tools.inspections.codeCleanupFixAppliedIn", { fixes, value: fixes, count: done.written.length, value2: done.written.length }),
      done.skipped.length ? tr("tools.inspections.changedSinceThePreview", { skipped: done.skipped.join(", ") }) : undefined,
    );
  } catch (e) {
    showBalloon(tr("tools.inspections.codeCleanupFailed"), e instanceof Error ? e.message : String(e), 5000, undefined, { type: "error" });
  }
}

/** Export a tab's results (HTML or XML) to a file the user picks. */
export async function exportResults(tab: ResultsTab, format: "html" | "xml"): Promise<void> {
  const file = `inspection-results.${format}`;
  try {
    const { isTauri } = await import("@/ipc");
    if (isTauri()) {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({ defaultPath: file, filters: [{ name: format.toUpperCase(), extensions: [format] }] });
      if (!path) return;
      await inspectionsApi.exportReport(tab.report, format, path);
      showBalloon(tr("tools.inspections.inspectionResultsExported"), path);
      return;
    }
    const text = await inspectionsApi.exportReport(tab.report, format);
    const url = URL.createObjectURL(new Blob([text], { type: format === "html" ? "text/html" : "application/xml" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = file;
    a.click();
    URL.revokeObjectURL(url);
    showBalloon(tr("tools.inspections.inspectionResultsExported"), file);
  } catch (e) {
    showBalloon(tr("common.exportFailed"), e instanceof Error ? e.message : String(e));
  }
}

export type { InspectScope, ReportProblem };
