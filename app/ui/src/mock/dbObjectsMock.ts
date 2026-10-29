/**
 * Browser-mode database objects: the table dialogs' previews, Drop / Rename, Generate DDL,
 * import previews and statements, Excel export and dump command lines run on the real Rust
 * code (`forge-dbmodel`, `forge-dbtransfer` through WebAssembly). Scripts are checked like
 * the backend does (read-only refused, production confirmed) and then only counted: the
 * demo database is not changed. Imports stream progress events; dumps stream a canned run
 * into the Run tool window. Nothing is written to the project.
 */
import { emitMock, invoke, registerMock } from "@/ipc";
import { dialectOf, type DataSourceInfo, type DbModel, type Guard } from "@/ipc/db";
import {
  IMPORT_EVENT,
  type DdlObject,
  type DdlOptions,
  type DumpRequest,
  type ImportPlan,
  type ImportSource,
  type ImportSummary,
  type ReadOptions,
  type ScriptResult,
} from "@/ipc/dbObjects";
import { callWasm, forgeWasm } from "@/wasm";
import { startMockRun, type Step } from "./runMock";

async function source(id: string): Promise<DataSourceInfo> {
  const list = await invoke<DataSourceInfo[]>("db_data_sources_list");
  const ds = list.find((d) => d.id === id);
  if (!ds) throw new Error(`unknown data source: ${id}`);
  return ds;
}

const WRITES = /^\s*(insert|update|delete|merge|replace|create|alter|drop|truncate|rename|comment|exec|grant|revoke|pragma)\b/i;

/** Statements of a script (by `;` outside quotes; good enough for generated scripts). */
export function statements(sql: string): string[] {
  const out: string[] = [];
  let cur = "";
  let quote = "";
  for (const ch of sql) {
    if (quote) {
      if (ch === quote) quote = "";
    } else if (ch === "'" || ch === '"' || ch === "`") quote = ch;
    else if (ch === ";") {
      if (cur.trim()) out.push(cur.trim());
      cur = "";
      continue;
    }
    cur += ch;
  }
  if (cur.trim()) out.push(cur.trim());
  return out;
}

async function objectsGuard(id: string, sql: string): Promise<Guard> {
  const verdict = await invoke<Guard>("db_guard", { id, sql });
  const ds = await source(id);
  if (verdict.blocked || verdict.confirm || !(ds.production || ds.options.color === "red")) return verdict;
  const writes = statements(sql)
    .map((s) => WRITES.exec(s.replace(/^(--[^\n]*\n|\s)+/, ""))?.[1]?.toUpperCase())
    .filter((w): w is string => !!w);
  if (!writes.length) return verdict;
  const kinds = [...new Set(writes)].sort();
  return { confirm: `${ds.name} is a production data source. The script runs ${writes.length} statement(s) that change it (${kinds.join(", ")}). Run it anyway?` };
}

async function checked(id: string, sql: string, confirmed: boolean): Promise<void> {
  const g = await objectsGuard(id, sql);
  if (g.blocked) throw new Error(g.blocked);
  if (g.confirm && !confirmed) throw new Error(g.confirm);
}

registerMock("db_design_info", async (a) => {
  const wasm = await forgeWasm();
  return callWasm(wasm.dbobjects_design_info, { dialect: a.dialect });
});

registerMock("db_design_load", async (a) => {
  const table = await invoke("db_table_info", { id: a.id, schema: a.schema, table: a.table });
  const wasm = await forgeWasm();
  return callWasm(wasm.dbobjects_design_spec, { schema: a.schema, table });
});

registerMock("db_design_preview", async (a) => {
  const wasm = await forgeWasm();
  return callWasm(wasm.dbobjects_design_preview, { dialect: a.dialect, old: a.old ?? null, table: a.table, options: a.options ?? undefined });
});

registerMock("db_drop_sql", async (a) => {
  const wasm = await forgeWasm();
  return callWasm(wasm.dbobjects_drop_sql, { dialect: a.dialect, requests: a.requests });
});

registerMock("db_rename_sql", async (a) => {
  const wasm = await forgeWasm();
  return callWasm(wasm.dbobjects_rename_sql, { dialect: a.dialect, schema: a.schema, table: a.table, column: a.column ?? null, to: a.to });
});

registerMock("db_objects_guard", (a) => objectsGuard(a.id as string, a.sql as string));

registerMock("db_run_script", async (a): Promise<ScriptResult> => {
  const started = Date.now();
  await checked(a.id as string, a.sql as string, !!a.confirmed);
  const n = statements(a.sql as string).length;
  return { executed: n, total: n, elapsedMs: Date.now() - started + 12 };
});

registerMock("db_generate_ddl", async (a) => {
  const id = a.id as string;
  const model = await invoke<DbModel>("db_model", { id });
  // Make sure the schemas of the selection are loaded.
  for (const s of new Set((a.objects as DdlObject[]).map((o) => o.schema))) {
    if (!model.loaded[s]) model.loaded[s] = await invoke("db_introspect", { id, schema: s });
  }
  const wasm = await forgeWasm();
  return callWasm<string>(wasm.dbobjects_generate_ddl, { model, objects: a.objects, options: (a.options as DdlOptions) ?? undefined, values: {} });
});

/** Browser mode has no file system: files come with their bytes. */
function bytesOf(src: ImportSource): string {
  if (src.data) return src.data;
  throw new Error(`${src.path ?? "the file"} cannot be read in browser mode; choose a file`);
}

registerMock("db_import_preview", async (a) => {
  const wasm = await forgeWasm();
  return callWasm(wasm.dbobjects_import_preview, { data: bytesOf(a.source as ImportSource), options: a.options as ReadOptions, rows: a.rows ?? 50, dialect: a.dialect ?? null });
});

const cancelled = new Set<string>();

registerMock("db_import_cancel", (a) => {
  cancelled.add(a.importId as string);
});

registerMock("db_import_run", async (a): Promise<ImportSummary> => {
  const id = a.id as string;
  const importId = a.importId as string;
  const plan = a.plan as ImportPlan;
  const ds = await source(id);
  const dialect = dialectOf(ds.driver, ds.options.ddlDialect);
  const wasm = await forgeWasm();
  const out = await callWasm<{ create: string[]; batches: { sql: string; first: number; rows: number }[]; rows: number }>(wasm.dbobjects_import_batches, {
    dialect,
    data: bytesOf(a.source as ImportSource),
    options: a.options,
    plan,
  });
  await checked(id, [...out.create, `INSERT INTO ${plan.table} VALUES (1)`].join(";\n"), !!a.confirmed);
  const started = Date.now();
  const total = plan.limit > 0 ? Math.min(plan.limit, out.rows) : out.rows;
  const summary: ImportSummary = { inserted: 0, skipped: 0, errors: [], cancelled: false, aborted: false, created: out.create.length > 0, elapsedMs: 0 };
  let done = 0;
  for (const b of out.batches) {
    if (cancelled.delete(importId)) {
      summary.cancelled = true;
      break;
    }
    await new Promise((r) => setTimeout(r, 120));
    done += b.rows;
    summary.inserted += b.rows;
    emitMock(IMPORT_EVENT, { importId, done, total, inserted: summary.inserted, skipped: 0 });
  }
  cancelled.delete(importId);
  summary.elapsedMs = Date.now() - started;
  return summary;
});

registerMock("db_export_xlsx", async (a) => {
  const wasm = await forgeWasm();
  return callWasm<string>(wasm.dbobjects_export_xlsx, { sheet: a.sheet, columns: a.columns, rows: a.rows });
});

registerMock("db_dump_tools", async (a) => {
  const ds = await source(a.id as string);
  const wasm = await forgeWasm();
  return callWasm(wasm.dbobjects_dump_tools, { driver: ds.driver });
});

function target(ds: DataSourceInfo) {
  const sqlite = ds.driver === "sqlite";
  return {
    driver: ds.driver,
    host: ds.host,
    port: sqlite ? 0 : (ds.port ?? { mysql: 3306, mariadb: 3306, postgres: 5432, sqlserver: 1433 }[ds.driver as string] ?? 0),
    user: ds.user,
    database: ds.database,
    file: sqlite ? ds.file : "",
    ssl: !sqlite && ds.options.sslMode !== "disable" && ds.options.sslMode !== "prefer",
  };
}

async function dumpCommand(id: string, request: DumpRequest): Promise<{ display: string; program: string; args: string[] }> {
  const ds = await source(id);
  const wasm = await forgeWasm();
  return callWasm(wasm.dbobjects_dump_command, { request, target: target(ds) });
}

registerMock("db_dump_command", async (a) => (await dumpCommand(a.id as string, a.request as DumpRequest)).display);

async function restoreGuard(id: string): Promise<string | null> {
  const ds = await source(id);
  if (ds.options.readOnly) throw new Error(`${ds.name} is read-only: restoring a dump is not allowed. Turn off Read-only in the data source properties first.`);
  return ds.production || ds.options.color === "red" ? `${ds.name} is a production data source. Restoring a dump overwrites its objects and data. Restore anyway?` : null;
}

registerMock("db_restore_guard", (a) => restoreGuard(a.id as string));

registerMock("db_dump_start", async (a) => {
  const id = a.id as string;
  const request = a.request as DumpRequest;
  const ds = await source(id);
  if (request.action === "restore") {
    const warning = await restoreGuard(id);
    if (warning && !a.confirmed) throw new Error(warning);
  }
  const cmd = await dumpCommand(id, request);
  const db = ds.database || ds.file;
  const steps: Step[] =
    request.action === "dump"
      ? [
          [150, `-- Connecting to ${db}\n`],
          [250, `-- Dumping structure${request.data ? " and data" : ""} of ${request.tables.length ? request.tables.join(", ") : db}\n`],
          [300, `-- Wrote ${request.file}\n`],
          [100, { code: 0 }],
        ]
      : [
          [150, `-- Reading ${request.file}\n`],
          [300, `-- Restored into ${db}\n`],
          [100, { code: 0 }],
        ];
  return startMockRun(a.runId as string, { program: cmd.program, args: cmd.args, cwd: "", env: {} }, steps);
});
