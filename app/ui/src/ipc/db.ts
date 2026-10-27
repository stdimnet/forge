/** Typed client for the database tools backend (`crates/forge-db`, `commands/db.rs`). */
import { invoke, listen } from "./index";

export type Driver = "mysql" | "mariadb" | "postgres" | "sqlite" | "sqlserver" | "clickhouse" | "oracle" | "redis" | "mongodb";
export type Dialect = "mysql" | "postgres" | "sqlite" | "sqlserver" | "clickhouse" | "oracle" | "redis" | "mongo";
export type Scope = "project" | "global";
export type SslMode = "disable" | "prefer" | "require" | "verify-ca" | "verify-full";
/** Relational: schemas and tables; keyValue: Redis; document: MongoDB. */
export type DataModel = "relational" | "keyValue" | "document";
export type SshAuth = "password" | "keyPair" | "agent";

/** Certificate files of the SSH/SSL tab (the key passphrase is in the keychain). */
export interface SslFiles {
  caFile?: string;
  certFile?: string;
  keyFile?: string;
}

/** The SSH tunnel of a data source: a saved SSH configuration or ad-hoc settings. */
export interface SshTunnel {
  enabled: boolean;
  configurationId?: string | null;
  host: string;
  port: number;
  user: string;
  auth: SshAuth;
  keyFile?: string | null;
}

export interface DataSourceOptions {
  readOnly: boolean;
  autoCommit: boolean;
  sslMode: SslMode;
  ssl?: SslFiles;
  ssh?: SshTunnel;
  schemas: string[];
  connectTimeoutSecs: number;
  initSql: string;
  savePassword: boolean;
  pageSize?: number | null;
  /** `green` (development), `yellow` (staging), `red` (production), or another colour. */
  color?: string | null;
  /** All consoles share one connection. */
  singleSession?: boolean;
}

export interface DataSource {
  id: string;
  name: string;
  driver: Driver;
  host: string;
  port?: number | null;
  database: string;
  user: string;
  file: string;
  comment: string;
  options: DataSourceOptions;
  scope: Scope;
}

export interface DataSourceInfo extends DataSource {
  hasPassword: boolean;
  hasSshSecret?: boolean;
  hasSslPassphrase?: boolean;
  summary: string;
  /** Set when the data source is reached through an SSH connection: its own tunnel or the
   * remote project's (`deploy@shop.example.com`). */
  via?: string;
  model?: DataModel;
  /** Consoles can use manual transactions. */
  transactions?: boolean;
  /** Production colour: unfiltered changes ask first. */
  production?: boolean;
}

/** SSH/SSL secrets: omitted keeps the stored one, `""` clears it. */
export interface SecretUpdate {
  ssh?: string;
  sslKey?: string;
}

export interface HostKeyInfo {
  host: string;
  port: number;
  keyType: string;
  fingerprint: string;
  status: string;
}

export interface TestResult {
  ok: boolean;
  driver: string;
  serverVersion?: string;
  latencyMs: number;
  error?: string;
  /** The SSH host's key is unknown or changed: trust it and test again. */
  hostKey?: HostKeyInfo;
}

export interface SchemaName {
  name: string;
  current: boolean;
}

export interface ColumnInfo {
  name: string;
  dataType: string;
  nullable: boolean;
  default?: string | null;
  autoIncrement: boolean;
  comment?: string;
}

export interface ForeignKey {
  name: string;
  columns: string[];
  refSchema: string;
  refTable: string;
  refColumns: string[];
  onUpdate?: string;
  onDelete?: string;
}

export interface IndexInfo {
  name: string;
  columns: string[];
  unique: boolean;
  primary: boolean;
}

export interface TableInfo {
  name: string;
  kind: "table" | "view" | "collection";
  comment?: string;
  columns: ColumnInfo[];
  primaryKey: string[];
  foreignKeys: ForeignKey[];
  indexes: IndexInfo[];
  rowEstimate?: number;
}

export interface RoutineInfo {
  name: string;
  kind: string;
  returns?: string;
}

export interface SchemaInfo {
  name: string;
  tables: TableInfo[];
  routines: RoutineInfo[];
}

/** A cell: JSON scalars; big numbers/decimals/dates as strings; binary as `{ $hex }`. */
export type Cell = null | boolean | number | string | { $hex: string };

export interface ColumnMeta {
  name: string;
  typeName: string;
}

export interface StatementResult {
  index: number;
  sql: string;
  start: number;
  end: number;
  columns: ColumnMeta[];
  rows: Cell[][];
  rowsAffected: number;
  hasMore: boolean;
  returnsRows: boolean;
  elapsedMs: number;
  error?: string;
  /** MongoDB: the documents of the rows (relaxed extended JSON). */
  documents?: unknown[];
}

export interface QueryPage {
  columns: ColumnMeta[];
  rows: Cell[][];
  hasMore: boolean;
  offset: number;
  total?: number;
  elapsedMs: number;
  sql: string;
  documents?: unknown[];
}

export interface SortKey {
  column: string;
  desc: boolean;
}

export interface TableDataRequest {
  schema: string;
  table: string;
  filter?: string;
  order?: SortKey[];
  offset?: number;
  limit?: number;
}

export type ChangeKind = "insert" | "update" | "delete";

export interface RowChange {
  kind: ChangeKind;
  key: Record<string, Cell>;
  values: Record<string, Cell>;
}

export interface ApplyResult {
  statements: string[];
  rowsAffected: number;
}

export interface SessionInfo {
  id: string;
  dataSourceId: string;
  /** `console` or `shared`. */
  name?: string;
  shared?: boolean;
  /** Consoles using the session. */
  consoles?: number;
  autoCommit: boolean;
  txOpen: boolean;
  /** The driver supports manual transactions. */
  transactions?: boolean;
  running: boolean;
  /** Unix milliseconds. */
  openedAt?: number;
  txStartedAt?: number;
  txStatements?: number;
  lastSql?: string;
}

/** What a console must do before running a script. */
export interface Guard {
  /** Refused: a write on a read-only data source. */
  blocked?: string;
  /** Ask first: an unfiltered change on a production data source. */
  confirm?: string;
}

export interface RedisKey {
  key: string;
  type: string;
  /** Milliseconds; -1 without expiry. */
  ttlMs: number;
}

export interface RedisKeys {
  db: number;
  pattern: string;
  keys: RedisKey[];
  truncated: boolean;
}

export interface RedisValue {
  key: string;
  type: string;
  ttlMs: number;
  length: number;
  value: unknown;
  truncated: boolean;
}

export interface DetectedDataSource {
  source: DataSource;
  hasPassword: boolean;
  origin: string;
}

export type ExportFormat = "csv" | "json" | "sql";

export const DATA_SOURCES_EVENT = "db://dataSources";
export const SESSION_EVENT = "db://session";
export const SESSIONS_EVENT = "db://sessions";

export const dbApi = {
  list: () => invoke<DataSourceInfo[]>("db_data_sources_list"),
  save: (source: DataSource, password?: string, secrets?: SecretUpdate) =>
    invoke<DataSourceInfo>("db_data_source_save", { source, password, secrets }),
  remove: (id: string) => invoke<void>("db_data_source_delete", { id }),
  test: (source: DataSource, password?: string, secrets?: SecretUpdate, acceptHostKey?: string) =>
    invoke<TestResult>("db_test_connection", { source, password, secrets, acceptHostKey }),
  disconnect: (id: string) => invoke<void>("db_disconnect", { id }),
  schemas: (id: string) => invoke<SchemaName[]>("db_schemas", { id }),
  introspect: (id: string, schema: string) => invoke<SchemaInfo>("db_introspect", { id, schema }),
  tableInfo: (id: string, schema: string, table: string) => invoke<TableInfo>("db_table_info", { id, schema, table }),
  tableDdl: (id: string, schema: string, table: string) => invoke<string>("db_table_ddl", { id, schema, table }),
  tableData: (id: string, request: TableDataRequest) => invoke<QueryPage>("db_table_data", { id, request }),
  applyChanges: (id: string, schema: string, table: string, changes: RowChange[]) =>
    invoke<ApplyResult>("db_apply_changes", { id, schema, table, changes }),
  /** A console session: its own connection, or with `shared` the data source's shared one. */
  openSession: (id: string, shared?: boolean) => invoke<SessionInfo>("db_session_open", { id, shared }),
  closeSession: (sessionId: string) => invoke<void>("db_session_close", { sessionId }),
  sessions: () => invoke<SessionInfo[]>("db_sessions_list"),
  terminateSession: (sessionId: string) => invoke<void>("db_session_terminate", { sessionId }),
  guard: (id: string, sql: string) => invoke<Guard>("db_guard", { id, sql }),
  execute: (sessionId: string, sql: string, pageSize?: number, confirmed?: boolean) =>
    invoke<StatementResult[]>("db_execute", { sessionId, sql, pageSize, confirmed }),
  redisKeys: (id: string, schema: string, pattern: string, limit?: number) =>
    invoke<RedisKeys>("db_redis_keys", { id, schema, pattern, limit }),
  redisValue: (id: string, schema: string, key: string) => invoke<RedisValue>("db_redis_value", { id, schema, key }),
  fetch: (sessionId: string, sql: string, offset: number, limit?: number) =>
    invoke<QueryPage>("db_fetch", { sessionId, sql, offset, limit }),
  setAutoCommit: (sessionId: string, autoCommit: boolean) =>
    invoke<SessionInfo>("db_set_auto_commit", { sessionId, autoCommit }),
  commit: (sessionId: string) => invoke<SessionInfo>("db_commit", { sessionId }),
  rollback: (sessionId: string) => invoke<SessionInfo>("db_rollback", { sessionId }),
  cancel: (sessionId: string) => invoke<void>("db_cancel", { sessionId }),
  detectEnv: () => invoke<DetectedDataSource[]>("db_detect_env"),
  addFromEnv: () => invoke<DataSourceInfo[]>("db_add_from_env"),
  export: (args: {
    format: ExportFormat;
    dialect: Dialect;
    table?: string;
    columns: string[];
    rows: Cell[][];
    path?: string;
  }) => invoke<string>("db_export", args),
  /** What Ctrl+Enter runs: the selection or the statement at the caret (editor offsets). */
  statementAt: (sql: string, from: number, to: number, dialect: Dialect) =>
    invoke<SqlStatement | null>("db_statement_at", { sql, from, to, dialect }),
  onDataSources: (cb: (list: DataSourceInfo[]) => void) => listen<DataSourceInfo[]>(DATA_SOURCES_EVENT, cb),
  onSession: (cb: (s: SessionInfo) => void) => listen<SessionInfo>(SESSION_EVENT, cb),
  onSessions: (cb: (list: SessionInfo[]) => void) => listen<SessionInfo[]>(SESSIONS_EVENT, cb),
};

/** A statement of a script (`forge_sql::split::Statement`). */
export interface SqlStatement {
  text: string;
  start: number;
  end: number;
}

/** Binary cells arrive as `{ $hex }`. */
export const isBinary = (v: Cell): v is { $hex: string } => typeof v === "object" && v !== null && "$hex" in v;

export const dialectOf = (d: Driver): Dialect => (d === "mariadb" ? "mysql" : d === "mongodb" ? "mongo" : d);

export const DRIVER_LABELS: Record<Driver, string> = {
  mysql: "MySQL",
  mariadb: "MariaDB",
  postgres: "PostgreSQL",
  sqlite: "SQLite",
  sqlserver: "SQL Server",
  clickhouse: "ClickHouse",
  oracle: "Oracle",
  redis: "Redis",
  mongodb: "MongoDB",
};

/** Drivers in the order of the "+" menu. */
export const DRIVERS: Driver[] = ["mysql", "mariadb", "postgres", "sqlite", "sqlserver", "clickhouse", "oracle", "redis", "mongodb"];

export const DEFAULT_PORTS: Partial<Record<Driver, number>> = {
  mysql: 3306,
  mariadb: 3306,
  postgres: 5432,
  sqlserver: 1433,
  clickhouse: 8123,
  oracle: 1521,
  redis: 6379,
  mongodb: 27017,
};

export const modelOf = (d: Driver): DataModel => (d === "redis" ? "keyValue" : d === "mongodb" ? "document" : "relational");

/** Drivers whose consoles can switch to manual transactions. */
export const supportsTransactions = (d: Driver): boolean => ["mysql", "mariadb", "postgres", "sqlite", "sqlserver"].includes(d);

/** Data source colours (Options tab): label, tint and accent CSS variables. */
export const DS_COLORS: Record<string, { label: string; tint: string; accent: string }> = {
  green: { label: "Green (development)", tint: "var(--file-color-green)", accent: "var(--success)" },
  yellow: { label: "Yellow (staging)", tint: "var(--file-color-yellow)", accent: "var(--warning)" },
  red: { label: "Red (production)", tint: "var(--file-color-rose)", accent: "var(--danger)" },
  blue: { label: "Blue", tint: "var(--file-color-blue)", accent: "var(--info)" },
};

export const defaultSsh = (): SshTunnel => ({ enabled: false, configurationId: null, host: "", port: 22, user: "", auth: "password", keyFile: null });

export function defaultOptions(): DataSourceOptions {
  return {
    readOnly: false,
    autoCommit: true,
    sslMode: "prefer",
    schemas: [],
    connectTimeoutSecs: 10,
    initSql: "",
    savePassword: true,
    pageSize: null,
    color: null,
  };
}
