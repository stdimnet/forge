/**
 * Browser-mode schema tools: the ER diagram layout and Compare Structure run the real
 * `forge-dbschema` code through WebAssembly; diagram exports are downloaded by the UI (the
 * save command does nothing here). Explain Plan lives in `dbMock.ts` (it needs the console
 * sessions) and uses `demoPlanRows` below to answer like the database would: the rows of
 * the dialect's `EXPLAIN` for the demo tables, which the real parser then turns into a plan.
 */
import { registerMock } from "@/ipc";
import type { Cell, ColumnMeta, Dialect } from "@/ipc/db";
import type { CompareResult, Diagram } from "@/ipc/dbSchema";
import { callWasm, forgeWasm } from "@/wasm";

registerMock("db_diagram", async (a): Promise<Diagram> => {
  const wasm = await forgeWasm();
  return callWasm<Diagram>(wasm.dbschema_diagram, a.request);
});

registerMock("db_schema_compare", async (a): Promise<CompareResult> => {
  const wasm = await forgeWasm();
  return callWasm<CompareResult>(wasm.dbschema_compare, a.request);
});

registerMock("db_save_export", () => undefined);

/** One operation of a demo plan. */
interface DemoOp {
  op: string;
  object?: string;
  alias?: string;
  detail?: string;
  rows: number;
  cost: number;
  children: DemoOp[];
}

export interface PlanTable {
  name: string;
  alias: string;
  rows: number;
}

const round = (n: number) => Math.round(n * 100) / 100;

/** A plausible plan for a query over `tables`: scans joined left-deep, sorted and limited. */
function demoPlan(tables: PlanTable[], sql: string): DemoOp {
  const where = /\bwhere\b/i.test(sql);
  const scan = (t: PlanTable, i: number): DemoOp => {
    const lookup = i > 0 || (where && /\bid\s*=/i.test(sql));
    const rows = lookup ? Math.max(1, Math.round(t.rows / 50)) : where ? Math.max(1, Math.round(t.rows / 3)) : t.rows;
    return {
      op: lookup ? "Index Scan" : "Seq Scan",
      object: t.name,
      alias: t.alias,
      detail: lookup ? `Index Cond: (${t.alias}.id = ${tables[0]?.alias ?? "t"}.${t.name.replace(/s$/, "")}_id)` : where ? "Filter: (status = 'paid')" : undefined,
      rows,
      cost: round(lookup ? 0.29 + rows * 0.02 : t.rows * 0.0125 + 1),
      children: [],
    };
  };
  let node = scan(tables[0], 0);
  for (let i = 1; i < tables.length; i++) {
    const inner = scan(tables[i], i);
    const hash: DemoOp = { op: "Hash", rows: inner.rows, cost: round(inner.cost * 1.1), children: [inner] };
    node = {
      op: "Hash Join",
      detail: `Hash Cond: (${node.alias ?? "o"}.${tables[i].name.replace(/s$/, "")}_id = ${tables[i].alias}.id)`,
      rows: Math.max(node.rows, inner.rows),
      cost: round(node.cost + hash.cost + node.rows * 0.01),
      children: [node, hash],
    };
  }
  if (/\border\s+by\b/i.test(sql)) node = { op: "Sort", detail: "Sort Key: total DESC", rows: node.rows, cost: round(node.cost * 1.35 + 2), children: [node] };
  const limit = /\blimit\s+(\d+)/i.exec(sql);
  if (limit) node = { op: "Limit", rows: Math.min(node.rows, Number(limit[1])), cost: round(node.cost * 1.01), children: [node] };
  return node;
}

const col = (name: string, typeName = "text"): ColumnMeta => ({ name, typeName });

function pgJson(n: DemoOp): unknown {
  return {
    "Node Type": n.op,
    ...(n.object ? { "Relation Name": n.object, Alias: n.alias } : {}),
    "Startup Cost": round(n.cost * 0.1),
    "Total Cost": n.cost,
    "Plan Rows": n.rows,
    "Plan Width": 48,
    ...(n.detail ? { [n.detail.split(": ")[0]]: n.detail.split(": ").slice(1).join(": ") } : {}),
    ...(n.children.length ? { Plans: n.children.map(pgJson) } : {}),
  };
}

function pgText(n: DemoOp, depth = 0, out: string[] = []): string[] {
  const pad = depth ? `${" ".repeat(depth * 6 - 4)}->  ` : "";
  out.push(`${pad}${n.op}${n.object ? ` on ${n.object} ${n.alias}` : ""}  (cost=${round(n.cost * 0.1).toFixed(2)}..${n.cost.toFixed(2)} rows=${n.rows} width=48)`);
  if (n.detail) out.push(`${" ".repeat(depth * 6 + 2)}${n.detail}`);
  for (const c of n.children) pgText(c, depth + 1, out);
  return out;
}

const MYSQL_OPS: Record<string, string> = {
  "Seq Scan": "Table scan",
  "Index Scan": "Single-row index lookup",
  "Hash Join": "Inner hash join",
  Sort: "Sort",
  Limit: "Limit",
};

function mysqlTree(n: DemoOp, depth = 0, out: string[] = []): string[] {
  if (n.op === "Hash") return mysqlTree(n.children[0], depth, out);
  const op = MYSQL_OPS[n.op] ?? n.op;
  const what = n.object ? ` on ${n.alias}${n.op === "Index Scan" ? " using PRIMARY" : ""}` : "";
  out.push(`${" ".repeat(depth * 4)}-> ${op}${what}  (cost=${n.cost} rows=${n.rows})`);
  for (const c of n.children) mysqlTree(c, depth + 1, out);
  return out;
}

function scans(n: DemoOp, out: DemoOp[] = []): DemoOp[] {
  if (n.object) out.push(n);
  for (const c of n.children) scans(c, out);
  return out;
}

/**
 * The rows the dialect's explain statement returns for `tables` (`raw`: Explain Plan
 * (Raw)'s statement).
 */
export function demoPlanRows(dialect: Dialect, raw: boolean, tables: PlanTable[], sql: string): { columns: ColumnMeta[]; rows: Cell[][] } {
  const plan = demoPlan(tables, sql);
  switch (dialect) {
    case "postgres":
      return raw
        ? { columns: [col("QUERY PLAN")], rows: pgText(plan).map((l) => [l]) }
        : { columns: [col("QUERY PLAN", "json")], rows: [[JSON.stringify([{ Plan: pgJson(plan) }], null, 2)]] };
    case "sqlite": {
      let id = 2;
      const rows: Cell[][] = scans(plan).map((s, i) => [
        (id += 3),
        0,
        0,
        i === 0 && s.op === "Seq Scan" ? `SCAN ${s.alias}` : `SEARCH ${s.alias} USING INTEGER PRIMARY KEY (rowid=?)`,
      ]);
      if (/\border\s+by\b/i.test(sql)) rows.push([(id += 3), 0, 0, "USE TEMP B-TREE FOR ORDER BY"]);
      return { columns: [col("id", "integer"), col("parent", "integer"), col("notused", "integer"), col("detail")], rows };
    }
    case "sqlserver": {
      const rows: Cell[][] = [];
      let next = 1;
      const walk = (n: DemoOp, parent: number) => {
        const id = next++;
        const op = n.op === "Seq Scan" ? "Clustered Index Scan" : n.op === "Index Scan" ? "Clustered Index Seek" : n.op === "Hash Join" ? "Hash Match" : n.op === "Limit" ? "Top" : n.op;
        if (n.op !== "Hash")
          rows.push([`  |--${op}`, id, parent, op, op, n.object ? `OBJECT:([shop].[dbo].[${n.object}].[PK_${n.object}] AS [${n.alias}])` : (n.detail ?? null), n.rows, round(n.cost / 100)]);
        for (const c of n.children) walk(c, n.op === "Hash" ? parent : id);
      };
      rows.push([sql.trim(), next++, 0, null, null, null, plan.rows, round(plan.cost / 100)]);
      walk(plan, 1);
      return {
        columns: ["StmtText", "NodeId", "Parent", "PhysicalOp", "LogicalOp", "Argument", "EstimateRows", "TotalSubtreeCost"].map((c) => col(c)),
        rows,
      };
    }
    case "clickhouse": {
      const ch = (n: DemoOp): unknown => ({
        "Node Type": n.object ? "ReadFromMergeTree" : n.op === "Hash Join" ? "Join" : n.op === "Sort" ? "Sorting" : "Expression",
        Description: n.object ?? n.detail ?? "",
        ...(n.children.length ? { Plans: n.children.map(ch) } : {}),
      });
      return raw
        ? { columns: [col("explain")], rows: mysqlTree(plan).map((l) => [l.replace(/\s+\(cost.*$/, "")]) }
        : { columns: [col("explain")], rows: [[JSON.stringify([{ Plan: ch(plan) }])]] };
    }
    default: {
      if (!raw) return { columns: [col("EXPLAIN")], rows: [[mysqlTree(plan).join("\n")]] };
      const names = ["id", "select_type", "table", "partitions", "type", "possible_keys", "key", "key_len", "ref", "rows", "filtered", "Extra"];
      const rows: Cell[][] = scans(plan).map((s, i) => [
        1,
        "SIMPLE",
        s.alias ?? s.object ?? null,
        null,
        s.op === "Seq Scan" ? "ALL" : "eq_ref",
        s.op === "Seq Scan" ? null : "PRIMARY",
        s.op === "Seq Scan" ? null : "PRIMARY",
        s.op === "Seq Scan" ? null : "8",
        i === 0 ? null : `shop.${tables[0].alias}.${s.object?.replace(/s$/, "")}_id`,
        s.rows,
        s.op === "Seq Scan" && /\bwhere\b/i.test(sql) ? 33.33 : 100,
        s.op === "Seq Scan" && /\bwhere\b/i.test(sql) ? "Using where" : null,
      ]);
      return { columns: names.map((n) => col(n)), rows };
    }
  }
}

/** Tables of a query with their aliases (`orders o`, `JOIN users AS u`). */
export function planTables(sql: string, known: (name: string) => number | undefined): PlanTable[] {
  const out: PlanTable[] = [];
  const re = /\b(?:from|join|update|into)\s+[`"[]?(\w+)[`"\]]?(?:\s+(?:as\s+)?(?!where|join|on|order|group|limit|inner|left|right|set|values)(\w+))?/gi;
  for (const m of sql.matchAll(re)) {
    const rows = known(m[1]);
    if (rows === undefined) throw new Error(`Table '${m[1]}' doesn't exist`);
    if (!out.some((t) => t.name === m[1])) out.push({ name: m[1], alias: m[2] ?? m[1], rows });
  }
  if (!out.length) throw new Error("There is no table to explain");
  return out;
}
