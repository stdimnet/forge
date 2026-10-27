/**
 * Browser-mode database backend: data sources live in memory (never in the project), every
 * SQL data source serves the demo `shop` database (with SQL Server, ClickHouse and Oracle
 * flavoured types), SQL runs through the small interpreter in `dbEngine.ts`, and Redis and
 * MongoDB data sources serve the demo keyspace and documents of `dbNoSqlDemo.ts`. The
 * read-only / production guard is the real one (`forge-sql` through WebAssembly). The demo
 * project's `.env` suggests `shop@localhost`.
 *
 * The Database Explorer runs on the real model code (`forge-dbmodel` through WebAssembly):
 * the schema filter, DDL data sources (parsing the demo project's SQL files), virtual
 * foreign keys and columns (kept in memory, never in the project), Quick Documentation and
 * the full-text search over the demo rows. The demo schema is enriched with checks,
 * triggers, routines with arguments, and (PostgreSQL) materialized views, sequences and
 * types, roles and other databases.
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
  MODEL_EVENT,
  SEARCH_EVENT,
  SESSION_EVENT,
  SESSIONS_EVENT,
  DRIVER_LABELS,
  dialectOf,
  modelOf,
  supportsTransactions,
  type Guard,
  type SecretUpdate,
  type TableInfo,
  type DbModel,
  type ModelWarning,
  type ObjectDoc,
  type ObjectRef,
  type RoleInfo,
  type SchemaInfo,
  type SchemaName,
  type SearchRequest,
  type SearchSummary,
  type TableMatches,
  type VirtualObjects,
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
import { invoke } from "@/ipc";
import type { FileEntry } from "@/ipc/workspace";
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
  if (ds.driver === "ddl") {
    const f = ds.options.ddlFiles ?? [];
    return f.length > 1 ? `${f[0]} +${f.length - 1}` : (f[0] ?? "no DDL files");
  }
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
    ddl: undefined,
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
  models.delete(source.id);
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

/** Every schema of a data source (before the schema filter). */
async function schemaList(s: MockSource): Promise<SchemaName[]> {
  const current = schemaName(s.source);
  if (s.source.driver === "redis") return redisSchemas(redisOf(s.source.id), Number(s.source.database) || 0);
  if (s.source.driver === "mongodb") return [...mongoOf(s.source.id).keys()].sort().map((name) => ({ name, current: name === current }));
  const extra = (
    {
      mysql: ["information_schema", "mysql", "performance_schema", "sys", "shop_archive"],
      mariadb: ["information_schema", "mysql", "performance_schema", "sys"],
      postgres: ["audit", "billing", "tenant_acme", "tenant_globex", "tenant_initech"],
      sqlserver: ["sales", "staging"],
      clickhouse: ["default", "system"],
      oracle: ["HR", "SALES"],
    } as Partial<Record<DataSource["driver"], string[]>>
  )[s.source.driver] ?? [];
  return [current, ...extra].sort().map((name) => ({ name, current: name === current }));
}

/** The schema filter, by the real `forge-dbmodel` code. */
async function applyFilter(s: MockSource, list: SchemaName[]): Promise<SchemaName[]> {
  const wasm = await forgeWasm();
  return callWasm<SchemaName[]>(wasm.dbmodel_match_schemas, { patterns: s.source.options.schemas, schemas: list });
}

/** Virtual foreign keys and columns per data source (memory only, never the project). */
const virtuals = new Map<string, VirtualObjects>();

async function withVirtuals(id: string, info: SchemaInfo): Promise<SchemaInfo> {
  const v = virtuals.get(id);
  if (!v || (!v.foreignKey?.length && !v.column?.length)) return info;
  const wasm = await forgeWasm();
  return callWasm<SchemaInfo>(wasm.dbmodel_apply_virtual, { dataSource: id, objects: v, schema: info });
}

const TRIGGER_BODY = "BEGIN\n    INSERT INTO audit_log (occurred_at, actor, action, payload)\n    VALUES (NOW(), CURRENT_USER(), 'order.status', JSON_OBJECT('id', NEW.id, 'status', NEW.status));\nEND";

/** The demo schema with the objects a real catalog has: checks, triggers, routines with
 * arguments, and per dialect materialized views, sequences and types. */
function enrich(driver: DataSource["driver"], schema: string, info: SchemaInfo): SchemaInfo {
  const pg = driver === "postgres";
  const tables = info.tables.map((x) => ({ ...x, foreignKeys: x.foreignKeys.map((f) => ({ ...f, refSchema: schema })) })).map((t) => {
    if (t.name === "orders")
      return {
        ...t,
        checks: [{ name: "orders_total_positive", expression: "(`total` >= 0)" }],
        triggers: [{ name: "orders_status_audit", timing: "AFTER", events: "UPDATE", definition: `CREATE TRIGGER orders_status_audit AFTER UPDATE ON orders FOR EACH ROW\n${TRIGGER_BODY}` }],
      };
    if (t.name === "order_items") return { ...t, checks: [{ name: "order_items_quantity_check", expression: "(`quantity` > 0)" }] };
    if (t.name === "products")
      return {
        ...t,
        triggers: [
          { name: "products_touch", timing: "BEFORE", events: "UPDATE", definition: "CREATE TRIGGER products_touch BEFORE UPDATE ON products FOR EACH ROW SET NEW.updated_at = NOW()" },
        ],
      };
    if (t.name === "customer_totals")
      return { ...t, definition: "SELECT u.id AS user_id, u.name, COUNT(o.id) AS orders, SUM(o.total) AS spent\nFROM users u JOIN orders o ON o.user_id = u.id\nGROUP BY u.id, u.name" };
    return t;
  });
  if (pg)
    tables.push({
      name: "daily_revenue",
      kind: "matview",
      columns: [
        { name: "day", dataType: "date", nullable: true, autoIncrement: false },
        { name: "orders", dataType: "bigint", nullable: true, autoIncrement: false },
        { name: "revenue", dataType: "numeric", nullable: true, autoIncrement: false },
      ],
      primaryKey: [],
      foreignKeys: [],
      indexes: [{ name: "daily_revenue_day_idx", columns: ["day"], unique: true, primary: false }],
      definition: "SELECT date(placed_at) AS day, count(*) AS orders, sum(total) AS revenue\nFROM orders\nGROUP BY date(placed_at)",
    });
  const routines = info.routines.map((r) =>
    r.name === "order_total"
      ? {
          ...r,
          arguments: [{ name: "o_id", dataType: "bigint unsigned" }],
          language: "SQL",
          definition: `CREATE FUNCTION order_total(o_id BIGINT UNSIGNED) RETURNS DECIMAL(10,2)\n    READS SQL DATA\nBEGIN\n    RETURN (SELECT SUM(quantity * unit_price) FROM order_items WHERE order_id = o_id);\nEND`,
        }
      : r.name === "close_stale_orders"
        ? {
            ...r,
            arguments: [{ name: "days", dataType: "int" }, { name: "closed", dataType: "int", mode: "OUT" }],
            language: "SQL",
            definition: `CREATE PROCEDURE close_stale_orders(IN days INT, OUT closed INT)\nBEGIN\n    UPDATE orders SET status = 'cancelled' WHERE status = 'pending' AND placed_at < NOW() - INTERVAL days DAY;\n    SET closed = ROW_COUNT();\nEND`,
          }
        : r,
  );
  if (pg) routines.push({ name: "touch_updated_at", kind: "function", returns: "trigger", arguments: [], language: "plpgsql", definition: "" });
  return {
    ...info,
    name: schema,
    tables: tables.sort((x, y) => x.name.localeCompare(y.name)),
    routines,
    sequences: pg
      ? [
          { name: "orders_id_seq", dataType: "bigint", start: 1, increment: 1 },
          { name: "invoice_number_seq", dataType: "bigint", start: 10000, increment: 1 },
        ]
      : undefined,
    types: pg
      ? [
          { name: "order_status", kind: "enum", labels: ["pending", "paid", "shipped", "delivered", "cancelled"] },
          { name: "email", kind: "domain", definition: "character varying(255)" },
        ]
      : undefined,
  };
}

/** One schema as the backend introspects it (before virtual objects). */
async function introspectSchema(s: MockSource, schema: string): Promise<SchemaInfo> {
  if (s.source.driver === "mongodb") return mongoSchema(mongoOf(s.source.id), schema);
  if (s.source.driver === "redis") return { name: schema, tables: [], routines: [] };
  if (schema !== schemaName(s.source)) return { name: schema, tables: [], routines: [] };
  const d = db(s.source.id);
  return enrich(s.source.driver, schema, {
    name: schema,
    tables: Object.values(d.tables)
      .map((t) => tableFor(s.source.driver, { ...t.info, rowEstimate: t.info.kind === "table" ? t.rows.length : undefined }))
      .sort((x, y) => x.name.localeCompare(y.name)),
    routines: ROUTINES[s.source.driver] ?? d.routines,
  });
}

const ROLES: Partial<Record<DataSource["driver"], RoleInfo[]>> = {
  mysql: [
    { name: "root@localhost", kind: "user", superuser: true, canLogin: true },
    { name: "shop@%", kind: "user", canLogin: true },
    { name: "reporting@%", kind: "user", canLogin: true },
  ],
  postgres: [
    { name: "postgres", kind: "user", superuser: true, canLogin: true },
    { name: "shop", kind: "user", canLogin: true },
    { name: "readonly", kind: "role" },
  ],
  sqlserver: [
    { name: "sa", kind: "user", superuser: true, canLogin: true },
    { name: "shop_app", kind: "user", canLogin: true },
    { name: "sysadmin", kind: "role", superuser: true },
  ],
};

/** Files of a DDL data source from the demo project (folders: their `.sql` files). */
async function ddlFiles(paths: string[]): Promise<{ path: string; text: string }[]> {
  const out: { path: string; text: string }[] = [];
  const walk = async (path: string) => {
    try {
      out.push({ path, text: await invoke<string>("fs_read_file", { path }) });
      return;
    } catch {
      // A folder.
    }
    const entries = await invoke<FileEntry[]>("fs_list_dir", { path }).catch(() => [] as FileEntry[]);
    const sorted = [...entries].sort((a, b) => a.name.localeCompare(b.name, undefined, { numeric: true }));
    for (const e of sorted) {
      if (e.kind === "dir") await walk(e.path);
      else if (/\.(sql|ddl)$/i.test(e.name)) out.push({ path: e.path, text: await invoke<string>("fs_read_file", { path: e.path }) });
    }
  };
  for (const p of paths) await walk(p.replace(/^\.\//, "").replace(/\/$/, ""));
  return out;
}

/** Explorer models: introspected schemas per data source. */
const models = new Map<string, Record<string, SchemaInfo>>();

async function buildModel(id: string, refresh: boolean): Promise<DbModel> {
  const s = get(id);
  const dialect = dialectOf(s.source.driver, s.source.options.ddlDialect);
  if (s.source.driver === "ddl") {
    const wasm = await forgeWasm();
    const parsed = await callWasm<{ schemas: SchemaInfo[]; warnings: ModelWarning[] }>(wasm.dbmodel_parse_ddl, {
      dialect,
      defaultSchema: s.source.database,
      files: await ddlFiles(s.source.options.ddlFiles ?? []),
    });
    const fallback = { postgres: "public", sqlite: "main", sqlserver: "dbo", clickhouse: "default", oracle: "PUBLIC" }[dialect as string] ?? "public";
    const current = s.source.database || fallback;
    const schemas = await applyFilter(
      s,
      parsed.schemas.map((x) => ({ name: x.name, current: x.name === current })),
    );
    const loaded: Record<string, SchemaInfo> = {};
    for (const x of parsed.schemas) if (schemas.find((n) => n.name === x.name)?.visible) loaded[x.name] = await withVirtuals(id, x);
    return { dialect, schemas, loaded, refreshedAt: Date.now(), warnings: parsed.warnings };
  }
  await ensureConnected(id);
  await delay(80);
  const schemas = await applyFilter(s, await schemaList(s));
  let cached = models.get(id) ?? {};
  if (refresh) {
    const again: Record<string, SchemaInfo> = {};
    for (const name of Object.keys(cached)) if (schemas.find((x) => x.name === name)?.visible) again[name] = await introspectSchema(s, name);
    cached = again;
  }
  models.set(id, cached);
  const loaded: Record<string, SchemaInfo> = {};
  for (const [name, info] of Object.entries(cached)) if (schemas.find((x) => x.name === name)?.visible) loaded[name] = await withVirtuals(id, info);
  const multi = s.source.driver === "postgres" || s.source.driver === "sqlserver";
  return {
    dialect,
    catalog: multi ? s.source.database || "shop" : undefined,
    catalogs: s.source.driver === "postgres" ? ["postgres", s.source.database || "shop", "shop_test"] : s.source.driver === "sqlserver" ? ["master", s.source.database || "shop", "tempdb"] : undefined,
    schemas,
    loaded,
    roles: ROLES[s.source.driver] ?? [],
    refreshedAt: Date.now(),
  };
}

registerMock("db_model", (a) => buildModel(a.id as string, !!a.refresh));

registerMock("db_schemas", async (a) => (await buildModel(a.id as string, false)).schemas);

registerMock("db_introspect", async (a) => {
  const id = a.id as string;
  const s = get(id);
  const model = await buildModel(id, false);
  const schema = a.schema as string;
  if (s.source.driver === "ddl") {
    const info = model.loaded[schema];
    if (!info) throw new Error(`schema ${schema} not found`);
    return info;
  }
  if (!model.schemas.find((x) => x.name === schema)?.visible) throw new Error(`schema ${schema} is hidden by the schema filter of ${s.source.name}`);
  await delay(120);
  const info = await introspectSchema(s, schema);
  models.set(id, { ...(models.get(id) ?? {}), [schema]: info });
  return withVirtuals(id, info);
});

registerMock("db_match_schemas", async (a) => {
  const wasm = await forgeWasm();
  return callWasm<SchemaName[]>(wasm.dbmodel_match_schemas, { patterns: a.patterns, schemas: a.schemas });
});

registerMock("db_object_doc", async (a): Promise<ObjectDoc> => {
  const id = a.id as string;
  const object = a.object as ObjectRef;
  const model = await buildModel(id, false);
  if (object.kind === "role") {
    const r = model.roles?.find((x) => x.name === object.name);
    if (!r) throw new Error(`role ${object.name} not found`);
    return {
      kind: r.kind,
      name: r.name,
      location: "",
      ddl: `CREATE ${r.kind === "user" ? "USER" : "ROLE"} ${r.name};\n`,
      properties: [
        { name: "Superuser", value: r.superuser ? "yes" : "no" },
        { name: "Can log in", value: r.canLogin ? "yes" : "no" },
      ],
    };
  }
  const schema = model.loaded[a.schema as string] ?? (await withVirtuals(id, await introspectSchema(get(id), a.schema as string)));
  const wasm = await forgeWasm();
  const doc = await callWasm<ObjectDoc | null>(wasm.dbmodel_object_doc, { dialect: model.dialect, schema, object });
  if (!doc) throw new Error(`${object.kind} ${object.name} not found`);
  return doc;
});

registerMock("db_virtual_objects", (a) => structuredClone(virtuals.get(a.id as string) ?? {}));

registerMock("db_virtual_objects_save", async (a) => {
  const id = a.id as string;
  const objects = a.objects as VirtualObjects;
  const wasm = await forgeWasm();
  await callWasm(wasm.dbmodel_apply_virtual, { dataSource: id, objects });
  virtuals.set(id, {
    foreignKey: (objects.foreignKey ?? []).map((f) => ({ ...f, dataSource: id })),
    column: (objects.column ?? []).map((c) => ({ ...c, dataSource: id })),
  });
  emitMock(MODEL_EVENT, [id]);
});

const cancelledSearches = new Set<string>();

registerMock("db_search_cancel", (a) => {
  cancelledSearches.add(a.searchId as string);
});

registerMock("db_search", async (a): Promise<SearchSummary> => {
  const searchId = a.searchId as string;
  const req = a.request as SearchRequest;
  const started = Date.now();
  const wasm = await forgeWasm();
  const targets: { id: string; schema: string; table: TableInfo; error?: string }[] = [];
  for (const scope of req.scopes) {
    const s = get(scope.dataSource);
    if (s.source.driver === "ddl" || modelOf(s.source.driver) !== "relational") {
      targets.push({ id: s.source.id, schema: "", table: { name: "", kind: "table", columns: [], primaryKey: [], foreignKeys: [], indexes: [] }, error: s.source.driver === "ddl" ? "DDL data sources have no data" : `${DRIVER_LABELS[s.source.driver]} cannot be searched; use its console` });
      continue;
    }
    const model = await buildModel(s.source.id, false);
    const schemas = scope.schemas?.length ? scope.schemas : model.schemas.filter((x) => x.visible).map((x) => x.name);
    const patterns = scope.tables ?? [];
    for (const schema of schemas) {
      const info = model.loaded[schema] ?? (await introspectSchema(s, schema));
      const names = patterns.length
        ? await callWasm<SchemaName[]>(wasm.dbmodel_match_schemas, { patterns, schemas: info.tables.map((t) => ({ name: t.name, current: false })) })
        : info.tables.map((t) => ({ name: t.name, current: false, visible: true }));
      for (const t of info.tables) if (t.kind !== "collection" && names.find((n) => n.name === t.name)?.visible) targets.push({ id: s.source.id, schema, table: t });
    }
  }
  const summary: SearchSummary = { tables: targets.length, matchedTables: 0, rows: 0, errors: 0, cancelled: false, elapsedMs: 0 };
  for (let i = 0; i < targets.length; i++) {
    if (cancelledSearches.has(searchId)) {
      summary.cancelled = true;
      break;
    }
    await delay(70);
    const t = targets[i];
    const result: TableMatches = { dataSource: t.id, schema: t.schema, table: t.table.name, columns: [], rows: [], matches: [], truncated: false, sql: "" };
    if (t.error) result.error = t.error;
    else {
      const demo = db(t.id).tables[t.table.name];
      const columns = t.table.columns.filter((c) => !c.virtual).map((c) => c.name);
      const res = await callWasm<{ sql: string; rows: TableMatches["rows"]; matches: number[][]; truncated: boolean } | { error: string }>(wasm.dbmodel_search_rows, {
        dialect: dialectOf(get(t.id).source.driver),
        schema: t.schema,
        table: t.table,
        options: { text: req.text, mode: req.mode, caseSensitive: req.caseSensitive, allColumns: req.allColumns, limit: req.limit },
        columns,
        rows: demo?.rows ?? [],
      }).catch((e: unknown) => ({ error: e instanceof Error ? e.message : String(e) }));
      if ("error" in res) result.error = res.error;
      else Object.assign(result, { ...res, columns });
    }
    if (result.error) summary.errors++;
    if (result.rows.length) {
      summary.matchedTables++;
      summary.rows += result.rows.length;
    }
    emitMock(SEARCH_EVENT, {
      searchId,
      done: i + 1,
      total: targets.length,
      current: `${t.id} › ${t.schema}.${t.table.name}`,
      result: result.error || result.rows.length ? result : undefined,
    });
  }
  cancelledSearches.delete(searchId);
  summary.elapsedMs = Date.now() - started;
  return summary;
});

registerMock("db_table_info", async (a) => {
  const id = a.id as string;
  if (get(id).source.driver === "ddl") {
    const t = (await buildModel(id, false)).loaded[a.schema as string]?.tables.find((x) => x.name === a.table);
    if (!t) throw new Error(`table ${a.schema}.${a.table} not found`);
    return t;
  }
  const s = await ensureConnected(id);
  const info = await withVirtuals(id, await introspectSchema(s, a.schema as string));
  const t = info.tables.find((x) => x.name === a.table);
  if (!t) throw new Error(`table ${a.schema}.${a.table} not found`);
  return t;
});

registerMock("db_table_ddl", async (a) => {
  if (get(a.id as string).source.driver === "ddl") {
    const doc = await invoke<ObjectDoc>("db_object_doc", { id: a.id, schema: a.schema, object: { kind: "table", name: a.table } });
    return doc.ddl;
  }
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
  // The same SQL as the backend (`forge_dataedit::query`), run by the demo engine unpaged.
  const wasm = await forgeWasm();
  const q = await callWasm<{ select: string; order: string; display: string }>(wasm.dataedit_table_query, {
    dialect: dialectOf(s.source.driver),
    query: { schema: req.schema, table: req.table, filter: req.filter ?? null, columnFilters: req.columnFilters ?? [], order: req.order ?? [] },
  });
  await delay(60);
  const all = runStatement(db(s.source.id), q.display);
  const rows = all.rows.slice(offset, offset + limit);
  return {
    columns: all.columns,
    rows,
    hasMore: offset + limit < all.rows.length,
    offset,
    total: all.rows.length,
    elapsedMs: 9 + (rows.length % 7),
    sql: q.display,
  };
});

/** DML of pending changes with the backend's key choice (`forge_dataedit::api::preview_changes`). */
async function previewChanges(src: MockSource, schema: string, table: string, changes: RowChange[]) {
  if (!supportsTransactions(src.source.driver)) throw new Error(`editing rows of ${DRIVER_LABELS[src.source.driver]} data sources is not supported; use a console`);
  const t = db(src.source.id).tables[table];
  if (!t) throw new Error(`table ${table} not found`);
  const wasm = await forgeWasm();
  return callWasm<{ statements: string[]; key?: { columns: string[]; source: string } }>(wasm.dataedit_preview, {
    dialect: dialectOf(src.source.driver),
    schema,
    table,
    info: t.info,
    changes,
  });
}

registerMock("db_preview_changes", async (a) => {
  const s = await ensureConnected(a.id as string);
  const changes = a.changes as RowChange[];
  const p = await previewChanges(s, a.schema as string, a.table as string, changes);
  return {
    ...p,
    blocked: s.source.options.readOnly ? `${s.source.name} is read-only` : undefined,
    confirm:
      s.source.options.color === "red"
        ? `${s.source.name} is a production data source. Submit ${changes.length} change${changes.length === 1 ? "" : "s"} to ${a.table}?`
        : undefined,
  };
});

registerMock("db_apply_changes", async (a) => {
  const s = await ensureConnected(a.id as string);
  if (s.source.options.readOnly) throw new Error(`${s.source.name} is read-only`);
  if (s.source.options.color === "red" && !a.confirmed) throw new Error(`${s.source.name} is a production data source; confirm the changes first`);
  const d = db(s.source.id);
  const changes = a.changes as RowChange[];
  const { statements } = await previewChanges(s, a.schema as string, a.table as string, changes);
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
  models.clear();
  virtuals.clear();
}

/** A PostgreSQL shop with schemas, roles and a virtual foreign key, and a DDL data source
 * over the demo project's `database/ddl` migrations (screenshots of the explorer). */
export function addExplorerSources(): void {
  const base = envSource();
  if (!sources.some((x) => x.source.id === "shop-pg"))
    sources.push({
      source: { ...base, id: "shop-pg", name: "shop@postgres", driver: "postgres", port: 5432, user: "shop", options: { ...base.options, schemas: [":current", "tenant_*"] } },
      password: "secret",
    });
  if (!sources.some((x) => x.source.id === "catalog-ddl"))
    sources.push({
      source: { ...base, id: "catalog-ddl", name: "catalog (DDL)", driver: "ddl", host: "", port: null, user: "", database: "", options: { ...base.options, ddlFiles: ["database/ddl"], ddlDialect: "postgres" } },
    });
  virtuals.set("shop-pg", {
    foreignKey: [{ dataSource: "shop-pg", schema: "public", table: "audit_log", columns: ["actor"], refTable: "users", refColumns: ["email"], name: "vfk_audit_log_actor_users" }],
  });
  emitSources();
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
    __forgeDbMock?: { addDemoSources: () => void; addExplorerSources: () => void };
  }
}

if (typeof window !== "undefined") window.__forgeDbMock = { addDemoSources, addExplorerSources };
