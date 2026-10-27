/**
 * Language server state shared by the editor extensions, the Problems tool window and
 * the status bar: diagnostics per file and the state of each server.
 */
import { createSignal } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { lspApi, type EditorDiagnostic, type ServerStatus, type Severity } from "@/ipc/lsp";

const [diagnostics, setDiagnostics] = createStore<Record<string, EditorDiagnostic[]>>({});
const [servers, setServers] = createSignal<ServerStatus[]>([]);

export { diagnostics, servers };

type DiagnosticsListener = (path: string, diags: EditorDiagnostic[]) => void;
const listeners = new Set<DiagnosticsListener>();

/** Called by editor views to refresh their squiggles when new diagnostics arrive. */
export function onDiagnostics(cb: DiagnosticsListener): () => void {
  listeners.add(cb);
  return () => listeners.delete(cb);
}

/** Diagnostics per file and source: language servers (`lsp`) and Forge's own inspections. */
const bySource = new Map<string, Map<string, EditorDiagnostic[]>>();

/** Replace the diagnostics `source` reported for `path`; the store keeps all sources merged. */
export function publishDiagnostics(path: string, diags: EditorDiagnostic[], source = "lsp"): void {
  let sources = bySource.get(path);
  if (!sources) bySource.set(path, (sources = new Map()));
  sources.set(source, diags);
  // Always a new array: the store adopts the first array it gets and `reconcile` updates
  // it in place later, which must never change an array kept in `sources`.
  const merged = sources.size === 1 ? [...diags] : [...sources.values()].flat();
  setDiagnostics(path, reconcile(merged));
  listeners.forEach((l) => l(path, merged));
}

export function setServerStatuses(s: ServerStatus[]): void {
  setServers(s);
}

export const severityRank: Record<Severity, number> = { error: 0, warning: 1, weak: 2, grammar: 3, typo: 4, info: 5 };

export function sortDiagnostics(diags: EditorDiagnostic[]): EditorDiagnostic[] {
  return [...diags].sort(
    (a, b) =>
      severityRank[a.severity] - severityRank[b.severity] ||
      a.range.start.line - b.range.start.line ||
      a.range.start.character - b.range.start.character,
  );
}

export interface ProblemCounts {
  error: number;
  warning: number;
  weak: number;
  info: number;
  typo: number;
  grammar: number;
}

export function countBySeverity(diags: readonly EditorDiagnostic[] | undefined): ProblemCounts {
  const c: ProblemCounts = { error: 0, warning: 0, weak: 0, info: 0, typo: 0, grammar: 0 };
  for (const d of diags ?? []) c[d.severity]++;
  return c;
}

/** Files with at least one problem, errors first, then by path. */
export function filesWithProblems(all: Record<string, EditorDiagnostic[]>): string[] {
  return Object.keys(all)
    .filter((p) => (all[p]?.length ?? 0) > 0)
    .sort((a, b) => {
      const ea = countBySeverity(all[a]).error > 0 ? 0 : 1;
      const eb = countBySeverity(all[b]).error > 0 ? 0 : 1;
      return ea - eb || a.localeCompare(b);
    });
}

let started = false;

/** Subscribe to backend events once. */
export function startLspListeners(): void {
  if (started) return;
  started = true;
  void lspApi.onDiagnostics((e) => publishDiagnostics(e.path, e.diagnostics));
  void lspApi.onStatus((s) => setServerStatuses(s));
  lspApi
    .status()
    .then(setServerStatuses)
    .catch(() => setServerStatuses([]));
}
