/**
 * Browser-mode database backend: data sources live in memory (never in the project), every
 * SQL data source serves the demo `shop` database (with SQL Server, ClickHouse and Oracle
 * flavoured types), SQL runs through the small interpreter in `dbEngine.ts`, and Redis and
 * MongoDB data sources serve the demo keyspace and documents of `dbNoSqlDemo.ts`. The
 * read-only / production guard is the real one (`forge-sql` through WebAssembly). The demo
 * project's `.env` suggests `shop@localhost`.
 */
import { emitMock, registerMock } from "@/ipc";
import { isLuaDemo } from "./demoLuaProject";
import { isPythonDemo } from "./demoPythonProject";
import { isGoDemo } from "./demoGoProject";
import { isCppDemo } from "./demoCppProject";
import { isRustDemo } from "./demoRustProject";
import { isJavaDemo } from "./demoJavaProject";
import { isKotlinDemo } from "./demoKotlinProject";
import { isScalaDemo } from "./demoScalaProject";
import { isAndroidDemo } from "./demoAndroidProject";
import {
  DATA_SOURCES_EVENT,
  SESSION_EVENT,
  SESSIONS_EVENT,
  DRIVER_LABELS,
  dialectOf,
  modelOf,
  supportsTransactions,
  type Guard,
  type SecretUpdate,
  type TableInfo,
  type DataSource,
  type DataSourceInfo,
  type DetectedDataSource,
  type QueryPage,
  type RowChange,
  type SessionInfo,
  type SqlStatement,
  type StatementResult,
  type TableDataRequest,
  type TestResult,
} from "@/ipc/db";
import { callWasm, forgeWasm } from "@/wasm";
import { createDemoDatabase, type DemoDatabase } from "./dbDemoData";
import { cloneDatabase, runStatement } from "./dbEngine";
import {
  createMongoDemo,
  createRedisDemo,
  mongoCommand,
  mongoSchema,
  redisCommand,
  redisKeys,
  redisSchemas,
  redisValue,
  type MongoDemo,
  type RedisDemo,
} from "./dbNoSqlDemo";
import { webDemo } from "./demoWebProjects";
import { currentRemote } from "./remoteMock";

const DEFAULT_PAGE = 500;

interface MockSource {
  source: DataSource;
  password?: string;
  sshSecret?: string;
  sslKey?: string;
}

interface MockSession {
  info: SessionInfo;
  snapshot?: DemoDatabase;
  /** Redis database / MongoDB database of the console. */
  redisDb: number;
  mongoDb: string;
}

const sources: MockSource[] = [];
const databases = new Map<string, DemoDatabase>();
const connected = new Set<string>();
const sessions = new Map<string, MockSession>();
let sessionSeq = 1;
const redisDemos = new Map<string, RedisDemo>();
const mongoDemos = new Map<string, MongoDemo>();
/** SSH hosts whose key the user trusted. */
const trustedHosts = new Set<string>();
const BASTION_FINGERPRINT = "SHA256:7xq1bKpDq3Yv2kZQe9wN0cJb4m8Rr5tUaFhGs6LdVio";

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

const sshLabel = (ds: DataSource): string | undefined => {
  const t = ds.options.ssh;
  if (!t?.enabled) return undefined;
  if (t.configurationId) return t.configurationId;
  return t.port && t.port !== 22 ? `${t.user}@${t.host}:${t.port}` : `${t.user}@${t.host}`;
};

/** Data sources with an SSH tunnel go through it; project data sources of a remote project
 * through its SSH connection. */
const via = (ds: DataSource): string | undefined =>
  sshLabel(ds) ?? (ds.scope === "project" ? currentRemote()?.label : undefined);

const info = (m: MockSource): DataSourceInfo => ({
  ...m.source,
  hasPassword: m.source.driver !== "sqlite" && !!m.password,
  hasSshSecret: !!m.sshSecret,
  hasSslPassphrase: !!m.sslKey,
  summary: summary(m.source),
  via: via(m.source),
  model: modelOf(m.source.driver),
  transactions: supportsTransactions(m.source.driver),
  production: m.source.options.color === "red",
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

const schemaName = (ds: DataSource) =>
  ({ sqlite: "main", postgres: "public", sqlserver: "dbo", oracle: "SHOP", redis: `db${Number(ds.database) || 0}` })[ds.driver as string] ??
  (ds.database || "shop");

const redisOf = (id: string) => {
  let d = redisDemos.get(id);
  if (!d) redisDemos.set(id, (d = createRedisDemo()));
  return d;
};
const mongoOf = (id: string) => {
  let d = mongoDemos.get(id);
  if (!d) mongoDemos.set(id, (d = createMongoDemo()));
  return d;
};

/** A demo column type in the data source's own type system. */
function dialectType(driver: DataSource["driver"], t: string): string {
  const x = t.toLowerCase();
  const n = /\((\d+)(?:,(\d+))?\)/.exec(x);
  const base = x.split(/[ (]/)[0];
  switch (driver) {
    case "sqlserver":
      if (x.startsWith("tinyint(1)")) return "bit";
      if (base === "varchar") return `nvarchar(${n?.[1] ?? 255})`;
      if (base === "text" || base === "json" || base === "longtext") return "nvarchar(max)";
      if (base === "timestamp") return "datetime2";
      if (base === "enum") return "nvarchar(20)";
      return base === "decimal" ? `decimal(${n?.[1] ?? 10},${n?.[2] ?? 2})` : base;
    case "clickhouse":
      if (x.startsWith("tinyint(1)")) return "Bool";
      if (base === "bigint" || base === "int") return x.includes("unsigned") ? "UInt64" : "Int64";
      if (base === "varchar" || base === "text" || base === "json" || base === "enum" || base === "longtext") return "String";
      if (base === "timestamp") return "DateTime";
      if (base === "decimal") return `Decimal(${n?.[1] ?? 10}, ${n?.[2] ?? 2})`;
      return base;
    case "oracle":
      if (x.startsWith("tinyint(1)")) return "NUMBER(1,0)";
      if (base === "bigint" || base === "int") return "NUMBER(20,0)";
      if (base === "varchar" || base === "enum") return `VARCHAR2(${n?.[1] ?? 20})`;
      if (base === "text" || base === "json" || base === "longtext") return "CLOB";
      if (base === "timestamp") return "TIMESTAMP(6)";
      if (base === "decimal") return `NUMBER(${n?.[1] ?? 10},${n?.[2] ?? 2})`;
      return base.toUpperCase();
    default:
      return t;
  }
}

function tableFor(driver: DataSource["driver"], t: TableInfo): TableInfo {
  if (!["sqlserver", "clickhouse", "oracle"].includes(driver)) return t;
  return {
    ...t,
    columns: t.columns.map((c) => ({
      ...c,
      dataType: dialectType(driver, c.dataType),
      nullable: driver === "clickhouse" ? false : c.nullable,
      autoIncrement: driver === "clickhouse" ? false : c.autoIncrement,
    })),
    foreignKeys: driver === "clickhouse" ? [] : t.foreignKeys,
    indexes: driver === "clickhouse" ? [] : t.indexes,
  };
}

const ROUTINES: Partial<Record<DataSource["driver"], { name: string; kind: string; returns?: string }[]>> = {
  sqlserver: [
    { name: "usp_archive_orders", kind: "procedure" },
    { name: "usp_monthly_revenue", kind: "procedure" },
    { name: "fn_order_total", kind: "function", returns: "decimal" },
  ],
  clickhouse: [],
  oracle: [
    { name: "ORDER_PKG", kind: "package" },
    { name: "CLOSE_STALE_ORDERS", kind: "procedure" },
  ],
};

function slug(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "db";
}

function testResult(ds: DataSource, password?: string): TestResult {
  const driver = DRIVER_LABELS[ds.driver];
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
  const serverVersion = {
    mysql: "8.0.36",
    mariadb: "11.4.2-MariaDB",
    postgres: "16.3",
    sqlite: "3.45.1",
    sqlserver: "SQL Server 16.0.4135.4 Developer Edition (64-bit)",
    clickhouse: "ClickHouse 24.8.4.13",
    oracle: "Oracle Database 23ai Free Release 23.0.0.0.0",
    redis: "Redis 7.2.4",
    mongodb: "MongoDB 7.0.12",
  }[ds.driver];
  return { ok: true, driver, serverVersion, latencyMs: 46 };
}

/** The SSH tunnel step of a test: an unknown bastion key needs the user's trust. */
function sshCheck(ds: DataSource, acceptHostKey?: string): TestResult | null {
  const t = ds.options.ssh;
  if (!t?.enabled) return null;
  const host = t.configurationId ?? t.host;
  if (!host) return { ok: false, driver: DRIVER_LABELS[ds.driver], latencyMs: 3, error: "SSH tunnel: host is required" };
  if (acceptHostKey === BASTION_FINGERPRINT) trustedHosts.add(host);
  if (!trustedHosts.has(host))
    return {
      ok: false,
      driver: DRIVER_LABELS[ds.driver],
      latencyMs: 212,
      error: `SSH tunnel: the authenticity of host ${host} cannot be established (ssh-ed25519 key fingerprint is ${BASTION_FINGERPRINT})`,
      hostKey: { host, port: t.port || 22, keyType: "ssh-ed25519", fingerprint: BASTION_FINGERPRINT, status: "unknown" },
    };
  return null;
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

const sessionInfos = () => [...sessions.values()].map((s) => ({ ...s.info }));
const emitSessions = () => emitMock(SESSIONS_EVENT, sessionInfos());
const emitSession = (s: SessionInfo) => {
  emitMock(SESSION_EVENT, { ...s });
  emitSessions();
};

const setTx = (s: MockSession, open: boolean) => {
  s.info.txOpen = open;
  s.info.txStartedAt = open ? (s.info.txStartedAt ?? Date.now()) : undefined;
  if (!open) s.info.txStatements = 0;
};

async function guardFor(src: MockSource, sql: string): Promise<Guard> {
  const wasm = await forgeWasm();
  return callWasm<Guard>(wasm.sql_guard, {
    sql,
    dialect: dialectOf(src.source.driver),
    readOnly: src.source.options.readOnly,
    production: src.source.options.color === "red",
    name: src.source.name,
  });
}

registerMock("db_data_sources_list", () => list());

registerMock("db_data_source_save", (a) => {
  const source = { ...(a.source as DataSource) };
  const password = a.password as string | undefined;
  const secrets = (a.secrets as SecretUpdate | undefined) ?? {};
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
    if (secrets.ssh !== undefined) existing.sshSecret = secrets.ssh || undefined;
    if (secrets.sslKey !== undefined) existing.sslKey = secrets.sslKey || undefined;
  } else sources.push({ source, password: password || undefined, sshSecret: secrets.ssh || undefined, sslKey: secrets.sslKey || undefined });
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
  return sshCheck(ds, a.acceptHostKey as string | undefined) ?? testResult(ds, (a.password as string | undefined) ?? stored);
});

registerMock("db_disconnect", (a) => {
  connected.delete(a.id as string);
  for (const [k, s] of sessions) if (s.info.dataSourceId === a.id) sessions.delete(k);
  emitSessions();
});

registerMock("db_schemas", async (a) => {
  const s = await ensureConnected(a.id as string);
  const current = schemaName(s.source);
  if (s.source.driver === "redis") return redisSchemas(redisOf(s.source.id), Number(s.source.database) || 0);
  if (s.source.driver === "mongodb") return [...mongoOf(s.source.id).keys()].sort().map((name) => ({ name, current: name === current }));
  const extra = (
    {
      mysql: ["information_schema", "mysql", "performance_schema", "sys"],
      mariadb: ["information_schema", "mysql", "performance_schema", "sys"],
      sqlserver: ["sales", "staging"],
      clickhouse: ["default", "system"],
      oracle: ["HR", "SALES"],
    } as Partial<Record<DataSource["driver"], string[]>>
  )[s.source.driver] ?? [];
  return [current, ...extra].sort().map((name) => ({ name, current: name === current }));
});

registerMock("db_introspect", async (a) => {
  const s = await ensureConnected(a.id as string);
  await delay(120);
  if (s.source.driver === "mongodb") return mongoSchema(mongoOf(s.source.id), a.schema as string);
  if (s.source.driver === "redis") return { name: a.schema, tables: [], routines: [] };
  if (a.schema !== schemaName(s.source)) return { name: a.schema, tables: [], routines: [] };
  const d = db(s.source.id);
  return {
    name: a.schema,
    tables: Object.values(d.tables)
      .map((t) => tableFor(s.source.driver, { ...t.info, rowEstimate: t.info.kind === "table" ? t.rows.length : undefined }))
      .sort((x, y) => x.name.localeCompare(y.name)),
    routines: ROUTINES[s.source.driver] ?? d.routines,
  };
});

registerMock("db_table_info", async (a) => {
  const s = await ensureConnected(a.id as string);
  const t = db(a.id as string).tables[a.table as string];
  if (!t) throw new Error(`table ${a.schema}.${a.table} not found`);
  return tableFor(s.source.driver, t.info);
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
  if (!supportsTransactions(s.source.driver)) throw new Error(`editing rows of ${DRIVER_LABELS[s.source.driver]} data sources is not supported; use a console`);
  const d = db(s.source.id);
  const t = d.tables[a.table as string];
  if (!t) throw new Error(`table ${a.table} not found`);
  const changes = a.changes as RowChange[];
  const dialect = dialectOf(s.source.driver);
  const wasm = await forgeWasm();
  const statements = await callWasm<string[]>(wasm.sql_change_statements, {
    dialect,
    schema: a.schema,
    table: a.table,
    pk: t.info.primaryKey,
    changes,
  });
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
  const shared = !!a.shared || !!s.source.options.singleSession;
  const sharedId = `${s.source.id}#shared`;
  const existing = shared ? sessions.get(sharedId) : undefined;
  if (existing) {
    existing.info.consoles = (existing.info.consoles ?? 1) + 1;
    emitSessions();
    return { ...existing.info };
  }
  const transactions = supportsTransactions(s.source.driver);
  const info: SessionInfo = {
    id: shared ? sharedId : `${s.source.id}#${sessionSeq++}`,
    dataSourceId: s.source.id,
    name: shared ? "shared" : "console",
    shared,
    consoles: 1,
    autoCommit: s.source.options.autoCommit || !transactions,
    txOpen: false,
    transactions,
    running: false,
    openedAt: Date.now(),
    txStatements: 0,
  };
  sessions.set(info.id, { info, redisDb: Number(s.source.database) || 0, mongoDb: s.source.database || "shop" });
  emitSessions();
  return { ...info };
});

function closeSession(id: string): void {
  const s = sessions.get(id);
  if (s?.snapshot) databases.set(s.info.dataSourceId, s.snapshot);
  sessions.delete(id);
  emitSessions();
}

registerMock("db_session_close", (a) => {
  const s = sessions.get(a.sessionId as string);
  if (s && (s.info.consoles ?? 1) > 1) {
    s.info.consoles = (s.info.consoles ?? 1) - 1;
    emitSessions();
    return;
  }
  closeSession(a.sessionId as string);
});

registerMock("db_sessions_list", () => sessionInfos());
registerMock("db_session_terminate", (a) => closeSession(a.sessionId as string));
registerMock("db_guard", async (a) => guardFor(get(a.id as string), a.sql as string));
registerMock("db_redis_keys", async (a) => {
  await ensureConnected(a.id as string);
  await delay(60);
  return redisKeys(redisOf(a.id as string), a.schema as string, (a.pattern as string) || "*", (a.limit as number | undefined) ?? 1000);
});
registerMock("db_redis_value", async (a) => {
  await ensureConnected(a.id as string);
  await delay(40);
  return redisValue(redisOf(a.id as string), a.schema as string, a.key as string);
});

function beginIfNeeded(s: MockSession): void {
  if (!s.info.autoCommit && !s.info.txOpen) {
    s.snapshot = cloneDatabase(db(s.info.dataSourceId));
    setTx(s, true);
  }
}

registerMock("db_execute", async (a): Promise<StatementResult[]> => {
  const s = session(a.sessionId as string);
  const src = get(s.info.dataSourceId);
  const guard = await guardFor(src, a.sql as string);
  if (guard.blocked) throw new Error(guard.blocked);
  if (guard.confirm && !a.confirmed) throw new Error(guard.confirm);
  const limit = (a.pageSize as number | undefined) ?? src.source.options.pageSize ?? DEFAULT_PAGE;
  const model = modelOf(src.source.driver);
  s.info.running = true;
  emitSession(s.info);
  await delay(140);
  const out: StatementResult[] = [];
  const wasm = await forgeWasm();
  const stmts = await callWasm<SqlStatement[]>(wasm.sql_split, { sql: a.sql, dialect: dialectOf(src.source.driver) });
  const keywords = await callWasm<string[]>(wasm.sql_leading_keywords, { statements: stmts.map((s) => s.text) });
  for (const [index, st] of stmts.entries()) {
    const kw = keywords[index];
    const base = { index, sql: st.text, start: st.start, end: st.end, elapsedMs: 4 + ((st.text.length * 7) % 23) };
    s.info.lastSql = st.text;
    try {
      if (model !== "relational") {
        const r = model === "keyValue" ? redisCommand(redisOf(src.source.id), { get db() { return s.redisDb; }, set db(n) { s.redisDb = n; } }, st.text) : mongoCommand(mongoOf(src.source.id), { get db() { return s.mongoDb; }, set db(n) { s.mongoDb = n; } }, st.text);
        out.push({ ...base, columns: r.columns, rows: r.rows.slice(0, limit), hasMore: r.rows.length > limit, rowsAffected: r.rowsAffected, returnsRows: r.columns.length > 0, documents: r.documents?.slice(0, limit) });
        continue;
      }
      if (s.info.transactions && ["BEGIN", "START"].includes(kw)) {
        s.snapshot = cloneDatabase(db(s.info.dataSourceId));
        setTx(s, true);
      } else if (s.info.transactions && ["COMMIT", "ROLLBACK"].includes(kw)) {
        if (kw === "ROLLBACK" && s.snapshot) databases.set(s.info.dataSourceId, s.snapshot);
        s.snapshot = undefined;
        setTx(s, false);
      } else if (!["SELECT", "SHOW", "WITH", "EXPLAIN", "DESCRIBE"].includes(kw)) beginIfNeeded(s);
      const r = runStatement(db(s.info.dataSourceId), st.text);
      if (s.info.txOpen && !["BEGIN", "START", "COMMIT", "ROLLBACK"].includes(kw)) s.info.txStatements = (s.info.txStatements ?? 0) + 1;
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
  const model = modelOf(src.source.driver);
  const r =
    model === "keyValue"
      ? redisCommand(redisOf(src.source.id), { db: s.redisDb }, a.sql as string)
      : model === "document"
        ? mongoCommand(mongoOf(src.source.id), { db: s.mongoDb }, a.sql as string)
        : runStatement(db(s.info.dataSourceId), a.sql as string);
  await delay(50);
  return { columns: r.columns, rows: r.rows.slice(offset, offset + limit), hasMore: offset + limit < r.rows.length, offset, elapsedMs: 6, sql: a.sql as string };
});

registerMock("db_set_auto_commit", (a) => {
  const s = session(a.sessionId as string);
  if (!a.autoCommit && !s.info.transactions) throw new Error(`${DRIVER_LABELS[get(s.info.dataSourceId).source.driver]} consoles run every statement on its own (no manual transactions)`);
  if (a.autoCommit && s.info.txOpen) {
    s.snapshot = undefined;
    setTx(s, false);
  }
  s.info.autoCommit = a.autoCommit as boolean;
  emitSession(s.info);
  return { ...s.info };
});

registerMock("db_commit", (a) => {
  const s = session(a.sessionId as string);
  s.snapshot = undefined;
  setTx(s, false);
  emitSession(s.info);
  return { ...s.info };
});

registerMock("db_rollback", (a) => {
  const s = session(a.sessionId as string);
  if (s.snapshot) databases.set(s.info.dataSourceId, s.snapshot);
  s.snapshot = undefined;
  setTx(s, false);
  emitSession(s.info);
  return { ...s.info };
});

registerMock("db_cancel", () => undefined);

registerMock("db_detect_env", (): DetectedDataSource[] => {
  // The WebForge demos and the Lua demo game have no .env with a database.
  if (webDemo() || isLuaDemo() || isPythonDemo() || isGoDemo() || isRustDemo() || isJavaDemo() || isKotlinDemo() || isScalaDemo() || isAndroidDemo() || isCppDemo()) return [];
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

registerMock("db_export", async (a) => {
  const wasm = await forgeWasm();
  return callWasm<string>(wasm.sql_export, { format: a.format, dialect: a.dialect, table: a.table ?? null, columns: a.columns, rows: a.rows });
});
registerMock("db_statement_at", async (a) => {
  const wasm = await forgeWasm();
  return callWasm<SqlStatement | null>(wasm.sql_statement_at, { sql: a.sql, from: a.from, to: a.to, dialect: a.dialect });
});

/** Reset all state (tests). */
export function resetDbMock(): void {
  sources.length = 0;
  databases.clear();
  connected.clear();
  sessions.clear();
  redisDemos.clear();
  mongoDemos.clear();
  trustedHosts.clear();
}

/** Demo data sources of every engine (screenshots, `window.__forgeDbMock.addDemoSources()`). */
export function addDemoSources(): void {
  const base = envSource();
  const add = (s: Partial<DataSource> & Pick<DataSource, "id" | "name" | "driver">, options: Partial<DataSource["options"]> = {}) => {
    if (sources.some((x) => x.source.id === s.id)) return;
    sources.push({ source: { ...base, host: "localhost", user: "", database: "", comment: "", ...s, options: { ...base.options, ...options } }, password: "secret" });
  };
  add({ id: "sales-prod", name: "sales (production)", driver: "sqlserver", host: "sql01.internal", port: 1433, user: "forge_ro", database: "shop" }, {
    color: "red",
    sslMode: "verify-full",
    ssl: { caFile: "certs/corp-root-ca.pem" },
    ssh: { enabled: true, configurationId: null, host: "bastion.example.com", port: 22, user: "deploy", auth: "keyPair", keyFile: "~/.ssh/id_ed25519" },
  });
  add({ id: "analytics", name: "analytics", driver: "clickhouse", port: 8123, user: "default", database: "shop" }, { color: "yellow" });
  add({ id: "hr-oracle", name: "hr@oracle", driver: "oracle", port: 1521, user: "hr", database: "FREEPDB1" });
  add({ id: "cache", name: "cache", driver: "redis", port: 6379, database: "0" }, { color: "green" });
  add({ id: "catalog", name: "catalog", driver: "mongodb", port: 27017, database: "shop" });
  emitSources();
}

declare global {
  interface Window {
    __forgeDbMock?: { addDemoSources: () => void };
  }
}

if (typeof window !== "undefined") window.__forgeDbMock = { addDemoSources };
