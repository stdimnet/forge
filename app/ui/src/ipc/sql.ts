/**
 * SQL editor IPC (see app/src-tauri/src/commands/sql.rs, logic in crates/forge-sqlcode):
 * dialects per file, the SQL inspections, navigation, the formatter, user parameters, the
 * query history and Run SQL Script. Offsets are UTF-16 (editor positions).
 */
import { invoke, listen } from "./index";

export type SqlDialect = "generic" | "mysql" | "postgres" | "sqlite" | "sqlserver" | "oracle" | "clickhouse";

export const SQL_DIALECTS: { id: SqlDialect; label: string }[] = [
  { id: "generic", label: "Generic SQL" },
  { id: "mysql", label: "MySQL" },
  { id: "postgres", label: "PostgreSQL" },
  { id: "sqlite", label: "SQLite" },
  { id: "sqlserver", label: "SQL Server" },
  { id: "oracle", label: "Oracle" },
  { id: "clickhouse", label: "ClickHouse" },
];

export const dialectLabel = (d: SqlDialect | undefined): string => SQL_DIALECTS.find((x) => x.id === d)?.label ?? "Generic SQL";

export interface DialectInfo {
  id: SqlDialect;
  label: string;
  keywords: string[];
  types: string[];
}

/** A SQL Dialects mapping: a file or folder, its dialect and the data source names resolve against. */
export interface DialectMapping {
  path: string;
  dialect?: SqlDialect | "";
  dataSource?: string;
}

export interface FileSql {
  dialect: SqlDialect;
  /** `dataSource` (consoles), `mapping`, `project` or `global`. */
  source: "dataSource" | "mapping" | "project" | "global";
  dataSourceId?: string;
}

/** A file (path) or console (dataSourceId + dialect) and its text. */
export interface SqlTarget {
  text: string;
  path?: string;
  dataSourceId?: string;
  dialect?: SqlDialect;
  defaultSchema?: string;
}

export interface SqlEdit {
  from: number;
  to: number;
  text: string;
}

export interface SqlFix {
  title: string;
  edits: SqlEdit[];
  /** Selection after applying. */
  select?: [number, number];
}

export interface SqlProblem {
  /** Inspection id (`SqlResolve`). */
  id: string;
  from: number;
  to: number;
  message: string;
  severity: "error" | "warning" | "weak";
  unnecessary?: boolean;
  fixes?: SqlFix[];
}

export interface AnalyzeAnswer {
  dialect: SqlDialect;
  problems: SqlProblem[];
  hasModel: boolean;
}

export interface SqlSymbol {
  name: string;
  kind: "table" | "view" | "column" | "schema" | "routine" | "alias" | "cte" | "selectAlias";
  /** A definition in the script (name range). */
  local?: [number, number];
  schema?: string;
  table?: string;
  column?: string;
  routine?: string;
}

export interface DeclarationAnswer {
  symbol: SqlSymbol;
  /** Where a DDL data source's files define it. */
  ddl?: { path: string; line: number; character: number };
}

export interface Occurrence {
  from: number;
  to: number;
  declaration: boolean;
}

export interface UsagesAnswer {
  name: string;
  occurrences: Occurrence[];
}

export interface RenameAnswer {
  name: string;
  occurrences: Occurrence[];
  error?: string;
}

export interface SqlParam {
  name: string;
  from: number;
  to: number;
}

export interface ParamsAnswer {
  params: SqlParam[];
  names: string[];
}

export interface HistoryEntry {
  sql: string;
  /** Unix milliseconds. */
  time: number;
  durationMs: number;
  rows?: number | null;
  dataSource: string;
  console: string;
  error?: string | null;
}

export interface HistoryQuery {
  text?: string;
  console?: string;
  limit?: number;
}

export interface RunFileRequest {
  runId: string;
  path: string;
  dataSourceId: string;
  stopOnError: boolean;
  confirmed?: boolean;
}

export interface RunFileEvent {
  runId: string;
  kind: "started" | "statement" | "finished";
  index: number;
  total: number;
  sql?: string;
  line: number;
  ok: boolean;
  error?: string;
  rows?: number;
  elapsedMs: number;
}

export interface RunFileSummary {
  total: number;
  executed: number;
  failed: number;
  cancelled: boolean;
  elapsedMs: number;
  needsConfirmation?: string;
}

export const RUN_FILE_EVENT = "sql://runFile";

export const sqlApi = {
  dialects: () => invoke<DialectInfo[]>("sql_dialects"),
  fileInfo: (request: SqlTarget) => invoke<FileSql>("sql_file_info", { request }),
  analyze: (request: SqlTarget) => invoke<AnalyzeAnswer>("sql_analyze", { request }),
  declaration: (request: SqlTarget, offset: number) => invoke<DeclarationAnswer | null>("sql_declaration", { request: { ...request, offset } }),
  usages: (request: SqlTarget, offset: number) => invoke<UsagesAnswer | null>("sql_usages", { request: { ...request, offset } }),
  rename: (request: SqlTarget, offset: number) => invoke<RenameAnswer>("sql_rename", { request: { ...request, offset } }),
  format: (request: SqlTarget) => invoke<string>("sql_format", { request }),
  params: (text: string, dialect: SqlDialect) => invoke<ParamsAnswer>("sql_params", { request: { text, dialect } }),
  substitute: (text: string, dialect: SqlDialect, values: Record<string, string>) => invoke<string>("sql_substitute", { request: { text, dialect, values } }),
  history: (dataSourceId: string, query: HistoryQuery = {}) => invoke<HistoryEntry[]>("sql_history_list", { dataSourceId, query }),
  addHistory: (entry: HistoryEntry) => invoke<void>("sql_history_add", { entry }),
  clearHistory: (dataSourceId: string) => invoke<void>("sql_history_clear", { dataSourceId }),
  runFile: (request: RunFileRequest) => invoke<RunFileSummary>("sql_run_file", { request }),
  cancelRunFile: (runId: string) => invoke<void>("sql_run_file_cancel", { runId }),
  onRunFile: (cb: (e: RunFileEvent) => void) => listen<RunFileEvent>(RUN_FILE_EVENT, cb),
};
