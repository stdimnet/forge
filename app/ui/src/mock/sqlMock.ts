/**
 * Browser-mode SQL editor: the real `forge-sqlcode` through WebAssembly (dialects per file,
 * inspections, navigation, formatter, parameters, history search), with the demo data
 * sources' models from the database mock. The query history is kept in memory and Run SQL
 * Script runs the demo project's `.sql` files on the mock engine; nothing is written to the
 * project.
 */
import { emitMock, invoke, registerMock } from "@/ipc";
import { callWasm, forgeWasm } from "@/wasm";
import type { DataSourceInfo, DbModel, SessionInfo, StatementResult } from "@/ipc/db";
import { RUN_FILE_EVENT, type HistoryEntry, type RunFileEvent, type RunFileRequest, type RunFileSummary, type SqlDialect, type SqlTarget } from "@/ipc/sql";
import { mockSettingLayers, mockSettingValue } from "./settingsMock";
import { ddlFiles } from "./dbMock";

const DB_DIALECT: Record<string, SqlDialect> = {
  mysql: "mysql",
  mariadb: "mysql",
  postgres: "postgres",
  sqlite: "sqlite",
  sqlserver: "sqlserver",
  clickhouse: "clickhouse",
  oracle: "oracle",
};

async function source(id: string | undefined): Promise<DataSourceInfo | undefined> {
  if (!id) return undefined;
  const list = await invoke<DataSourceInfo[]>("db_data_sources_list");
  return list.find((s) => s.id === id);
}

const dialectOfSource = (ds: DataSourceInfo): SqlDialect =>
  ds.driver === "ddl" ? (DB_DIALECT[ds.options.ddlDialect ?? "postgres"] ?? "postgres") : (DB_DIALECT[ds.driver] ?? "generic");

/** The data source model the editor resolves against: loaded schemas only (no connecting
 * beyond what the console already did). */
async function modelOf(id: string | undefined): Promise<DbModel | null> {
  const ds = await source(id);
  if (!ds || ds.driver === "redis" || ds.driver === "mongodb") return null;
  try {
    const model = await invoke<DbModel>("db_model", { id: ds.id });
    if (!Object.keys(model.loaded ?? {}).length) {
      const current = model.schemas.find((s) => s.current && s.visible !== false) ?? model.schemas.find((s) => s.visible !== false);
      if (current) model.loaded = { [current.name]: await invoke("db_introspect", { id: ds.id, schema: current.name }) };
    }
    return model;
  } catch {
    return null;
  }
}

async function fileInfo(t: SqlTarget): Promise<{ dialect: SqlDialect; source: string; dataSourceId?: string }> {
  if (t.dialect) return { dialect: t.dialect, source: "dataSource", dataSourceId: t.dataSourceId };
  if (!t.path && t.dataSourceId) {
    const ds = await source(t.dataSourceId);
    if (ds) return { dialect: dialectOfSource(ds), source: "dataSource", dataSourceId: ds.id };
  }
  const info = await callWasm<{ dialect: SqlDialect; source: string; dataSourceId?: string | null }>((await forgeWasm()).sql_file_info, {
    path: t.path ?? "",
    project: mockSettingValue("sql.dialect.project") ?? "",
    global: mockSettingValue("sql.dialect.default") ?? "generic",
    mappings: mockSettingValue("sql.dialect.mappings") ?? [],
  });
  return { ...info, dataSourceId: t.dataSourceId || info.dataSourceId || undefined };
}

async function context(t: SqlTarget) {
  const info = await fileInfo(t);
  return { info, ctx: { text: t.text, dialect: info.dialect, model: await modelOf(info.dataSourceId), defaultSchema: t.defaultSchema } };
}

registerMock("sql_dialects", async () => callWasm((await forgeWasm()).sql_dialects, {}));
registerMock("sql_file_info", async (a) => fileInfo(a.request as SqlTarget));
registerMock("sql_analyze", async (a) => callWasm((await forgeWasm()).sql_analyze, (await context(a.request as SqlTarget)).ctx));

registerMock("sql_declaration", async (a) => {
  const r = a.request as SqlTarget & { offset: number };
  const { info, ctx } = await context(r);
  const ds = await source(info.dataSourceId);
  const files = ds?.driver === "ddl" ? await ddlFiles(ds.options.ddlFiles ?? []) : [];
  return callWasm((await forgeWasm()).sql_declaration, { ...ctx, offset: r.offset, ddlFiles: files });
});

registerMock("sql_usages", async (a) => {
  const r = a.request as SqlTarget & { offset: number };
  return callWasm((await forgeWasm()).sql_usages, { ...(await context(r)).ctx, offset: r.offset });
});

registerMock("sql_rename", async (a) => {
  const r = a.request as SqlTarget & { offset: number };
  return callWasm((await forgeWasm()).sql_rename, { ...(await context(r)).ctx, offset: r.offset });
});

registerMock("sql_format", async (a) => {
  const r = a.request as SqlTarget;
  const wasm = await forgeWasm();
  const values = await callWasm(wasm.codestyle_selected_values, mockSettingLayers());
  return callWasm(wasm.sql_format, { text: r.text, dialect: (await fileInfo(r)).dialect, values });
});

const patterns = () => {
  const on = (k: string) => mockSettingValue(`database.parameters.${k}`) !== false;
  return { question: on("question"), colon: on("colon"), dollar: on("dollar"), dollarBrace: on("dollarBrace"), hashBrace: on("hashBrace") };
};

registerMock("sql_params", async (a) => {
  const r = a.request as { text: string; dialect: SqlDialect };
  return callWasm((await forgeWasm()).sql_params, { ...r, patterns: patterns() });
});

registerMock("sql_substitute", async (a) => {
  const r = a.request as { text: string; dialect: SqlDialect; values: Record<string, string> };
  return callWasm((await forgeWasm()).sql_substitute, { ...r, patterns: patterns() });
});

// ---- query history (in memory) -------------------------------------------------------------

const histories = new Map<string, { entries: HistoryEntry[] }>();

async function addHistory(entry: HistoryEntry): Promise<void> {
  const history = histories.get(entry.dataSource) ?? { entries: [] };
  const limit = Number(mockSettingValue("database.history.limit") ?? 1000);
  histories.set(entry.dataSource, await callWasm((await forgeWasm()).sql_history_add, { history, entry, limit }));
}

/** Seed the demo history so the popup has something to show. */
export function seedHistory(dataSource: string, entries: HistoryEntry[]): void {
  histories.set(dataSource, { entries: [...entries].sort((a, b) => b.time - a.time) });
}

registerMock("sql_history_list", async (a) =>
  callWasm((await forgeWasm()).sql_history_search, { history: histories.get(a.dataSourceId as string) ?? { entries: [] }, query: a.query ?? {} }),
);
registerMock("sql_history_add", async (a) => addHistory(a.entry as HistoryEntry));
registerMock("sql_history_clear", (a) => {
  histories.delete(a.dataSourceId as string);
});

// ---- Run SQL Script ----------------------------------------------------------------------

const cancelled = new Set<string>();
const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

registerMock("sql_run_file", async (a): Promise<RunFileSummary> => {
  const r = a.request as RunFileRequest;
  const text = await invoke<string>("fs_read_file", { path: r.path });
  const ds = await source(r.dataSourceId);
  if (!ds) throw new Error(`unknown data source ${r.dataSourceId}`);
  if (ds.driver === "ddl") throw new Error("DDL data sources cannot run SQL");
  const guard = await invoke<{ blocked?: string; confirm?: string }>("db_guard", { id: ds.id, sql: text });
  if (guard.blocked) throw new Error(guard.blocked);
  if (guard.confirm && !r.confirmed) return { total: 0, executed: 0, failed: 0, cancelled: false, elapsedMs: 0, needsConfirmation: guard.confirm };
  const plan = await callWasm<{ index: number; text: string; line: number }[]>((await forgeWasm()).sql_plan_run, { text, dialect: dialectOfSource(ds) });
  const total = plan.length;
  const emit = (e: Partial<RunFileEvent> & Pick<RunFileEvent, "kind">) =>
    emitMock(RUN_FILE_EVENT, { runId: r.runId, index: 0, total, line: 0, ok: true, elapsedMs: 0, ...e });
  emit({ kind: "started" });
  const session = await invoke<SessionInfo>("db_session_open", { id: ds.id, shared: false });
  const summary: RunFileSummary = { total, executed: 0, failed: 0, cancelled: false, elapsedMs: 0 };
  const started = Date.now();
  const console = r.path.split("/").pop() ?? r.path;
  for (const st of plan) {
    if (cancelled.delete(r.runId)) {
      summary.cancelled = true;
      break;
    }
    await delay(120);
    let ok = true;
    let error: string | undefined;
    let rows: number | undefined;
    let elapsedMs = 0;
    try {
      const res = await invoke<StatementResult[]>("db_execute", { sessionId: session.id, sql: st.text, pageSize: 100, confirmed: true });
      const failed = res.find((x) => x.error);
      ok = !failed;
      error = failed?.error;
      const last = res[res.length - 1];
      rows = last ? (last.returnsRows ? last.rows.length : last.rowsAffected) : undefined;
      elapsedMs = res.reduce((n, x) => n + x.elapsedMs, 0);
    } catch (e) {
      ok = false;
      error = e instanceof Error ? e.message : String(e);
    }
    summary.executed++;
    if (!ok) summary.failed++;
    await addHistory({ sql: st.text, time: Date.now(), durationMs: elapsedMs, rows, dataSource: ds.id, console, error });
    emit({ kind: "statement", index: st.index, sql: st.text, line: st.line, ok, error, rows, elapsedMs });
    if (!ok && r.stopOnError) break;
  }
  await invoke("db_session_close", { sessionId: session.id }).catch(() => undefined);
  summary.elapsedMs = Date.now() - started;
  emit({ kind: "finished", index: summary.executed, ok: summary.failed === 0 && !summary.cancelled, elapsedMs: summary.elapsedMs });
  return summary;
});

registerMock("sql_run_file_cancel", (a) => {
  cancelled.add(a.runId as string);
});
