/**
 * Join completion of the SQL console (DataGrip's "JOIN … ON" suggestions): after `JOIN`,
 * tables related to the ones already in the statement by a foreign key (real or virtual)
 * are offered with the whole `ON` condition.
 */
import type { CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import type { SchemaInfo, TableInfo } from "@/ipc/db";

export interface JoinSuggestion {
  /** `users u ON u.id = o.user_id` */
  apply: string;
  /** `orders_user_id_foreign` (· virtual) */
  detail: string;
  virtual: boolean;
}

interface Ref {
  table: TableInfo;
  schema: string;
  alias: string;
}

const IDENT = String.raw`[\w$]+|"[^"]+"|` + "`[^`]+`" + String.raw`|\[[^\]]+\]`;
const unquote = (s: string) => s.replace(/^["`[]|["`\]]$/g, "");
const KEYWORDS = new Set(["on", "where", "join", "left", "right", "inner", "outer", "cross", "full", "group", "order", "limit", "using", "natural", "set", "union"]);

/** `order_items` → `oi`, avoiding aliases already used. */
export function aliasFor(table: string, used: Set<string>): string {
  const base =
    table
      .split(/[_\s]+/)
      .filter(Boolean)
      .map((w) => w[0]!.toLowerCase())
      .join("") || "t";
  let alias = base;
  for (let n = 1; used.has(alias); n++) alias = `${base}${n}`;
  return alias;
}

function find(schemas: SchemaInfo[], name: string, schema?: string): { table: TableInfo; schema: string } | undefined {
  for (const s of schemas) {
    if (schema && s.name.toLowerCase() !== schema.toLowerCase()) continue;
    const t = s.tables.find((x) => x.name.toLowerCase() === name.toLowerCase());
    if (t) return { table: t, schema: s.name };
  }
  return undefined;
}

/** Tables named after FROM/JOIN in `sql`, with their aliases. */
export function referencedTables(schemas: SchemaInfo[], sql: string): Ref[] {
  const re = new RegExp(String.raw`\b(?:from|join)\s+((?:${IDENT})(?:\s*\.\s*(?:${IDENT}))?)(?:\s+(?:as\s+)?(${IDENT}))?`, "gi");
  const out: Ref[] = [];
  for (const m of sql.matchAll(re)) {
    const parts = m[1]!.split(/\s*\.\s*/).map(unquote);
    const name = parts[parts.length - 1]!;
    const found = find(schemas, name, parts.length > 1 ? parts[0] : undefined);
    if (!found) continue;
    const alias = m[2] && !KEYWORDS.has(m[2].toLowerCase()) ? unquote(m[2]) : name;
    out.push({ ...found, alias });
  }
  return out;
}

/** Join suggestions for a statement whose text before the caret is `before`. */
export function joinSuggestions(schemas: SchemaInfo[], before: string): JoinSuggestion[] {
  const refs = referencedTables(schemas, before);
  const used = new Set(refs.map((r) => r.alias.toLowerCase()));
  const out: JoinSuggestion[] = [];
  const seen = new Set<string>();
  const add = (target: { table: TableInfo; schema: string }, on: (alias: string) => string, fk: { name: string; virtual?: boolean }, home: string) => {
    const alias = aliasFor(target.table.name, used);
    const qualified = target.schema === home ? target.table.name : `${target.schema}.${target.table.name}`;
    const apply = `${qualified} ${alias} ON ${on(alias)}`;
    if (seen.has(apply)) return;
    seen.add(apply);
    out.push({ apply, detail: `${fk.name}${fk.virtual ? " · virtual" : ""}`, virtual: !!fk.virtual });
  };
  for (const r of refs) {
    // r → referenced tables.
    for (const fk of r.table.foreignKeys) {
      const target = find(schemas, fk.refTable, fk.refSchema || r.schema);
      if (!target) continue;
      add(target, (a) => fk.columns.map((c, i) => `${a}.${fk.refColumns[i]} = ${r.alias}.${c}`).join(" AND "), fk, r.schema);
    }
    // Tables referencing r.
    for (const s of schemas)
      for (const t of s.tables)
        for (const fk of t.foreignKeys) {
          if (fk.refTable.toLowerCase() !== r.table.name.toLowerCase() || (fk.refSchema && fk.refSchema !== r.schema)) continue;
          add({ table: t, schema: s.name }, (a) => fk.columns.map((c, i) => `${a}.${c} = ${r.alias}.${fk.refColumns[i]}`).join(" AND "), fk, r.schema);
        }
  }
  return out;
}

/** CodeMirror completion source: right after `JOIN` (and while typing the table name). */
export function joinCompletionSource(schemas: () => SchemaInfo[]) {
  return (ctx: CompletionContext): CompletionResult | null => {
    const text = ctx.state.sliceDoc(Math.max(0, ctx.pos - 4000), ctx.pos);
    const m = /\bjoin\s+([\w$]*)$/i.exec(text);
    if (!m) return null;
    const statement = text.slice(text.lastIndexOf(";") + 1);
    const list = joinSuggestions(schemas(), statement);
    if (!list.length) return null;
    return {
      from: ctx.pos - m[1]!.length,
      options: list.map((s) => ({ label: s.apply, detail: s.detail, type: "keyword", boost: 99 })),
      validFor: /^[\w$]*$/,
    };
  };
}
