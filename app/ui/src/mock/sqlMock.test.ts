import { beforeEach, describe, expect, it } from "vitest";
import { invoke, listen } from "@/ipc";
import "./workspaceMock";
import type { DataSourceInfo } from "@/ipc/db";
import type { AnalyzeAnswer, FileSql, HistoryEntry, ParamsAnswer, RunFileEvent, RunFileSummary } from "@/ipc/sql";
import { resetDbMock } from "./dbMock";
import { setSetting } from "./settingsMock";
import "./sqlMock";

async function addShop(): Promise<DataSourceInfo> {
  const [ds] = await invoke<DataSourceInfo[]>("db_add_from_env");
  return ds;
}

describe("SQL editor mock", () => {
  beforeEach(() => {
    resetDbMock();
    setSetting("sql.dialect.mappings", null, "project");
  });

  it("resolves a console against the data source model", async () => {
    const ds = await addShop();
    const answer = await invoke<AnalyzeAnswer>("sql_analyze", {
      request: { text: "SELECT id, emial FROM users u WHERE id = 'abc'", dataSourceId: ds.id, dialect: "mysql" },
    });
    expect(answer.hasModel).toBe(true);
    const ids = answer.problems.map((p) => p.id);
    expect(ids).toContain("SqlResolve");
    expect(ids).toContain("SqlType");
    expect(ids).toContain("SqlUnusedAlias");
    const unresolved = answer.problems.find((p) => p.id === "SqlResolve")!;
    expect(unresolved.message).toBe("Cannot resolve column 'emial'");
  });

  it("takes a file's dialect and data source from the mappings", async () => {
    const ds = await addShop();
    setSetting("sql.dialect.mappings", [{ path: "database", dialect: "postgres" }, { path: "database/queries/reports.sql", dataSource: ds.id }], "project");
    const info = await invoke<FileSql>("sql_file_info", { request: { text: "", path: "database/queries/reports.sql" } });
    expect(info).toMatchObject({ dialect: "postgres", source: "mapping", dataSourceId: ds.id });
    const other = await invoke<FileSql>("sql_file_info", { request: { text: "", path: "other.sql" } });
    expect(other.dialect).toBe("generic");
  });

  it("finds and fills user parameters", async () => {
    const found = await invoke<ParamsAnswer>("sql_params", { request: { text: "SELECT * FROM t WHERE a = :id AND b = ?", dialect: "mysql" } });
    expect(found.names).toEqual([":id", "?1"]);
    const sql = await invoke<string>("sql_substitute", { request: { text: "SELECT * FROM t WHERE a = :id AND b = ?", dialect: "mysql", values: { ":id": "1", "?1": "'x'" } } });
    expect(sql).toBe("SELECT * FROM t WHERE a = 1 AND b = 'x'");
  });

  it("keeps and searches the query history", async () => {
    const entry = (sql: string, time: number): HistoryEntry => ({ sql, time, durationMs: 2, rows: 1, dataSource: "h", console: "console" });
    await invoke("sql_history_add", { entry: entry("SELECT 1", 1) });
    await invoke("sql_history_add", { entry: entry("SELECT name FROM users", 2) });
    const all = await invoke<HistoryEntry[]>("sql_history_list", { dataSourceId: "h", query: {} });
    expect(all.map((e) => e.sql)).toEqual(["SELECT name FROM users", "SELECT 1"]);
    const hits = await invoke<HistoryEntry[]>("sql_history_list", { dataSourceId: "h", query: { text: "users" } });
    expect(hits).toHaveLength(1);
    await invoke("sql_history_clear", { dataSourceId: "h" });
    expect(await invoke<HistoryEntry[]>("sql_history_list", { dataSourceId: "h", query: {} })).toEqual([]);
  });

  it("runs a SQL file statement by statement", async () => {
    const ds = await addShop();
    const events: RunFileEvent[] = [];
    const off = await listen<RunFileEvent>("sql://runFile", (e) => events.push(e));
    const summary = await invoke<RunFileSummary>("sql_run_file", {
      request: { runId: "r1", path: "database/queries/reports.sql", dataSourceId: ds.id, stopOnError: false },
    });
    off();
    expect(summary.total).toBe(5);
    expect(summary.executed).toBe(5);
    expect(summary.failed).toBe(1);
    const statements = events.filter((e) => e.kind === "statement");
    expect(statements[0].sql).toMatch(/^SELECT name, email FROM users/);
    expect(statements[1].ok).toBe(false);
    expect(events.at(-1)?.kind).toBe("finished");
    const history = await invoke<HistoryEntry[]>("sql_history_list", { dataSourceId: ds.id, query: { console: "reports.sql" } });
    expect(history).toHaveLength(5);
  });
});
