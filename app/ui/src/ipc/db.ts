/** Typed client for the database tools backend (`crates/forge-db`, `commands/db.rs`). */
import { invoke, listen } from "./index";

export type Driver = "mysql" | "mariadb" | "postgres" | "sqlite" | "sqlserver" | "clickhouse" | "oracle" | "redis" | "mongodb" | "ddl";
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
  /** DDL data sources: project-relative SQL files and folders, parsed in order. */
  ddlFiles?: string[];
  /** DDL data sources: the files' dialect (PostgreSQL when not set). */
  ddlDialect?: Dialect | null;
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
  /** Passes the data source's schema filter (shown and introspected). */
  visible?: boolean;
}

export interface ColumnInfo {
  name: string;
  dataType: string;
  nullable: boolean;
  default?: string | null;
  autoIncrement: boolean;
  comment?: string;
  /** A virtual column defined in the project. */
  virtual?: boolean;
  /** Expression of a virtual or generated column. */
  expression?: string | null;
}

export interface ForeignKey {
  name: string;
  columns: string[];
  refSchema: string;
  refTable: string;
  refColumns: string[];
  onUpdate?: string;
  onDelete?: string;
  /** A virtual foreign key defined in the project. */
  virtual?: boolean;
}

export interface IndexInfo {
  name: string;
  columns: string[];
  unique: boolean;
  primary: boolean;
}

export interface CheckInfo {
  name: string;
  expression: string;
}

export interface TriggerInfo {
  name: string;
  /** `BEFORE`, `AFTER` or `INSTEAD OF`. */
  timing?: string;
  /** `INSERT OR UPDATE`, … */
  events?: string;
  definition?: string;
}

export interface TableInfo {
  name: string;
  /** `matview`: a materialized view. */
  kind: "table" | "view" | "matview" | "collection";
  comment?: string;
  columns: ColumnInfo[];
  primaryKey: string[];
  foreignKeys: ForeignKey[];
  indexes: IndexInfo[];
  checks?: CheckInfo[];
  triggers?: TriggerInfo[];
  rowEstimate?: number;
  /** The query of a view. */
  definition?: string;
}

export interface RoutineArg {
  name?: string;
  dataType: string;
  /** `OUT`, `INOUT`, `VARIADIC`; empty for `IN`. */
  mode?: string;
}

export interface RoutineInfo {
  name: string;
  /** `function`, `procedure`, `aggregate` or `package`. */
  kind: string;
  returns?: string;
  arguments?: RoutineArg[];
  language?: string;
  definition?: string;
}

export interface SequenceInfo {
  name: string;
  dataType?: string;
  start?: number;
  increment?: number;
}

export interface TypeInfo {
  name: string;
  /** `enum`, `composite`, `domain` or `type`. */
  kind: string;
  labels?: string[];
  definition?: string;
}

export interface SchemaInfo {
  name: string;
  tables: TableInfo[];
  routines: RoutineInfo[];
  sequences?: SequenceInfo[];
  types?: TypeInfo[];
}

export interface RoleInfo {
  name: string;
  /** `role` or `user`. */
  kind: string;
  superuser?: boolean;
  canLogin?: boolean;
}

export interface ModelWarning {
  /** Project-relative DDL file. */
  file?: string;
  line?: number;
  message: string;
}

/** The explorer model of a data source (cached; the tree shows it without connecting). */
export interface DbModel {
  dialect: Dialect;
  /** The connected database for engines with databases above schemas. */
  catalog?: string;
  /** Other databases on the server. */
  catalogs?: string[];
  schemas: SchemaName[];
  /** Introspected (visible) schemas by name. */
  loaded: Record<string, SchemaInfo>;
  roles?: RoleInfo[];
  refreshedAt: number;
  warnings?: ModelWarning[];
}

/** A tree node for Quick Documentation. */
export interface ObjectRef {
  kind: string;
  table?: string;
  name: string;
}

export interface ObjectDoc {
  kind: string;
  name: string;
  location: string;
  comment?: string;
  ddl: string;
  properties: { name: string; value: string }[];
}

export interface VirtualForeignKey {
  dataSource?: string;
  schema: string;
  table: string;
  name?: string;
  columns: string[];
  refSchema?: string;
  refTable: string;
  refColumns: string[];
}

export interface VirtualColumn {
  dataSource?: string;
  schema: string;
  table: string;
  name: string;
  expression: string;
  type?: string;
  comment?: string;
}

/** `.forge/database/virtual.toml` of one data source. */
export interface VirtualObjects {
  foreignKey?: VirtualForeignKey[];
  column?: VirtualColumn[];
}

export type SearchMode = "contains" | "exact" | "regex";

export interface SearchOptions {
  text: string;
  mode?: SearchMode;
  caseSensitive?: boolean;
  /** Also search numbers, dates and other non-text columns. */
  allColumns?: boolean;
  /** Rows per table (default 100). */
  limit?: number;
}

export interface SearchScope {
  dataSource: string;
  /** Empty: the visible schemas. */
  schemas?: string[];
  /** Table names or patterns; empty: all. */
  tables?: string[];
}

export interface SearchRequest extends SearchOptions {
  scopes: SearchScope[];
}

export interface TableMatches {
  dataSource: string;
  schema: string;
  table: string;
  columns: string[];
  rows: Cell[][];
  /** Matching cell indexes per row. */
  matches: number[][];
  truncated: boolean;
  sql: string;
  error?: string;
}

export interface SearchProgress {
  searchId: string;
  done: number;
  total: number;
  current: string;
  result?: TableMatches;
}

export interface SearchSummary {
  tables: number;
  matchedTables: number;
  rows: number;
  errors: number;
  cancelled: boolean;
  elapsedMs: number;
}

/** A cell: JSON scalars; big numbers/decimals/dates as strings; binary as `{ $hex }`; the
 * value editor's "Set DEFAULT" as `{ $default: true }` (pending changes only). */
export type Cell = null | boolean | number | string | { $hex: string } | { $default: true };

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
  /** The typed WHERE condition (checked by the backend: one read-only condition). */
  filter?: string;
  /** "Filter by value" conditions (`forge_dataedit::ColumnFilter`). */
  columnFilters?: import("./dataedit").ColumnFilter[];
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
export const MODEL_EVENT = "db://modelChanged";
export const SEARCH_EVENT = "db://search";

export const dbApi = {
  list: () => invoke<DataSourceInfo[]>("db_data_sources_list"),
  save: (source: DataSource, password?: string, secrets?: SecretUpdate) =>
    invoke<DataSourceInfo>("db_data_source_save", { source, password, secrets }),
  remove: (id: string) => invoke<void>("db_data_source_delete", { id }),
  test: (source: DataSource, password?: string, secrets?: SecretUpdate, acceptHostKey?: string) =>
    invoke<TestResult>("db_test_connection", { source, password, secrets, acceptHostKey }),
  disconnect: (id: string) => invoke<void>("db_disconnect", { id }),
  schemas: (id: string) => invoke<SchemaName[]>("db_schemas", { id }),
  /** The explorer model: cached unless `refresh` (which reconnects and reloads). */
  model: (id: string, refresh?: boolean) => invoke<DbModel>("db_model", { id, refresh }),
  objectDoc: (id: string, schema: string, object: ObjectRef) => invoke<ObjectDoc>("db_object_doc", { id, schema, object }),
  virtualObjects: (id: string) => invoke<VirtualObjects>("db_virtual_objects", { id }),
  saveVirtualObjects: (id: string, objects: VirtualObjects) => invoke<void>("db_virtual_objects_save", { id, objects }),
  /** The schema filter applied to `schemas` (the Schemas tab preview). */
  matchSchemas: (patterns: string[], schemas: SchemaName[]) => invoke<SchemaName[]>("db_match_schemas", { patterns, schemas }),
  search: (searchId: string, request: SearchRequest) => invoke<SearchSummary>("db_search", { searchId, request }),
  cancelSearch: (searchId: string) => invoke<void>("db_search_cancel", { searchId }),
  onModelChanged: (cb: (ids: string[]) => void) => listen<string[]>(MODEL_EVENT, cb),
  onSearch: (cb: (p: SearchProgress) => void) => listen<SearchProgress>(SEARCH_EVENT, cb),
  introspect: (id: string, schema: string) => invoke<SchemaInfo>("db_introspect", { id, schema }),
  tableInfo: (id: string, schema: string, table: string) => invoke<TableInfo>("db_table_info", { id, schema, table }),
  tableDdl: (id: string, schema: string, table: string) => invoke<string>("db_table_ddl", { id, schema, table }),
  tableData: (id: string, request: TableDataRequest) => invoke<QueryPage>("db_table_data", { id, request }),
  /** Submit pending changes; production data sources need `confirmed`. */
  applyChanges: (id: string, schema: string, table: string, changes: RowChange[], confirmed?: boolean) =>
    invoke<ApplyResult>("db_apply_changes", { id, schema, table, changes, confirmed }),
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

export const dialectOf = (d: Driver, ddlDialect?: Dialect | null): Dialect =>
  d === "mariadb" ? "mysql" : d === "mongodb" ? "mongo" : d === "ddl" ? (ddlDialect ?? "postgres") : d;

/** The dialect of a data source (DDL data sources: their files' dialect). */
export const sourceDialect = (ds: Pick<DataSource, "driver" | "options"> | undefined): Dialect =>
  dialectOf(ds?.driver ?? "mysql", ds?.options.ddlDialect);

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
  ddl: "DDL Data Source",
};

/** Drivers in the order of the "+" menu. */
export const DRIVERS: Driver[] = ["mysql", "mariadb", "postgres", "sqlite", "sqlserver", "clickhouse", "oracle", "redis", "mongodb", "ddl"];

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
  green: { label: "Green (development)", tint: "color-mix(in srgb, var(--success) 14%, transparent)", accent: "var(--success)" },
  yellow: { label: "Yellow (staging)", tint: "color-mix(in srgb, var(--warning) 14%, transparent)", accent: "var(--warning)" },
  red: { label: "Red (production)", tint: "color-mix(in srgb, var(--danger) 20%, transparent)", accent: "var(--danger)" },
  blue: { label: "Blue", tint: "color-mix(in srgb, var(--info) 14%, transparent)", accent: "var(--info)" },
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
