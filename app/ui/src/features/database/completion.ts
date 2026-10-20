/** Schema-based SQL completion: introspected tables and columns as a lang-sql namespace. */
import { MySQL, PostgreSQL, SQLite, sql, type SQLNamespace } from "@codemirror/lang-sql";
import type { Completion } from "@codemirror/autocomplete";
import type { Extension } from "@codemirror/state";
import type { Dialect, SchemaInfo, TableInfo } from "@/ipc/db";

function columnCompletions(t: TableInfo): Completion[] {
  return t.columns.map((c) => ({
    label: c.name,
    type: t.primaryKey.includes(c.name) ? "constant" : "property",
    detail: c.dataType,
    info: [t.primaryKey.includes(c.name) ? "primary key" : "", c.nullable ? "nullable" : "not null", c.comment ?? ""].filter(Boolean).join(" · "),
  }));
}

function tableNamespace(schema: SchemaInfo): SQLNamespace {
  return Object.fromEntries(
    schema.tables.map((t) => [
      t.name,
      {
        self: { label: t.name, type: t.kind === "view" ? "interface" : "class", detail: t.kind === "view" ? "view" : `table${t.rowEstimate != null ? ` · ${t.rowEstimate} rows` : ""}` },
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

const DIALECTS = { mysql: MySQL, postgres: PostgreSQL, sqlite: SQLite };

/** The SQL language with completion for the given schemas. */
export function sqlWithSchema(dialect: Dialect, schemas: SchemaInfo[], defaultSchema?: string): Extension {
  return sql({
    dialect: DIALECTS[dialect],
    schema: buildNamespace(schemas),
    defaultSchema: defaultSchema || undefined,
    upperCaseKeywords: true,
  });
}
