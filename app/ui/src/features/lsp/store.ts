/**
 * Language server state shared by the editor extensions, the Problems tool window and
 * the status bar: diagnostics per file and the state of each server.
 */
import { createSignal } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import { lspApi, type EditorDiagnostic, type ServerStatus, type Severity } from "@/ipc/lsp";
import { GUEST_PREFIX } from "@/ipc/projectPath";
import { editor } from "@/state/editor";
import { onProjectSwitch } from "@/state/workspace";

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

/**
 * Decides which diagnostics of a source are shown and how severe they are: the inspection
 * profile (`features/inspections`) maps every source's problems onto inspections, drops
 * disabled and suppressed ones and applies the profile's severities.
 */
export type DiagnosticsFilter = (path: string, source: string, diags: EditorDiagnostic[]) => EditorDiagnostic[];
let filter: DiagnosticsFilter | null = null;

/** Source key of Forge's own PHP inspections (`features/php/meta.ts`). */
export const PHP_INSPECTIONS_SOURCE = "php-inspections";

const overlaps = (a: EditorDiagnostic["range"], b: EditorDiagnostic["range"]) => {
  const before = (x: EditorDiagnostic["range"]["start"], y: EditorDiagnostic["range"]["start"]) =>
    x.line < y.line || (x.line === y.line && x.character < y.character);
  return !before(a.end, b.start) && !before(b.end, a.start);
};

/**
 * Forge's PHP inspections and the PHP language server (php-lsp) both report
 * deprecated symbol uses and unreachable code: show the server's problem once and drop the
 * inspection's twin on the same range. Its quick fixes stay available through Alt+Enter.
 */
export function withoutInspectionTwins(source: string, list: EditorDiagnostic[], lsp: EditorDiagnostic[]): EditorDiagnostic[] {
  if (source !== PHP_INSPECTIONS_SOURCE || !lsp.length) return list;
  return list.filter(
    (d) => !lsp.some((l) => overlaps(l.range, d.range) && ((d.deprecated && l.deprecated) || (d.unnecessary && l.unnecessary && d.code === "PhpUnreachableStatement"))),
  );
}

function show(path: string): void {
  const sources = bySource.get(path);
  if (!sources) return;
  const lsp = sources.get("lsp") ?? [];
  const merged = [...sources.entries()].flatMap(([source, list]) => {
    const shown = withoutInspectionTwins(source, list, lsp);
    return filter && shown.length ? filter(path, source, shown) : shown;
  });
  setDiagnostics(path, reconcile(merged));
  listeners.forEach((l) => l(path, merged));
}

/** Install the filter (or remove it with `null`) and apply it to everything published so far. */
export function setDiagnosticsFilter(f: DiagnosticsFilter | null): void {
  filter = f;
  refilterDiagnostics();
}

/** Apply the filter again (the profile or a file's suppression comments changed). */
export function refilterDiagnostics(path?: string): void {
  if (path !== undefined) show(path);
  else for (const p of bySource.keys()) show(p);
}

/** Replace the diagnostics `source` reported for `path`; the store keeps all sources merged. */
export function publishDiagnostics(path: string, diags: EditorDiagnostic[], source = "lsp"): void {
  let sources = bySource.get(path);
  if (!sources) bySource.set(path, (sources = new Map()));
  sources.set(source, diags);
  show(path);
}

/** Forget every file's problems (another project opened in this window). */
export function clearDiagnostics(): void {
  const paths = [...bySource.keys()];
  bySource.clear();
  setDiagnostics(reconcile({}));
  for (const path of paths) listeners.forEach((l) => l(path, []));
}

onProjectSwitch(clearDiagnostics);

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

/**
 * Where the server's diagnostics for a project file go: a Code With Me guest's `cwm://` tab
 * of that file when one is open (the host's servers name the project file), else the file.
 */
export function diagnosticsTab(path: string): string {
  const shared = GUEST_PREFIX + path;
  return !path.includes("://") && editor.tabs.some((t) => t.path === shared) ? shared : path;
}

let started = false;

/** Subscribe to backend events once. */
export function startLspListeners(): void {
  if (started) return;
  started = true;
  void lspApi.onDiagnostics((e) => publishDiagnostics(diagnosticsTab(e.path), e.diagnostics));
  void lspApi.onStatus((s) => setServerStatuses(s));
  lspApi
    .status()
    .then(setServerStatuses)
    .catch(() => setServerStatuses([]));
}
