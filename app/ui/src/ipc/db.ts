/** Typed client for the database tools backend (`crates/forge-db`, `commands/db.rs`). */
import { invoke, listen } from "./index";

export type Driver = "mysql" | "mariadb" | "postgres" | "sqlite";
export type Dialect = "mysql" | "postgres" | "sqlite";
export type Scope = "project" | "global";
export type SslMode = "disable" | "prefer" | "require";

export interface DataSourceOptions {
  readOnly: boolean;
  autoCommit: boolean;
  sslMode: SslMode;
  schemas: string[];
  connectTimeoutSecs: number;
  initSql: string;
  savePassword: boolean;
  pageSize?: number | null;
  color?: string | null;
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
  summary: string;
}

export interface TestResult {
  ok: boolean;
  driver: string;
  serverVersion?: string;
  latencyMs: number;
  error?: string;
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
  kind: "table" | "view";
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
}

export interface QueryPage {
  columns: ColumnMeta[];
  rows: Cell[][];
  hasMore: boolean;
  offset: number;
  total?: number;
  elapsedMs: number;
  sql: string;
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
  autoCommit: boolean;
  txOpen: boolean;
  running: boolean;
}

export interface DetectedDataSource {
  source: DataSource;
  hasPassword: boolean;
  origin: string;
}

export type ExportFormat = "csv" | "json" | "sql";

export const DATA_SOURCES_EVENT = "db://dataSources";
export const SESSION_EVENT = "db://session";

export const dbApi = {
  list: () => invoke<DataSourceInfo[]>("db_data_sources_list"),
  save: (source: DataSource, password?: string) =>
    invoke<DataSourceInfo>("db_data_source_save", { source, password }),
  remove: (id: string) => invoke<void>("db_data_source_delete", { id }),
  test: (source: DataSource, password?: string) => invoke<TestResult>("db_test_connection", { source, password }),
  disconnect: (id: string) => invoke<void>("db_disconnect", { id }),
  schemas: (id: string) => invoke<SchemaName[]>("db_schemas", { id }),
  introspect: (id: string, schema: string) => invoke<SchemaInfo>("db_introspect", { id, schema }),
  tableInfo: (id: string, schema: string, table: string) => invoke<TableInfo>("db_table_info", { id, schema, table }),
  tableDdl: (id: string, schema: string, table: string) => invoke<string>("db_table_ddl", { id, schema, table }),
  tableData: (id: string, request: TableDataRequest) => invoke<QueryPage>("db_table_data", { id, request }),
  applyChanges: (id: string, schema: string, table: string, changes: RowChange[]) =>
    invoke<ApplyResult>("db_apply_changes", { id, schema, table, changes }),
  openSession: (id: string) => invoke<SessionInfo>("db_session_open", { id }),
  closeSession: (sessionId: string) => invoke<void>("db_session_close", { sessionId }),
  execute: (sessionId: string, sql: string, pageSize?: number) =>
    invoke<StatementResult[]>("db_execute", { sessionId, sql, pageSize }),
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
};

/** A statement of a script (`forge_sql::split::Statement`). */
export interface SqlStatement {
  text: string;
  start: number;
  end: number;
}

/** Binary cells arrive as `{ $hex }`. */
export const isBinary = (v: Cell): v is { $hex: string } => typeof v === "object" && v !== null && "$hex" in v;

export const dialectOf = (d: Driver): Dialect => (d === "mariadb" ? "mysql" : d);

export const DRIVER_LABELS: Record<Driver, string> = {
  mysql: "MySQL",
  mariadb: "MariaDB",
  postgres: "PostgreSQL",
  sqlite: "SQLite",
};

export const DEFAULT_PORTS: Partial<Record<Driver, number>> = { mysql: 3306, mariadb: 3306, postgres: 5432 };

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
