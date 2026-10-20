/**
 * A tiny SQL interpreter for the browser mock: single-table SELECT (columns, COUNT(*),
 * WHERE, ORDER BY, LIMIT/OFFSET), UPDATE, DELETE, INSERT, SHOW TABLES and transaction
 * statements over the in-memory demo database. Enough to make consoles and the data editor
 * behave realistically without a server.
 */
import type { Cell, ColumnMeta } from "@/ipc/db";
import { resultTypeName, type DemoDatabase, type DemoTable } from "./dbDemoData";

export interface EngineResult {
  columns: ColumnMeta[];
  rows: Cell[][];
  rowsAffected: number;
}

type Tok = { t: "id" | "num" | "str" | "op" | "kw"; v: string };

const KEYWORDS = new Set(
  "SELECT FROM WHERE AND OR NOT ORDER BY ASC DESC LIMIT OFFSET AS IS NULL LIKE IN BETWEEN TRUE FALSE UPDATE SET DELETE INSERT INTO VALUES SHOW TABLES COUNT DISTINCT".split(" "),
);

export function tokenize(sql: string): Tok[] {
  const out: Tok[] = [];
  let i = 0;
  while (i < sql.length) {
    const c = sql[i];
    if (/\s/.test(c)) {
      i++;
    } else if (c === "-" && sql[i + 1] === "-") {
      while (i < sql.length && sql[i] !== "\n") i++;
    } else if (c === "/" && sql[i + 1] === "*") {
      const e = sql.indexOf("*/", i + 2);
      i = e < 0 ? sql.length : e + 2;
    } else if (c === "'") {
      let j = i + 1;
      let v = "";
      while (j < sql.length) {
        if (sql[j] === "\\") {
          v += sql[j + 1] ?? "";
          j += 2;
        } else if (sql[j] === "'" && sql[j + 1] === "'") {
          v += "'";
          j += 2;
        } else if (sql[j] === "'") break;
        else v += sql[j++];
      }
      out.push({ t: "str", v });
      i = j + 1;
    } else if (c === "`" || c === '"') {
      const e = sql.indexOf(c, i + 1);
      out.push({ t: "id", v: sql.slice(i + 1, e < 0 ? sql.length : e) });
      i = e < 0 ? sql.length : e + 1;
    } else if (/[0-9]/.test(c) || (c === "." && /[0-9]/.test(sql[i + 1] ?? ""))) {
      const m = /^[0-9]*\.?[0-9]+(e[+-]?[0-9]+)?/i.exec(sql.slice(i))!;
      out.push({ t: "num", v: m[0] });
      i += m[0].length;
    } else if (/[A-Za-z_]/.test(c)) {
      const m = /^[A-Za-z_][A-Za-z0-9_$]*/.exec(sql.slice(i))!;
      const up = m[0].toUpperCase();
      out.push(KEYWORDS.has(up) ? { t: "kw", v: up } : { t: "id", v: m[0] });
      i += m[0].length;
    } else {
      const two = sql.slice(i, i + 2);
      if (["<=", ">=", "<>", "!="].includes(two)) {
        out.push({ t: "op", v: two });
        i += 2;
      } else {
        out.push({ t: "op", v: c });
        i++;
      }
    }
  }
  return out;
}

class Parser {
  i = 0;
  constructor(private toks: Tok[]) {}
  peek(o = 0): Tok | undefined {
    return this.toks[this.i + o];
  }
  isKw(v: string, o = 0) {
    const t = this.peek(o);
    return t?.t === "kw" && t.v === v;
  }
  isOp(v: string) {
    const t = this.peek();
    return t?.t === "op" && t.v === v;
  }
  eatKw(v: string) {
    if (!this.isKw(v)) return false;
    this.i++;
    return true;
  }
  eatOp(v: string) {
    if (!this.isOp(v)) return false;
    this.i++;
    return true;
  }
  expectKw(v: string) {
    if (!this.eatKw(v)) throw new Error(`syntax error: expected ${v} near '${this.peek()?.v ?? "end of input"}'`);
  }
  expectOp(v: string) {
    if (!this.eatOp(v)) throw new Error(`syntax error: expected '${v}' near '${this.peek()?.v ?? "end of input"}'`);
  }
  ident(): string {
    const t = this.peek();
    if (t?.t !== "id") throw new Error(`syntax error near '${t?.v ?? "end of input"}'`);
    this.i++;
    return t.v;
  }
  done() {
    this.eatOp(";");
    return this.i >= this.toks.length;
  }
}

type Expr =
  | { k: "col"; name: string }
  | { k: "lit"; v: Cell }
  | { k: "cmp"; op: string; a: Expr; b: Expr }
  | { k: "and" | "or"; a: Expr; b: Expr }
  | { k: "not"; a: Expr }
  | { k: "isnull"; a: Expr; neg: boolean }
  | { k: "like"; a: Expr; b: Expr; neg: boolean }
  | { k: "in"; a: Expr; list: Expr[]; neg: boolean }
  | { k: "between"; a: Expr; lo: Expr; hi: Expr };

function parseExpr(p: Parser): Expr {
  let a = parseAnd(p);
  while (p.eatKw("OR")) a = { k: "or", a, b: parseAnd(p) };
  return a;
}

function parseAnd(p: Parser): Expr {
  let a = parseNot(p);
  while (p.isKw("AND")) {
    p.i++;
    a = { k: "and", a, b: parseNot(p) };
  }
  return a;
}

function parseNot(p: Parser): Expr {
  if (p.eatKw("NOT")) return { k: "not", a: parseNot(p) };
  return parsePredicate(p);
}

function parsePredicate(p: Parser): Expr {
  if (p.eatOp("(")) {
    const e = parseExpr(p);
    p.expectOp(")");
    return e;
  }
  const a = parseOperand(p);
  if (p.eatKw("IS")) {
    const neg = p.eatKw("NOT");
    p.expectKw("NULL");
    return { k: "isnull", a, neg };
  }
  const neg = p.eatKw("NOT");
  if (p.eatKw("LIKE")) return { k: "like", a, b: parseOperand(p), neg };
  if (p.eatKw("IN")) {
    p.expectOp("(");
    const list = [parseOperand(p)];
    while (p.eatOp(",")) list.push(parseOperand(p));
    p.expectOp(")");
    return { k: "in", a, list, neg };
  }
  if (p.eatKw("BETWEEN")) {
    const lo = parseOperand(p);
    p.expectKw("AND");
    return { k: "between", a, lo, hi: parseOperand(p) };
  }
  const t = p.peek();
  if (t?.t === "op" && ["=", "<>", "!=", "<", ">", "<=", ">="].includes(t.v)) {
    p.i++;
    return { k: "cmp", op: t.v, a, b: parseOperand(p) };
  }
  // A bare operand is truthy/falsy (e.g. `WHERE is_active`).
  return { k: "cmp", op: "truthy", a, b: { k: "lit", v: null } };
}

function parseOperand(p: Parser): Expr {
  const t = p.peek();
  if (!t) throw new Error("syntax error: unexpected end of input");
  if (t.t === "op" && t.v === "-" && p.peek(1)?.t === "num") {
    p.i += 2;
    return { k: "lit", v: -Number(p.peek(-1)!.v) };
  }
  p.i++;
  if (t.t === "num") return { k: "lit", v: Number(t.v) };
  if (t.t === "str") return { k: "lit", v: t.v };
  if (t.t === "kw" && t.v === "NULL") return { k: "lit", v: null };
  if (t.t === "kw" && (t.v === "TRUE" || t.v === "FALSE")) return { k: "lit", v: t.v === "TRUE" };
  if (t.t === "id") {
    if (p.isOp("(")) {
      // NOW(), CURRENT_DATE() and friends.
      p.i++;
      p.expectOp(")");
      return { k: "lit", v: "2025-06-02 09:41:00" };
    }
    if (p.eatOp(".")) return { k: "col", name: p.ident() };
    return { k: "col", name: t.v };
  }
  throw new Error(`syntax error near '${t.v}'`);
}

const num = (v: Cell): number | null => {
  if (typeof v === "number") return v;
  if (typeof v === "boolean") return v ? 1 : 0;
  if (typeof v === "string" && v.trim() !== "" && !Number.isNaN(Number(v))) return Number(v);
  return null;
};

function compare(a: Cell, b: Cell): number {
  if (a === null && b === null) return 0;
  if (a === null) return -1;
  if (b === null) return 1;
  const x = num(a);
  const y = num(b);
  if (x !== null && y !== null) return x - y;
  return String(a).localeCompare(String(b));
}

function evalExpr(e: Expr, row: Record<string, Cell>): Cell {
  switch (e.k) {
    case "lit":
      return e.v;
    case "col": {
      if (!(e.name in row)) throw new Error(`Unknown column '${e.name}' in 'where clause'`);
      return row[e.name];
    }
    case "and":
      return !!evalExpr(e.a, row) && !!evalExpr(e.b, row);
    case "or":
      return !!evalExpr(e.a, row) || !!evalExpr(e.b, row);
    case "not":
      return !evalExpr(e.a, row);
    case "isnull":
      return (evalExpr(e.a, row) === null) !== e.neg;
    case "like": {
      const v = evalExpr(e.a, row);
      if (v === null) return false;
      const pat = String(evalExpr(e.b, row))
        .replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
        .replace(/%/g, ".*")
        .replace(/_/g, ".");
      return new RegExp(`^${pat}$`, "i").test(String(v)) !== e.neg;
    }
    case "in": {
      const v = evalExpr(e.a, row);
      return e.list.some((x) => compare(v, evalExpr(x, row)) === 0) !== e.neg;
    }
    case "between": {
      const v = evalExpr(e.a, row);
      return compare(v, evalExpr(e.lo, row)) >= 0 && compare(v, evalExpr(e.hi, row)) <= 0;
    }
    case "cmp": {
      const a = evalExpr(e.a, row);
      if (e.op === "truthy") return !!a && a !== "0";
      const b = evalExpr(e.b, row);
      if (a === null || b === null) return false;
      const c = compare(a, b);
      return e.op === "=" ? c === 0 : e.op === "<>" || e.op === "!=" ? c !== 0 : e.op === "<" ? c < 0 : e.op === ">" ? c > 0 : e.op === "<=" ? c <= 0 : c >= 0;
    }
  }
}

function table(db: DemoDatabase, name: string): DemoTable {
  const t = db.tables[name] ?? Object.values(db.tables).find((x) => x.info.name.toLowerCase() === name.toLowerCase());
  if (!t) throw new Error(`Table 'shop.${name}' doesn't exist`);
  return t;
}

const rowObject = (t: DemoTable, r: Cell[]) => Object.fromEntries(t.info.columns.map((c, i) => [c.name, r[i]]));

/** Skip an optional `schema.` qualifier. */
function tableName(p: Parser): string {
  const first = p.ident();
  return p.eatOp(".") ? p.ident() : first;
}

function parseWhere(p: Parser): Expr | null {
  return p.eatKw("WHERE") ? parseExpr(p) : null;
}

type Item = { kind: "star" } | { kind: "count"; alias: string } | { kind: "expr"; e: Expr; alias: string };

function select(p: Parser, db: DemoDatabase): EngineResult {
  p.eatKw("DISTINCT");
  const items: Item[] = [];
  do {
    if (p.eatOp("*")) items.push({ kind: "star" });
    else if (p.isKw("COUNT")) {
      p.i++;
      p.expectOp("(");
      while (!p.eatOp(")")) p.i++;
      const alias = p.eatKw("AS") ? p.ident() : p.peek()?.t === "id" && !p.isKw("FROM") ? p.ident() : "COUNT(*)";
      items.push({ kind: "count", alias });
    } else {
      const start = p.i;
      const e = parseOperand(p);
      let alias = e.k === "col" ? e.name : String(p.peek(-1)?.v ?? "?");
      if (e.k === "lit" && p.i - start > 1) alias = "NOW()";
      if (p.eatKw("AS")) alias = p.ident();
      else if (p.peek()?.t === "id") alias = p.ident();
      items.push({ kind: "expr", e, alias });
    }
  } while (p.eatOp(","));

  if (!p.eatKw("FROM")) {
    const row = items.map((it) => (it.kind === "expr" ? evalExpr(it.e, {}) : null));
    return {
      columns: items.map((it, i) => ({
        name: it.kind === "star" ? "*" : it.alias,
        typeName: typeof row[i] === "number" ? "BIGINT" : "VARCHAR",
      })),
      rows: [row],
      rowsAffected: 0,
    };
  }
  const t = table(db, tableName(p));
  if (p.peek()?.t === "id") p.ident(); // alias
  if (p.peek()?.t === "id" && /^(join|left|right|inner|cross)$/i.test(p.peek()!.v)) {
    throw new Error("The browser demo understands single-table queries only; connect a real database for joins.");
  }
  const where = parseWhere(p);
  const order: { e: Expr; desc: boolean }[] = [];
  if (p.eatKw("ORDER")) {
    p.expectKw("BY");
    do {
      // `ORDER BY 2` sorts by the second column.
      const e = parseOperand(p);
      const desc = p.eatKw("DESC");
      if (!desc) p.eatKw("ASC");
      order.push({ e, desc });
    } while (p.eatOp(","));
  }
  let limit = Infinity;
  let offset = 0;
  if (p.eatKw("LIMIT")) {
    const a = Number(p.peek()?.v);
    p.i++;
    if (p.eatOp(",")) {
      offset = a;
      limit = Number(p.peek()?.v);
      p.i++;
    } else limit = a;
    if (p.eatKw("OFFSET")) {
      offset = Number(p.peek()?.v);
      p.i++;
    }
  }
  if (!p.done()) throw new Error(`syntax error near '${p.peek()?.v}'`);

  let rows = t.rows.map((r) => ({ r, o: rowObject(t, r) }));
  if (where) rows = rows.filter((x) => !!evalExpr(where, x.o));
  if (order.length) {
    const colIdx = (e: Expr) => (e.k === "lit" && typeof e.v === "number" ? t.info.columns[e.v - 1]?.name : undefined);
    rows.sort((x, y) => {
      for (const o of order) {
        const name = colIdx(o.e);
        const a = name ? x.o[name] : evalExpr(o.e, x.o);
        const b = name ? y.o[name] : evalExpr(o.e, y.o);
        const c = compare(a, b);
        if (c) return o.desc ? -c : c;
      }
      return 0;
    });
  }

  if (items.some((it) => it.kind === "count")) {
    return {
      columns: items.map((it) => ({ name: it.kind === "count" ? it.alias : it.kind === "expr" ? it.alias : "*", typeName: "BIGINT" })),
      rows: [items.map((it) => (it.kind === "count" ? rows.length : it.kind === "expr" ? evalExpr(it.e, rows[0]?.o ?? {}) : null))],
      rowsAffected: 0,
    };
  }
  rows = rows.slice(offset, offset + limit);
  const columns: ColumnMeta[] = [];
  const getters: ((o: Record<string, Cell>) => Cell)[] = [];
  for (const it of items) {
    if (it.kind === "star") {
      for (const c of t.info.columns) {
        columns.push({ name: c.name, typeName: resultTypeName(c.dataType) });
        getters.push((o) => o[c.name]);
      }
    } else if (it.kind === "expr") {
      const c = it.e.k === "col" ? t.info.columns.find((x) => x.name === (it.e as { name: string }).name) : undefined;
      if (it.e.k === "col" && !c) throw new Error(`Unknown column '${it.e.name}' in 'field list'`);
      columns.push({ name: it.alias, typeName: c ? resultTypeName(c.dataType) : "VARCHAR" });
      const e = it.e;
      getters.push((o) => evalExpr(e, o));
    }
  }
  return { columns, rows: rows.map((x) => getters.map((g) => g(x.o))), rowsAffected: 0 };
}

function update(p: Parser, db: DemoDatabase): EngineResult {
  const t = table(db, tableName(p));
  p.expectKw("SET");
  const sets: [number, Expr][] = [];
  do {
    const name = p.ident();
    const idx = t.info.columns.findIndex((c) => c.name === name);
    if (idx < 0) throw new Error(`Unknown column '${name}' in 'field list'`);
    p.expectOp("=");
    sets.push([idx, parseOperand(p)]);
  } while (p.eatOp(","));
  const where = parseWhere(p);
  if (!p.done()) throw new Error(`syntax error near '${p.peek()?.v}'`);
  let n = 0;
  for (const r of t.rows) {
    const o = rowObject(t, r);
    if (where && !evalExpr(where, o)) continue;
    let changed = false;
    for (const [idx, e] of sets) {
      const v = evalExpr(e, o);
      if (compare(r[idx], v) !== 0 || (r[idx] === null) !== (v === null)) changed = true;
      r[idx] = typeof v === "number" && t.info.columns[idx].dataType.startsWith("decimal") ? v.toFixed(2) : v;
    }
    if (changed) n++;
  }
  return { columns: [], rows: [], rowsAffected: n };
}

function del(p: Parser, db: DemoDatabase): EngineResult {
  p.expectKw("FROM");
  const t = table(db, tableName(p));
  const where = parseWhere(p);
  if (!p.done()) throw new Error(`syntax error near '${p.peek()?.v}'`);
  const before = t.rows.length;
  t.rows = t.rows.filter((r) => where && !evalExpr(where, rowObject(t, r)));
  return { columns: [], rows: [], rowsAffected: before - t.rows.length };
}

function insert(p: Parser, db: DemoDatabase): EngineResult {
  p.expectKw("INTO");
  const t = table(db, tableName(p));
  let cols = t.info.columns.map((c) => c.name);
  if (p.eatOp("(")) {
    cols = [];
    if (!p.isOp(")")) {
      do cols.push(p.ident());
      while (p.eatOp(","));
    }
    p.expectOp(")");
  }
  p.expectKw("VALUES");
  let n = 0;
  do {
    p.expectOp("(");
    const vals: Cell[] = [];
    if (!p.isOp(")")) {
      do vals.push(evalExpr(parseOperand(p), {}));
      while (p.eatOp(","));
    }
    p.expectOp(")");
    const row: Cell[] = t.info.columns.map((c) => {
      const i = cols.indexOf(c.name);
      if (i >= 0) return vals[i];
      if (c.autoIncrement) return Math.max(0, ...t.rows.map((r) => Number(r[t.info.columns.indexOf(c)]) || 0)) + 1;
      if (c.name === "created_at" || c.name === "updated_at") return "2025-06-02 09:41:00";
      if (c.default != null) return c.default;
      if (!c.nullable) throw new Error(`Field '${c.name}' doesn't have a default value`);
      return null;
    });
    for (const idx of t.info.indexes.filter((i) => i.unique)) {
      const pos = idx.columns.map((c) => t.info.columns.findIndex((x) => x.name === c));
      if (t.rows.some((r) => pos.every((p2) => compare(r[p2], row[p2]) === 0)))
        throw new Error(`[1062] Duplicate entry '${pos.map((p2) => row[p2]).join("-")}' for key '${t.info.name}.${idx.name}'`);
    }
    t.rows.push(row);
    n++;
  } while (p.eatOp(","));
  return { columns: [], rows: [], rowsAffected: n };
}

/** Run one statement against the demo database (mutating it for DML). */
export function runStatement(db: DemoDatabase, sql: string): EngineResult {
  const p = new Parser(tokenize(sql));
  const first = p.peek();
  if (!first) return { columns: [], rows: [], rowsAffected: 0 };
  if (first.t === "kw") {
    p.i++;
    switch (first.v) {
      case "SELECT":
        return select(p, db);
      case "UPDATE":
        return update(p, db);
      case "DELETE":
        return del(p, db);
      case "INSERT":
        return insert(p, db);
      case "SHOW":
        if (p.eatKw("TABLES"))
          return {
            columns: [{ name: "Tables_in_shop", typeName: "VARCHAR" }],
            rows: Object.keys(db.tables)
              .sort()
              .map((n) => [n]),
            rowsAffected: 0,
          };
    }
  }
  // DDL and everything else: accepted without effect in the demo.
  return { columns: [], rows: [], rowsAffected: 0 };
}

/** Deep copy for transaction snapshots. */
export function cloneDatabase(db: DemoDatabase): DemoDatabase {
  return {
    routines: db.routines,
    tables: Object.fromEntries(Object.entries(db.tables).map(([k, t]) => [k, { info: t.info, rows: t.rows.map((r) => [...r]) }])),
  };
}
