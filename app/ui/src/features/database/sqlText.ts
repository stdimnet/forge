/**
 * SQL text helpers mirroring `crates/forge-db` (quoting, literals, statement splitting,
 * row change statements and export), used by the console, the data editor's preview and the
 * browser mock.
 */
import type { Cell, Dialect, ExportFormat, RowChange } from "@/ipc/db";

export const isBinary = (v: Cell): v is { $hex: string } => typeof v === "object" && v !== null && "$hex" in v;

export function quoteIdent(d: Dialect, name: string): string {
  return d === "mysql" ? `\`${name.replace(/`/g, "``")}\`` : `"${name.replace(/"/g, '""')}"`;
}

export function quoteStr(d: Dialect, s: string): string {
  const body = d === "mysql" ? s.replace(/\\/g, "\\\\").replace(/'/g, "''") : s.replace(/'/g, "''");
  return `'${body}'`;
}

export function qualified(d: Dialect, schema: string, table: string): string {
  if (!schema || (d === "sqlite" && schema === "main")) return quoteIdent(d, table);
  return `${quoteIdent(d, schema)}.${quoteIdent(d, table)}`;
}

export function sqlLiteral(d: Dialect, v: Cell): string {
  if (v === null) return "NULL";
  if (typeof v === "boolean") return d === "sqlite" ? (v ? "1" : "0") : v ? "TRUE" : "FALSE";
  if (typeof v === "number") return String(v);
  if (isBinary(v)) return d === "postgres" ? `'\\x${v.$hex}'::bytea` : `X'${v.$hex}'`;
  return quoteStr(d, v);
}

/** Plain text of a cell (clipboard, CSV): NULL as empty, binary as `0x…`. */
export function cellText(v: Cell): string {
  if (v === null) return "";
  if (isBinary(v)) return `0x${v.$hex.toUpperCase()}`;
  return String(v);
}

export function changeSql(d: Dialect, schema: string, table: string, pk: string[], c: RowChange): string {
  const target = qualified(d, schema, table);
  const wherePk = () => {
    if (!pk.length) throw new Error(`Table ${table} has no primary key, so its rows cannot be identified.`);
    return pk
      .map((col) => {
        if (!(col in c.key)) throw new Error(`missing primary key value for ${col}`);
        const v = c.key[col];
        return v === null ? `${quoteIdent(d, col)} IS NULL` : `${quoteIdent(d, col)} = ${sqlLiteral(d, v)}`;
      })
      .join(" AND ");
  };
  const cols = Object.keys(c.values).sort();
  if (c.kind === "insert") {
    if (!cols.length) return d === "mysql" ? `INSERT INTO ${target} () VALUES ()` : `INSERT INTO ${target} DEFAULT VALUES`;
    return `INSERT INTO ${target} (${cols.map((k) => quoteIdent(d, k)).join(", ")}) VALUES (${cols
      .map((k) => sqlLiteral(d, c.values[k]))
      .join(", ")})`;
  }
  if (c.kind === "update") {
    const w = wherePk();
    return `UPDATE ${target} SET ${cols.map((k) => `${quoteIdent(d, k)} = ${sqlLiteral(d, c.values[k])}`).join(", ")} WHERE ${w}`;
  }
  return `DELETE FROM ${target} WHERE ${wherePk()}`;
}

export interface Statement {
  text: string;
  start: number;
  end: number;
}

/** Split a script at `;` outside strings, comments and dollar quotes (like the backend). */
export function splitStatements(sql: string, d: Dialect = "mysql"): Statement[] {
  const out: Statement[] = [];
  const mysql = d === "mysql";
  let start = 0;
  let i = 0;
  const push = (from: number, to: number) => {
    const piece = sql.slice(from, to);
    if (!hasCode(piece, mysql)) return;
    const lead = piece.length - piece.trimStart().length;
    const text = piece.trim();
    out.push({ text, start: from + lead, end: from + lead + text.length });
  };
  while (i < sql.length) {
    const c = sql[i];
    if (c === "'" || c === '"' || c === "`") i = skipQuoted(sql, i, mysql);
    else if (c === "-" && sql[i + 1] === "-") i = skipLine(sql, i);
    else if (c === "#" && mysql) i = skipLine(sql, i);
    else if (c === "/" && sql[i + 1] === "*") {
      const e = sql.indexOf("*/", i + 2);
      i = e < 0 ? sql.length : e + 2;
    } else if (c === "$") i = skipDollar(sql, i);
    else if (c === ";") {
      push(start, i);
      i++;
      start = i;
    } else i++;
  }
  push(start, sql.length);
  return out;
}

function skipLine(s: string, i: number): number {
  const e = s.indexOf("\n", i);
  return e < 0 ? s.length : e + 1;
}

function skipQuoted(s: string, i: number, backslash: boolean): number {
  const q = s[i];
  let j = i + 1;
  while (j < s.length) {
    if (backslash && s[j] === "\\" && q !== "`") {
      j += 2;
      continue;
    }
    if (s[j] === q) {
      if (s[j + 1] === q) {
        j += 2;
        continue;
      }
      return j + 1;
    }
    j++;
  }
  return s.length;
}

function skipDollar(s: string, i: number): number {
  const m = /^\$([A-Za-z_][A-Za-z0-9_]*)?\$/.exec(s.slice(i));
  if (!m) return i + 1;
  const end = s.indexOf(m[0], i + m[0].length);
  return end < 0 ? s.length : end + m[0].length;
}

function hasCode(piece: string, mysql: boolean): boolean {
  let i = 0;
  while (i < piece.length) {
    const c = piece[i];
    if (/\s/.test(c)) i++;
    else if (c === "-" && piece[i + 1] === "-") i = skipLine(piece, i);
    else if (c === "#" && mysql) i = skipLine(piece, i);
    else if (c === "/" && piece[i + 1] === "*") {
      const e = piece.indexOf("*/", i + 2);
      i = e < 0 ? piece.length : e + 2;
    } else return true;
  }
  return false;
}

/**
 * What Ctrl+Enter runs: the selection if there is one, otherwise the statement at the caret
 * (or the closest one before it, like DataGrip).
 */
export function statementAt(sql: string, from: number, to: number, d: Dialect = "mysql"): Statement | null {
  if (to > from) {
    const text = sql.slice(from, to);
    if (!text.trim()) return null;
    const lead = text.length - text.trimStart().length;
    return { text: text.trim(), start: from + lead, end: from + lead + text.trim().length };
  }
  const stmts = splitStatements(sql, d);
  if (!stmts.length) return null;
  const inside = stmts.find((s) => from >= s.start && from <= s.end + 1);
  if (inside) return inside;
  const before = [...stmts].reverse().find((s) => s.end <= from);
  return before ?? stmts[0];
}

function csvField(s: string): string {
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

/** CSV/JSON/SQL INSERT export, identical to the backend's `forge_db::export`. */
export function exportRows(format: ExportFormat, d: Dialect, table: string | undefined, columns: string[], rows: Cell[][]): string {
  if (format === "csv") {
    return [columns.map(csvField).join(","), ...rows.map((r) => r.map((v) => csvField(cellText(v))).join(","))].join("\n") + "\n";
  }
  if (format === "json") {
    const objs = rows.map((r) => Object.fromEntries(columns.map((c, i) => [c, isBinary(r[i]) ? cellText(r[i]) : r[i]])));
    return JSON.stringify(objs, null, 2) + "\n";
  }
  const target = quoteIdent(d, table ?? "MY_TABLE");
  const cols = columns.map((c) => quoteIdent(d, c)).join(", ");
  return rows.map((r) => `INSERT INTO ${target} (${cols}) VALUES (${r.map((v) => sqlLiteral(d, v)).join(", ")});\n`).join("");
}

/** Leading keyword of a statement, upper-cased. */
export function leadingKeyword(stmt: string): string {
  const s = stmt.replace(/^(\s|--[^\n]*\n?|#[^\n]*\n?|\/\*[\s\S]*?\*\/|\()+/, "");
  return (/^[A-Za-z]+/.exec(s)?.[0] ?? "").toUpperCase();
}
