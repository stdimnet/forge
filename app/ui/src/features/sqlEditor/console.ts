/**
 * What the SQL editor adds to query consoles: the inspections with their squiggles, alias
 * rename and live templates in the console's editor, the Parameters dialog before running
 * (values remembered per console) and the query history of every execution.
 */
import type { Extension } from "@codemirror/state";
import type { StatementResult } from "@/ipc/db";
import { sqlApi, type SqlDialect } from "@/ipc/sql";
import { lspDiagnostics } from "@/features/lsp/diagnostics";
import { inPlaceRename } from "@/features/codeintel/rename";
import { liveTemplatesExtension } from "@/features/liveTemplates/editor";
import { sqlInspection } from "./inspector";
import { askParams } from "./SqlDialogs";

/** Editor extensions of a SQL console. */
export function sqlConsoleExtensions(path: string): Extension[] {
  return [sqlInspection(path), lspDiagnostics(path), inPlaceRename(), liveTemplatesExtension(path)];
}

const remembered = new Map<string, Record<string, string>>();

/**
 * `sql` with its user parameters filled in: asks for the values (the console's last ones
 * prefilled); `null` when the user cancelled.
 */
export async function withParameters(consolePath: string, sql: string, dialect: SqlDialect): Promise<string | null> {
  const found = await sqlApi.params(sql, dialect).catch(() => null);
  if (!found?.names.length) return sql;
  const values = await askParams(found.names, remembered.get(consolePath) ?? {}, sql);
  if (!values) return null;
  remembered.set(consolePath, { ...(remembered.get(consolePath) ?? {}), ...values });
  return sqlApi.substitute(sql, dialect, values);
}

/** Add executed statements to the data source's query history. */
export function recordHistory(dataSource: string, console: string, results: StatementResult[]): void {
  const time = Date.now();
  for (const r of results) {
    void sqlApi
      .addHistory({
        sql: r.sql,
        time,
        durationMs: r.elapsedMs,
        rows: r.error ? null : r.returnsRows ? r.rows.length : r.rowsAffected,
        dataSource,
        console,
        error: r.error ?? null,
      })
      .catch(() => undefined);
  }
}
