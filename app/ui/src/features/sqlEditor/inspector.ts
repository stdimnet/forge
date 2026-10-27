/**
 * The SQL inspections of an editor (file or console): a moment after typing stops the text
 * goes to Rust (`sql_analyze`: resolve against the data source model, inspections), and the
 * problems are published as source `SQL` to the shared diagnostics store, which draws them,
 * applies the inspection profile and fills the Problems tool window. Problems stay mapped
 * through edits until the next answer, for the Alt+Enter quick fixes.
 */
import type { Extension } from "@codemirror/state";
import { ViewPlugin, type EditorView, type PluginValue, type ViewUpdate } from "@codemirror/view";
import type { EditorDiagnostic } from "@/ipc/lsp";
import { sqlApi, type SqlProblem } from "@/ipc/sql";
import { publishDiagnostics } from "@/features/lsp/store";
import { toLsp } from "@/features/lsp/util";
import { sqlTarget } from "./targets";

export const SQL_SOURCE = "SQL";
export const EDIT_DELAY_MS = 350;

const inspectors = new Set<SqlInspector>();
const byView = new WeakMap<EditorView, SqlInspector>();

class SqlInspector implements PluginValue {
  problems: SqlProblem[] = [];
  private timer: ReturnType<typeof setTimeout> | undefined;
  private seq = 0;
  private destroyed = false;

  constructor(
    readonly view: EditorView,
    readonly path: string,
  ) {
    inspectors.add(this);
    byView.set(view, this);
    this.schedule(30);
  }

  update(u: ViewUpdate): void {
    if (!u.docChanged) return;
    const map = (pos: number, assoc: number) => u.changes.mapPos(pos, assoc);
    this.problems = this.problems
      .map((p) => ({
        ...p,
        from: map(p.from, 1),
        to: map(p.to, -1),
        fixes: p.fixes?.map((f) => ({
          ...f,
          edits: f.edits.map((e) => ({ ...e, from: map(e.from, -1), to: map(e.to, 1) })),
          select: f.select && ([map(f.select[0], -1), map(f.select[1], 1)] as [number, number]),
        })),
      }))
      .filter((p) => p.to >= p.from);
    this.schedule(EDIT_DELAY_MS);
  }

  schedule(delay: number): void {
    clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.run(), delay);
  }

  private async run(): Promise<void> {
    if (this.destroyed) return;
    const seq = ++this.seq;
    const doc = this.view.state.doc;
    try {
      const answer = await sqlApi.analyze(sqlTarget(this.path, this.view));
      if (this.destroyed || seq !== this.seq || !this.view.state.doc.eq(doc)) return;
      this.problems = answer.problems;
      this.publish();
    } catch {
      // The next edit tries again.
    }
  }

  problemsAt(pos: number): SqlProblem[] {
    return this.problems.filter((p) => p.from <= pos && pos <= p.to);
  }

  publish(): void {
    publishDiagnostics(this.path, toDiagnostics(this.view, this.problems), SQL_SOURCE);
  }

  destroy(): void {
    this.destroyed = true;
    clearTimeout(this.timer);
    inspectors.delete(this);
    if (![...inspectors].some((i) => i.path === this.path)) publishDiagnostics(this.path, [], SQL_SOURCE);
  }
}

function toDiagnostics(view: EditorView, problems: SqlProblem[]): EditorDiagnostic[] {
  const doc = view.state.doc;
  return problems.map((p) => ({
    range: { start: toLsp(doc, Math.min(p.from, doc.length)), end: toLsp(doc, Math.min(p.to, doc.length)) },
    severity: p.severity,
    message: p.message,
    source: SQL_SOURCE,
    code: p.id,
    unnecessary: p.unnecessary || undefined,
    data: p,
  }));
}

/** The SQL inspections for the editor at `path`. */
export function sqlInspection(path: string): Extension {
  return ViewPlugin.define((view) => new SqlInspector(view, path));
}

/** Problems at a position (Alt+Enter). */
export const sqlProblemsAt = (view: EditorView, pos: number): SqlProblem[] => byView.get(view)?.problemsAt(pos) ?? [];

/** Analyze again (the data source model or the settings changed); all editors without `path`. */
export function reanalyze(path?: string): void {
  for (const i of inspectors) if (!path || i.path === path) i.schedule(30);
}
