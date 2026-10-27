/** Schema-based SQL completion: introspected tables and columns as a lang-sql namespace, and
 * `JOIN … ON` suggestions from foreign keys (real and virtual). */
import { MSSQL, MySQL, PLSQL, PostgreSQL, SQLite, sql, type SQLNamespace } from "@codemirror/lang-sql";
import { javascript } from "@codemirror/lang-javascript";
import { StreamLanguage } from "@codemirror/language";
import type { Completion } from "@codemirror/autocomplete";
import type { Extension } from "@codemirror/state";
import type { Dialect, SchemaInfo, TableInfo } from "@/ipc/db";
import { joinCompletionSource } from "./joinCompletion";

function columnCompletions(t: TableInfo): Completion[] {
  return t.columns.map((c) => ({
    label: c.name,
    type: t.primaryKey.includes(c.name) ? "constant" : "property",
    detail: c.dataType,
    info: [t.primaryKey.includes(c.name) ? "primary key" : "", c.virtual ? `virtual: ${c.expression}` : c.nullable ? "nullable" : "not null", c.comment ?? ""].filter(Boolean).join(" · "),
  }));
}

function tableNamespace(schema: SchemaInfo): SQLNamespace {
  return Object.fromEntries(
    schema.tables.map((t) => [
      t.name,
      {
        self: {
          label: t.name,
          type: t.kind === "view" || t.kind === "matview" ? "interface" : "class",
          detail: t.kind === "table" ? `table${t.rowEstimate != null ? ` · ${t.rowEstimate} rows` : ""}` : t.kind === "matview" ? "materialized view" : t.kind,
        },
        children: columnCompletions(t),
      },
    ]),
  );
}

/** Namespace with every introspected schema; tables of `defaultSchema` also resolve unqualified. */
export function buildNamespace(schemas: SchemaInfo[]): SQLNamespace {
  return Object.fromEntries(
    schemas.map((s) => [s.name, { self: { label: s.name, type: "namespace", detail: "schema" }, children: tableNamespace(s) }]),
  );
}

const DIALECTS = { mysql: MySQL, postgres: PostgreSQL, sqlite: SQLite, sqlserver: MSSQL, clickhouse: MySQL, oracle: PLSQL };

/** Redis console: the command word of each line as a keyword, strings and comments. */
const redisLanguage = StreamLanguage.define<{ start: boolean }>({
  startState: () => ({ start: true }),
  token(stream, state) {
    if (stream.sol()) state.start = true;
    if (stream.eatSpace()) return null;
    if (stream.match(/^(#|\/\/).*/)) return "comment";
    if (stream.match(/^"(?:[^"\\]|\\.)*"?/) || stream.match(/^'[^']*'?/)) {
      state.start = false;
      return "string";
    }
    if (stream.match(/^-?\d+(\.\d+)?(?=\s|$)/)) {
      state.start = false;
      return "number";
    }
    stream.match(/^\S+/);
    const kw = state.start;
    state.start = false;
    return kw ? "keyword" : null;
  },
});

/** The console language: SQL with completion for the given schemas, Redis commands or the
 * MongoDB shell (JavaScript). */
export function sqlWithSchema(dialect: Dialect, schemas: SchemaInfo[], defaultSchema?: string): Extension {
  if (dialect === "redis") return redisLanguage;
  if (dialect === "mongo") return javascript();
  const support = sql({
    dialect: DIALECTS[dialect],
    schema: buildNamespace(schemas),
    defaultSchema: defaultSchema || undefined,
    upperCaseKeywords: true,
  });
  return [support, support.language.data.of({ autocomplete: joinCompletionSource(() => schemas) })];
}
