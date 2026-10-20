/**
 * Browser-mode database backend: data sources live in memory (never in the project), every
 * data source serves the demo `shop` database, and SQL runs through the small interpreter in
 * `dbEngine.ts`. The demo project's `.env` suggests `shop@localhost`.
 */
import { emitMock, registerMock } from "@/ipc";
import {
  DATA_SOURCES_EVENT,
  SESSION_EVENT,
  dialectOf,
  type Cell,
  type DataSource,
  type DataSourceInfo,
  type DetectedDataSource,
  type ExportFormat,
  type QueryPage,
  type RowChange,
  type SessionInfo,
  type StatementResult,
  type TableDataRequest,
  type TestResult,
} from "@/ipc/db";
import { changeSql, exportRows, leadingKeyword, splitStatements } from "@/features/database/sqlText";
import { createDemoDatabase, type DemoDatabase } from "./dbDemoData";
import { cloneDatabase, runStatement } from "./dbEngine";

const DEFAULT_PAGE = 500;

interface MockSource {
  source: DataSource;
  password?: string;
}

interface MockSession {
  info: SessionInfo;
  snapshot?: DemoDatabase;
}

const sources: MockSource[] = [];
const databases = new Map<string, DemoDatabase>();
const connected = new Set<string>();
const sessions = new Map<string, MockSession>();
let sessionSeq = 1;

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

const envSource = (): DataSource => ({
  id: "",
  name: "shop@localhost",
  driver: "mysql",
  host: "127.0.0.1",
  port: 3306,
  database: "shop",
  user: "shop",
  file: "",
  comment: "",
  scope: "project",
  options: {
    readOnly: false,
    autoCommit: true,
    sslMode: "prefer",
    schemas: [],
    connectTimeoutSecs: 10,
    initSql: "",
    savePassword: true,
    pageSize: null,
    color: null,
  },
});

function summary(ds: DataSource): string {
  if (ds.driver === "sqlite") return ds.file;
  const host = !ds.host || ds.host === "127.0.0.1" ? "localhost" : ds.host;
  return ds.database ? `${ds.database}@${host}` : host;
}

const info = (m: MockSource): DataSourceInfo => ({
  ...m.source,
  hasPassword: m.source.driver !== "sqlite" && !!m.password,
  summary: summary(m.source),
});

const list = () => sources.map(info);
const emitSources = () => emitMock(DATA_SOURCES_EVENT, list());

function get(id: string): MockSource {
  const s = sources.find((x) => x.source.id === id);
  if (!s) throw new Error(`unknown data source: ${id}`);
  return s;
}

function db(id: string): DemoDatabase {
  let d = databases.get(id);
  if (!d) {
    d = createDemoDatabase();
    databases.set(id, d);
  }
  return d;
}

const schemaName = (ds: DataSource) => (ds.driver === "sqlite" ? "main" : ds.driver === "postgres" ? "public" : ds.database || "shop");

function slug(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "db";
}

function testResult(ds: DataSource, password?: string): TestResult {
  const driver = { mysql: "MySQL", mariadb: "MariaDB", postgres: "PostgreSQL", sqlite: "SQLite" }[ds.driver];
  if (ds.driver !== "sqlite" && (ds.host === "unreachable" || password === "wrong")) {
    return {
      ok: false,
      driver,
      latencyMs: 812,
      error:
        password === "wrong"
          ? `[1045] Access denied for user '${ds.user}'@'172.18.0.1' (using password: YES)`
          : "connection timed out (is the server running?)",
    };
  }
  const serverVersion = { mysql: "8.0.36", mariadb: "11.4.2-MariaDB", postgres: "16.3", sqlite: "3.45.1" }[ds.driver];
  return { ok: true, driver, serverVersion, latencyMs: 46 };
}

async function ensureConnected(id: string): Promise<MockSource> {
  const s = get(id);
  if (!connected.has(id)) {
    await delay(160);
    const t = testResult(s.source, s.password);
    if (!t.ok) throw new Error(t.error);
    connected.add(id);
  }
  return s;
}

function session(id: string): MockSession {
  const s = sessions.get(id);
  if (!s) throw new Error(`unknown console session: ${id}`);
  return s;
}

const emitSession = (s: SessionInfo) => emitMock(SESSION_EVENT, { ...s });

registerMock("db_data_sources_list", () => list());

registerMock("db_data_source_save", (a) => {
  const source = { ...(a.source as DataSource) };
  const password = a.password as string | undefined;
  if (!source.id) {
    const base = slug(source.name || source.driver);
    let id = base;
    for (let n = 2; sources.some((s) => s.source.id === id); n++) id = `${base}-${n}`;
    source.id = id;
  }
  if (!source.name) source.name = summary(source);
  const existing = sources.find((s) => s.source.id === source.id);
  if (existing) {
    existing.source = source;
    if (password !== undefined) existing.password = password || undefined;
  } else sources.push({ source, password: password || undefined });
  connected.delete(source.id);
  emitSources();
  return info(get(source.id));
});

registerMock("db_data_source_delete", (a) => {
  const i = sources.findIndex((s) => s.source.id === a.id);
  if (i >= 0) sources.splice(i, 1);
  connected.delete(a.id as string);
  emitSources();
});

registerMock("db_test_connection", async (a) => {
  await delay(380);
  const ds = a.source as DataSource;
  const stored = sources.find((s) => s.source.id === ds.id)?.password;
  return testResult(ds, (a.password as string | undefined) ?? stored);
});

registerMock("db_disconnect", (a) => {
  connected.delete(a.id as string);
  for (const [k, s] of sessions) if (s.info.dataSourceId === a.id) sessions.delete(k);
});

registerMock("db_schemas", async (a) => {
  const s = await ensureConnected(a.id as string);
  const current = schemaName(s.source);
  const extra = s.source.driver === "mysql" || s.source.driver === "mariadb" ? ["information_schema", "mysql", "performance_schema", "sys"] : [];
  return [current, ...extra].sort().map((name) => ({ name, current: name === current }));
});

registerMock("db_introspect", async (a) => {
  const s = await ensureConnected(a.id as string);
  await delay(120);
  if (a.schema !== schemaName(s.source)) return { name: a.schema, tables: [], routines: [] };
  const d = db(s.source.id);
  return {
    name: a.schema,
    tables: Object.values(d.tables)
      .map((t) => ({ ...t.info, rowEstimate: t.info.kind === "table" ? t.rows.length : undefined }))
      .sort((x, y) => x.name.localeCompare(y.name)),
    routines: d.routines,
  };
});

registerMock("db_table_info", async (a) => {
  await ensureConnected(a.id as string);
  const t = db(a.id as string).tables[a.table as string];
  if (!t) throw new Error(`table ${a.schema}.${a.table} not found`);
  return t.info;
});

registerMock("db_table_ddl", async (a) => {
  await ensureConnected(a.id as string);
  const t = db(a.id as string).tables[a.table as string];
  if (!t) throw new Error(`no DDL for ${a.table}`);
  const i = t.info;
  if (i.kind === "view") {
    return "create definer = shop@`%` view customer_totals as\nselect `u`.`id` AS `user_id`, `u`.`name` AS `name`, count(`o`.`id`) AS `orders`, sum(`o`.`total`) AS `spent`\nfrom (`shop`.`users` `u` join `shop`.`orders` `o` on ((`o`.`user_id` = `u`.`id`)))\ngroup by `u`.`id`, `u`.`name`;\n";
  }
  const lines = i.columns.map((c) => {
    let l = `    ${c.name} ${c.dataType}`;
    if (!c.nullable) l += " not null";
    if (c.default != null) l += ` default ${/^\d/.test(c.default) ? c.default : `'${c.default}'`}`;
    if (c.autoIncrement) l += " auto_increment";
    if (c.autoIncrement && i.primaryKey.length === 1 && i.primaryKey[0] === c.name) l += "\n        primary key";
    return l;
  });
  for (const idx of i.indexes.filter((x) => x.unique && !x.primary)) lines.push(`    constraint ${idx.name}\n        unique (${idx.columns.join(", ")})`);
  for (const f of i.foreignKeys)
    lines.push(
      `    constraint ${f.name}\n        foreign key (${f.columns.join(", ")}) references ${f.refTable} (${f.refColumns.join(", ")})${f.onDelete && f.onDelete !== "NO ACTION" ? `\n            on delete ${f.onDelete.toLowerCase()}` : ""}`,
    );
  let out = `create table ${i.name}\n(\n${lines.join(",\n")}\n)${i.comment ? `\n    comment '${i.comment}'` : ""}\n    collate = utf8mb4_unicode_ci;\n`;
  for (const idx of i.indexes.filter((x) => !x.unique && !i.foreignKeys.some((f) => f.name === x.name)))
    out += `\ncreate index ${idx.name}\n    on ${i.name} (${idx.columns.join(", ")});\n`;
  return out;
});

registerMock("db_table_data", async (a): Promise<QueryPage> => {
  const s = await ensureConnected(a.id as string);
  const req = a.request as TableDataRequest;
  const limit = req.limit ?? s.source.options.pageSize ?? DEFAULT_PAGE;
  const offset = req.offset ?? 0;
  let sql = `SELECT * FROM ${req.table}`;
  const filter = req.filter?.trim().replace(/;+$/, "");
  if (filter) sql += ` WHERE ${filter}`;
  if (req.order?.length) sql += ` ORDER BY ${req.order.map((o) => `${o.column}${o.desc ? " DESC" : ""}`).join(", ")}`;
  await delay(60);
  const all = runStatement(db(s.source.id), sql);
  const rows = all.rows.slice(offset, offset + limit);
  const quoted = sql.replace(/FROM (\w+)/, "FROM `$1`");
  return {
    columns: all.columns,
    rows,
    hasMore: offset + limit < all.rows.length,
    offset,
    total: all.rows.length,
    elapsedMs: 9 + (rows.length % 7),
    sql: quoted,
  };
});

registerMock("db_apply_changes", async (a) => {
  const s = await ensureConnected(a.id as string);
  if (s.source.options.readOnly) throw new Error(`${s.source.name} is read-only`);
  const d = db(s.source.id);
  const t = d.tables[a.table as string];
  if (!t) throw new Error(`table ${a.table} not found`);
  const changes = a.changes as RowChange[];
  const dialect = dialectOf(s.source.driver);
  const statements = changes.map((c) => changeSql(dialect, a.schema as string, a.table as string, t.info.primaryKey, c));
  const backup = cloneDatabase(d);
  let affected = 0;
  try {
    for (const sql of statements) affected += runStatement(d, sql).rowsAffected;
  } catch (e) {
    databases.set(s.source.id, backup);
    throw e;
  }
  await delay(80);
  return { statements, rowsAffected: affected };
});

registerMock("db_session_open", async (a) => {
  const s = await ensureConnected(a.id as string);
  const info: SessionInfo = { id: `${s.source.id}#${sessionSeq++}`, dataSourceId: s.source.id, autoCommit: s.source.options.autoCommit, txOpen: false, running: false };
  sessions.set(info.id, { info });
  return { ...info };
});

registerMock("db_session_close", (a) => {
  const s = sessions.get(a.sessionId as string);
  if (s?.snapshot) databases.set(s.info.dataSourceId, s.snapshot);
  sessions.delete(a.sessionId as string);
});

function beginIfNeeded(s: MockSession): void {
  if (!s.info.autoCommit && !s.info.txOpen) {
    s.snapshot = cloneDatabase(db(s.info.dataSourceId));
    s.info.txOpen = true;
  }
}

registerMock("db_execute", async (a): Promise<StatementResult[]> => {
  const s = session(a.sessionId as string);
  const src = get(s.info.dataSourceId);
  const limit = (a.pageSize as number | undefined) ?? src.source.options.pageSize ?? DEFAULT_PAGE;
  s.info.running = true;
  emitSession(s.info);
  await delay(140);
  const out: StatementResult[] = [];
  const stmts = splitStatements(a.sql as string, dialectOf(src.source.driver));
  for (const [index, st] of stmts.entries()) {
    const kw = leadingKeyword(st.text);
    const base = { index, sql: st.text, start: st.start, end: st.end, elapsedMs: 4 + ((st.text.length * 7) % 23) };
    try {
      if (["BEGIN", "START"].includes(kw)) {
        s.snapshot = cloneDatabase(db(s.info.dataSourceId));
        s.info.txOpen = true;
      } else if (["COMMIT", "ROLLBACK"].includes(kw)) {
        if (kw === "ROLLBACK" && s.snapshot) databases.set(s.info.dataSourceId, s.snapshot);
        s.snapshot = undefined;
        s.info.txOpen = false;
      } else if (!["SELECT", "SHOW", "WITH", "EXPLAIN", "DESCRIBE"].includes(kw)) beginIfNeeded(s);
      const r = runStatement(db(s.info.dataSourceId), st.text);
      out.push({ ...base, columns: r.columns, rows: r.rows.slice(0, limit), hasMore: r.rows.length > limit, rowsAffected: r.rowsAffected, returnsRows: r.columns.length > 0 });
    } catch (e) {
      out.push({ ...base, columns: [], rows: [], hasMore: false, rowsAffected: 0, returnsRows: false, error: e instanceof Error ? e.message : String(e) });
      break;
    }
  }
  s.info.running = false;
  emitSession(s.info);
  return out;
});

registerMock("db_fetch", async (a): Promise<QueryPage> => {
  const s = session(a.sessionId as string);
  const src = get(s.info.dataSourceId);
  const limit = (a.limit as number | undefined) ?? src.source.options.pageSize ?? DEFAULT_PAGE;
  const offset = a.offset as number;
  const r = runStatement(db(s.info.dataSourceId), a.sql as string);
  await delay(50);
  return { columns: r.columns, rows: r.rows.slice(offset, offset + limit), hasMore: offset + limit < r.rows.length, offset, elapsedMs: 6, sql: a.sql as string };
});

registerMock("db_set_auto_commit", (a) => {
  const s = session(a.sessionId as string);
  if (a.autoCommit && s.info.txOpen) {
    s.snapshot = undefined;
    s.info.txOpen = false;
  }
  s.info.autoCommit = a.autoCommit as boolean;
  emitSession(s.info);
  return { ...s.info };
});

registerMock("db_commit", (a) => {
  const s = session(a.sessionId as string);
  s.snapshot = undefined;
  s.info.txOpen = false;
  emitSession(s.info);
  return { ...s.info };
});

registerMock("db_rollback", (a) => {
  const s = session(a.sessionId as string);
  if (s.snapshot) databases.set(s.info.dataSourceId, s.snapshot);
  s.snapshot = undefined;
  s.info.txOpen = false;
  emitSession(s.info);
  return { ...s.info };
});

registerMock("db_cancel", () => undefined);

registerMock("db_detect_env", (): DetectedDataSource[] => {
  const ds = envSource();
  if (sources.some((s) => s.source.driver === "mysql" && s.source.database === ds.database)) return [];
  return [{ source: ds, hasPassword: true, origin: ".env" }];
});

registerMock("db_add_from_env", () => {
  const ds = envSource();
  if (sources.some((s) => s.source.driver === "mysql" && s.source.database === ds.database)) return [];
  ds.id = "shop-localhost";
  ds.comment = "From .env";
  sources.push({ source: ds, password: "secret" });
  emitSources();
  return [info(get(ds.id))];
});

registerMock("db_export", (a) =>
  exportRows(a.format as ExportFormat, a.dialect as "mysql", a.table as string | undefined, a.columns as string[], a.rows as Cell[][]),
);

/** Reset all state (tests). */
export function resetDbMock(): void {
  sources.length = 0;
  databases.clear();
  connected.clear();
  sessions.clear();
}
