/**
 * SQL highlighting and keywords per dialect: CodeMirror's SQL dialects (ClickHouse defined
 * here), chosen per file from the SQL Dialects settings (Rust resolves the mappings) and
 * switched in open editors when the dialect changes.
 */
import { createStore } from "solid-js/store";
import { Compartment, type Extension } from "@codemirror/state";
import { ViewPlugin, type EditorView } from "@codemirror/view";
import { MSSQL, MySQL, PLSQL, PostgreSQL, SQLDialect, SQLite, StandardSQL, sql } from "@codemirror/lang-sql";
import { sqlApi, type FileSql, type SqlDialect } from "@/ipc/sql";
import type { Dialect } from "@/ipc/db";

/** ClickHouse: MySQL-like quoting with its own clauses and types. */
const ClickHouse = SQLDialect.define({
  ...MySQL.spec,
  keywords: `${MySQL.spec.keywords ?? ""} prewhere final sample settings format global asof any semi anti ttl engine optimize array join`,
  types: `${MySQL.spec.types ?? ""} uint8 uint16 uint32 uint64 int8 int16 int32 int64 float32 float64 string fixedstring datetime64 lowcardinality nullable tuple map uuid ipv4 ipv6`,
  backslashEscapes: true,
  hashComments: true,
  identifierQuotes: '`"',
});

const DIALECTS: Record<SqlDialect, SQLDialect> = {
  generic: StandardSQL,
  mysql: MySQL,
  postgres: PostgreSQL,
  sqlite: SQLite,
  sqlserver: MSSQL,
  oracle: PLSQL,
  clickhouse: ClickHouse,
};

export const cmDialect = (d: SqlDialect | undefined): SQLDialect => DIALECTS[d ?? "generic"] ?? StandardSQL;

/** The SQL dialect of a console's data source (`null`: Redis and MongoDB are not SQL). */
export const consoleDialect = (d: Dialect): SqlDialect | null => (d === "redis" || d === "mongo" ? null : d);

/** Dialect and data source of open SQL files, by path. */
const [fileSql, setFileSql] = createStore<Record<string, FileSql>>({});
export { fileSql };

interface Live {
  path: string;
  dialect: SqlDialect;
  compartment: Compartment;
}

const live = new Map<EditorView, Live>();

const support = (d: SqlDialect) => sql({ dialect: cmDialect(d), upperCaseKeywords: true });

/** Ask Rust for a file's dialect again and switch its open editors. */
export async function refreshFileSql(path: string): Promise<void> {
  try {
    const info = await sqlApi.fileInfo({ text: "", path });
    setFileSql(path, info);
    for (const [view, l] of live) {
      if (l.path === path && l.dialect !== info.dialect) {
        l.dialect = info.dialect;
        view.dispatch({ effects: l.compartment.reconfigure(support(info.dialect)) });
      }
    }
  } catch {
    // Keep the current dialect.
  }
}

/** Settings changed: every open SQL file again. */
export function refreshAllFileSql(): void {
  const paths = new Set([...live.values()].map((l) => l.path));
  for (const p of paths) void refreshFileSql(p);
}

/** The language of a `.sql` file: its dialect, updated when the mappings change. */
export function sqlFileLanguage(path: string): Extension {
  const compartment = new Compartment();
  const initial = fileSql[path]?.dialect ?? "generic";
  return [
    compartment.of(support(initial)),
    ViewPlugin.define((view) => {
      live.set(view, { path, dialect: initial, compartment });
      queueMicrotask(() => void refreshFileSql(path));
      return {
        destroy() {
          live.delete(view);
        },
      };
    }),
  ];
}
