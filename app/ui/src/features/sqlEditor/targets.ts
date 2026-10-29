/**
 * What the SQL editor features send Rust for an editor: a file (its path; the dialect and
 * data source come from the SQL Dialects mappings) or a console (its data source, dialect
 * and schema).
 */
import type { EditorView } from "@codemirror/view";
import { sourceDialect } from "@/ipc/db";
import type { SqlTarget } from "@/ipc/sql";
import { languageIdFor } from "@/editor/languages";
import { consoleSourceId, defaultSchema, sourceById } from "@/features/database/store";
import { consoleDialect } from "./language";

/** Whether the editor at `path` holds SQL (a `.sql` file or a SQL console). */
export function isSqlEditor(path: string): boolean {
  const ds = consoleSourceId(path);
  if (ds) return consoleDialect(sourceDialect(sourceById(ds))) !== null;
  return languageIdFor(path) === "sql";
}

export const sqlTarget = (path: string, view: EditorView): SqlTarget => sqlTargetOf(path, view.state.doc.toString());

export function sqlTargetOf(path: string, text: string): SqlTarget {
  const ds = consoleSourceId(path);
  if (ds) {
    const dialect = consoleDialect(sourceDialect(sourceById(ds))) ?? undefined;
    return { text, dataSourceId: ds, dialect, defaultSchema: defaultSchema(ds) || undefined };
  }
  return { text, path };
}
