/**
 * Navigation in SQL files and consoles, answered by Rust (`forge_sqlcode::navigate`):
 * Go to Declaration (Ctrl+B) jumps to an alias or CTE in the script, to the table, view or
 * column in the Database tool window, or for DDL data sources to its CREATE statement; Find
 * Usages lists a column's or alias's usages in the file; Rename (Shift+F6) renames aliases
 * and CTEs in place. Alt+Enter offers the inspections' quick fixes; Reformat Code uses the
 * SQL formatter.
 */
import { ChangeSet } from "@codemirror/state";
import { EditorView } from "@codemirror/view";
import type { ContextAction, UsageRow, UsagesAnswer, UsageTree } from "@/ipc/codeintel";
import { sqlApi, type Occurrence, type SqlFix, type UsagesAnswer as SqlUsages } from "@/ipc/sql";
import { registerDeclarationProvider } from "@/features/lsp/goto";
import { registerFormatProvider } from "@/features/lsp/format";
import { showHint } from "@/features/lsp/hint";
import { toLsp } from "@/features/lsp/util";
import { registerContextActionHandler, registerContextActionProvider } from "@/features/codeintel/intentions";
import { registerUsagesProvider, showUsagesAt } from "@/features/codeintel/usages";
import { NOT_RENAMEABLE, registerRenameProvider, startLocalSession } from "@/features/codeintel/rename";
import { openFileAt } from "@/editor/views";
import { showToolWindow } from "@/state/layout";
import { connect, ensureSchema, revealSource, setNodeOpen, setSelectedNode, tableInfo } from "@/features/database/store";
import { consoleSourceId } from "@/features/database/store";
import { fileSql } from "./language";
import { sqlProblemsAt } from "./inspector";
import { isSqlEditor, sqlTarget, sqlTargetOf } from "./targets";

const message = (e: unknown) => String(e instanceof Error ? e.message : e);

/** The data source a SQL editor resolves against. */
export const editorDataSource = (path: string): string | undefined => consoleSourceId(path) ?? fileSql[path]?.dataSourceId;

/** Select a table, view or column node in the Database tool window. */
export async function revealInDatabase(id: string, schema: string | undefined, table?: string, column?: string): Promise<void> {
  showToolWindow("database");
  if (!(await connect(id))) return;
  revealSource(id);
  let key = `ds:${id}`;
  if (schema) {
    key = `schema:${id}/${schema}`;
    setNodeOpen(key, true);
    ensureSchema(id, schema);
  }
  if (schema && table) {
    const kind = tableInfo(id, schema, table)?.kind ?? "table";
    const folder = kind === "view" ? "views" : kind === "matview" ? "materialized views" : kind === "collection" ? "collections" : "tables";
    setNodeOpen(`folder:${id}/${schema}/${folder}`, true);
    key = `${kind}:${id}/${schema}/${table}`;
    if (column) {
      setNodeOpen(key, true);
      const base = `${id}/${schema}/${table}`;
      setNodeOpen(`group:${base}/columns`, true);
      key = `column:${base}/${column}`;
    }
  }
  setSelectedNode(key);
  for (let i = 0; i < 20; i++) {
    await new Promise((r) => setTimeout(r, 50));
    const el = document.querySelector<HTMLElement>(`.db-tree [data-key="${CSS.escape(key)}"]`);
    if (el) {
      el.scrollIntoView({ block: "center" });
      el.focus();
      return;
    }
  }
}

registerDeclarationProvider(async (path, view, pos) => {
  if (!isSqlEditor(path)) return false;
  const answer = await sqlApi.declaration(sqlTarget(path, view), pos);
  if (!answer) return false;
  const s = answer.symbol;
  if (s.local && !s.column) {
    if (pos >= s.local[0] && pos <= s.local[1]) {
      // On the definition: its usages, like PHPStorm.
      await showUsagesAt(path, view, pos);
      return true;
    }
    view.dispatch({ selection: { anchor: s.local[0], head: s.local[1] }, effects: EditorView.scrollIntoView(s.local[0], { y: "center" }) });
    view.focus();
    return true;
  }
  if (s.local && s.column) {
    // A column of a CTE or subquery: its definition.
    view.dispatch({ selection: { anchor: s.local[0], head: s.local[1] }, effects: EditorView.scrollIntoView(s.local[0], { y: "center" }) });
    return true;
  }
  if (answer.ddl) {
    await openFileAt(answer.ddl.path, answer.ddl.line + 1, answer.ddl.character + 1);
    return true;
  }
  const ds = editorDataSource(path);
  if (!ds) {
    showHint(view, pos, "Attach a data source to navigate to database objects (SQL Dialects settings or the status bar)");
    return true;
  }
  await revealInDatabase(ds, s.schema, s.table, s.column);
  return true;
});

function usagesAnswer(path: string, view: EditorView, answer: SqlUsages): UsagesAnswer {
  const doc = view.state.doc;
  const row = (o: Occurrence): UsageRow => {
    const line = doc.lineAt(o.from);
    const end = doc.lineAt(o.to);
    const lead = line.text.length - line.text.trimStart().length;
    return {
      line: line.number - 1,
      column: o.from - line.from,
      endLine: end.number - 1,
      endColumn: o.to - end.from,
      text: line.text.trimStart(),
      ranges: [[o.from - line.from - lead, Math.min(o.to, line.to) - line.from - lead]],
      kind: "read",
    };
  };
  const rows = answer.occurrences.filter((o) => !o.declaration).map(row);
  const tree: UsageTree = {
    total: rows.length,
    fileCount: rows.length ? 1 : 0,
    groups: rows.length ? [{ kind: "read", label: "Usages in file", count: rows.length, files: [{ path, usages: rows }] }] : [],
  };
  const decl = answer.occurrences.find((o) => o.declaration);
  return {
    title: answer.name,
    source: "index",
    declaration: decl ? { path, range: { start: toLsp(doc, decl.from), end: toLsp(doc, decl.to) } } : null,
    byType: tree,
    byFile: tree,
    element: "symbol",
    scope: `File '${path.split("/").pop()}'`,
  };
}

registerUsagesProvider(async (path, view, pos) => {
  if (!isSqlEditor(path)) return null;
  const answer = await sqlApi.usages(sqlTarget(path, view), pos);
  return answer ? usagesAnswer(path, view, answer) : null;
});

registerRenameProvider(async (path, view) => {
  if (!isSqlEditor(path)) return false;
  const pos = view.state.selection.main.head;
  const doc = view.state.doc;
  let answer;
  try {
    answer = await sqlApi.rename(sqlTarget(path, view), pos);
  } catch (e) {
    showHint(view, pos, message(e));
    return true;
  }
  if (answer.error || !answer.occurrences.length) {
    showHint(view, pos, answer.error ?? NOT_RENAMEABLE);
    return true;
  }
  if (!view.state.doc.eq(doc)) return true;
  const ranges = answer.occurrences.map((o) => ({ from: o.from, to: o.to }));
  startLocalSession(path, view, ChangeSet.empty(doc.length), ranges, {
    suggestions: [],
    hint: `Rename '${answer.name}' and its usages, then press Enter`,
    finish: async (name, revert) => {
      if (!name.trim()) return false;
      revert();
      view.dispatch({ changes: ranges.map((r) => ({ from: r.from, to: r.to, insert: name })), userEvent: "input.rename" });
      return true;
    },
  });
  return true;
});

const APPLY_FIX = "Sql.ApplyFix";

registerContextActionProvider(async (path, view) => {
  if (!isSqlEditor(path)) return [];
  const problems = sqlProblemsAt(view, view.state.selection.main.head);
  return problems.flatMap((p) =>
    (p.fixes ?? []).map(
      (f): ContextAction => ({ title: f.title, kind: "quickFix", source: "sql", isPreferred: p.id !== "SqlWithoutWhere", needsResolve: false, action: APPLY_FIX, data: f }),
    ),
  );
});

/** Apply a quick fix as one undo step. */
export function applyFix(view: EditorView, f: SqlFix): void {
  const len = view.state.doc.length;
  if (f.edits.some((e) => e.to > len)) return;
  view.dispatch({
    changes: f.edits.map((e) => ({ from: e.from, to: e.to, insert: e.text })),
    selection: f.select ? { anchor: f.select[0], head: f.select[1] } : undefined,
    userEvent: "input.quickfix",
    scrollIntoView: true,
  });
  view.focus();
}

registerContextActionHandler(APPLY_FIX, (data, { view }) => applyFix(view, data as SqlFix));

registerFormatProvider(async (path, text) => (isSqlEditor(path) ? sqlApi.format(sqlTargetOf(path, text)) : null));
