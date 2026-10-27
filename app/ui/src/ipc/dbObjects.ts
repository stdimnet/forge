/**
 * Typed client for database objects (`commands/db_objects.rs`): Create/Modify Table
 * previews, Drop / Rename, scripts with the read-only / production checks, Generate DDL,
 * data import (progress on `db://import`), Excel export and native dump / restore tools.
 */
import { invoke, listen } from "./index";
import type { CheckInfo, Dialect, ForeignKey, Guard, Cell } from "./db";
import type { RunStarted } from "./run";

export interface ColumnSpec {
  /** The column's name in the database (existing columns): a changed name is a rename. */
  original?: string;
  name: string;
  dataType: string;
  nullable: boolean;
  default?: string | null;
  comment: string;
  autoIncrement: boolean;
}

export interface KeySpec {
  name: string;
  columns: string[];
}

export interface IndexSpec {
  name: string;
  columns: string[];
  unique: boolean;
  method: string;
}

export interface TableOptions {
  engine: string;
  charset: string;
  collation: string;
  tablespace: string;
  unlogged: boolean;
  temporary: boolean;
  withoutRowid: boolean;
  strict: boolean;
  orderBy: string;
  partitionBy: string;
}

export interface TableSpec {
  schema: string;
  name: string;
  comment: string;
  columns: ColumnSpec[];
  primaryKey?: KeySpec | null;
  uniqueKeys: KeySpec[];
  foreignKeys: ForeignKey[];
  indexes: IndexSpec[];
  checks: CheckInfo[];
  options: TableOptions;
}

export interface OptionField {
  key: keyof TableOptions;
  label: string;
  kind: "text" | "bool";
  choices?: string[];
}

export interface DesignDialect {
  types: string[];
  options: OptionField[];
  foreignKeys: boolean;
  checks: boolean;
  indexes: boolean;
  comments: boolean;
  autoIncrement: boolean;
  fkActions: string[];
  indexMethods: string[];
  defaultType: string;
}

export interface DesignWarning {
  message: string;
  destructive: boolean;
}

export interface DesignPreview {
  sql: string;
  statements: string[];
  warnings: DesignWarning[];
}

export interface DropRequest {
  schema: string;
  name: string;
  kind: string;
  table?: string;
  cascade?: boolean;
  ifExists?: boolean;
  referencing?: { schema: string; table: string; constraint: string }[];
}

export interface ScriptResult {
  executed: number;
  total: number;
  failed?: string;
  error?: string;
  elapsedMs: number;
}

export interface DdlObject {
  schema: string;
  /** `schema`, `table`, `view`, `matview`, `routine`, `sequence`, `type`, `index`, `trigger`. */
  kind: string;
  table?: string;
  name: string;
}

export interface DdlOptions {
  includeDrops: boolean;
  ifNotExists: boolean;
  qualify: boolean;
}

export type SourceFormat = "csv" | "tsv" | "excel" | "json";

export interface ReadOptions {
  format: SourceFormat;
  delimiter?: string | null;
  quote: string;
  header: boolean;
  encoding: string;
  sheet: string;
  skipRows: number;
  trim: boolean;
}

/** A file to import: a path on this machine, or its bytes as base64 (browser mode). */
export interface ImportSource {
  path?: string;
  data?: string;
}

export type ValueKind = "integer" | "decimal" | "boolean" | "date" | "datetime" | "text";

export interface ColumnGuess {
  name: string;
  kind: ValueKind;
  maxLen: number;
  precision: number;
  scale: number;
  nullable: boolean;
}

export interface SourceTable {
  columns: string[];
  rows: (string | null)[][];
  sheets?: string[];
  delimiter?: string;
  encoding?: string;
  warnings?: string[];
}

export interface ImportPreview {
  table: SourceTable;
  guesses: ColumnGuess[];
  /** Target column names for the source columns. */
  names: string[];
  /** SQL types of a new table's columns in the target dialect. */
  types: string[];
  totalRows: number;
}

export interface ColumnMapping {
  source: number;
  target: string;
  kind: ValueKind;
}

export interface ImportPlan {
  schema: string;
  table: string;
  columns: ColumnMapping[];
  create?: TableSpec | null;
  batchSize: number;
  onError: "skip" | "abort";
  emptyAsNull: boolean;
  limit: number;
}

export interface ImportProgress {
  importId: string;
  done: number;
  total: number;
  inserted: number;
  skipped: number;
}

export interface ImportSummary {
  inserted: number;
  skipped: number;
  errors: { row: number; message: string }[];
  cancelled: boolean;
  aborted: boolean;
  created: boolean;
  elapsedMs: number;
}

export type DumpTool = "mysqldump" | "mysql" | "pgDump" | "pgRestore" | "psql" | "sqlite3" | "sqlcmd";
export type DumpAction = "dump" | "restore";

export interface ToolChoice {
  tool: DumpTool;
  action: DumpAction;
  label: string;
}

export interface DumpRequest {
  tool: DumpTool;
  action: DumpAction;
  program: string;
  file: string;
  schemas: string[];
  tables: string[];
  structure: boolean;
  data: boolean;
  dropObjects: boolean;
  createDatabase: boolean;
  singleTransaction: boolean;
  format: string;
  noOwner: boolean;
  jobs: number;
  extra: string;
}

export const IMPORT_EVENT = "db://import";

export const dbObjectsApi = {
  designInfo: (dialect: Dialect) => invoke<DesignDialect>("db_design_info", { dialect }),
  designLoad: (id: string, schema: string, table: string) => invoke<TableSpec>("db_design_load", { id, schema, table }),
  designPreview: (dialect: Dialect, table: TableSpec, old?: TableSpec | null, options?: { ifNotExists: boolean }) =>
    invoke<DesignPreview>("db_design_preview", { dialect, table, old: old ?? null, options }),
  dropSql: (dialect: Dialect, requests: DropRequest[]) => invoke<string>("db_drop_sql", { dialect, requests }),
  renameSql: (dialect: Dialect, schema: string, table: string, to: string, column?: string) =>
    invoke<string>("db_rename_sql", { dialect, schema, table, column: column ?? null, to }),
  guard: (id: string, sql: string) => invoke<Guard>("db_objects_guard", { id, sql }),
  runScript: (id: string, sql: string, confirmed?: boolean) => invoke<ScriptResult>("db_run_script", { id, sql, confirmed }),
  generateDdl: (id: string, objects: DdlObject[], options: DdlOptions, path?: string) =>
    invoke<string>("db_generate_ddl", { id, objects, options, path: path ?? null }),
  importPreview: (source: ImportSource, options: ReadOptions, rows?: number, dialect?: Dialect) =>
    invoke<ImportPreview>("db_import_preview", { source, options, rows, dialect: dialect ?? null }),
  importRun: (args: { id: string; importId: string; source: ImportSource; options: ReadOptions; plan: ImportPlan; confirmed?: boolean }) =>
    invoke<ImportSummary>("db_import_run", args),
  importCancel: (importId: string) => invoke<void>("db_import_cancel", { importId }),
  onImport: (cb: (p: ImportProgress) => void) => listen<ImportProgress>(IMPORT_EVENT, cb),
  exportXlsx: (sheet: string, columns: string[], rows: Cell[][], path?: string) =>
    invoke<string | null>("db_export_xlsx", { sheet, columns, rows, path: path ?? null }),
  dumpTools: (id: string) => invoke<ToolChoice[]>("db_dump_tools", { id }),
  dumpCommand: (id: string, request: DumpRequest) => invoke<string>("db_dump_command", { id, request }),
  /** Before a restore: throws on read-only data sources, returns the production warning. */
  restoreGuard: (id: string) => invoke<string | null>("db_restore_guard", { id }),
  dumpStart: (id: string, request: DumpRequest, runId: string, confirmed?: boolean) =>
    invoke<RunStarted>("db_dump_start", { id, request, runId, confirmed }),
};

export function defaultReadOptions(format: SourceFormat = "csv"): ReadOptions {
  return { format, delimiter: null, quote: '"', header: true, encoding: "", sheet: "", skipRows: 0, trim: false };
}

export function defaultTableOptions(): TableOptions {
  return { engine: "", charset: "", collation: "", tablespace: "", unlogged: false, temporary: false, withoutRowid: false, strict: false, orderBy: "", partitionBy: "" };
}

export function emptyTable(schema: string, name = "new_table"): TableSpec {
  return { schema, name, comment: "", columns: [], primaryKey: null, uniqueKeys: [], foreignKeys: [], indexes: [], checks: [], options: defaultTableOptions() };
}

export function defaultDumpRequest(tool: DumpTool, action: DumpAction): DumpRequest {
  return {
    tool,
    action,
    program: "",
    file: "",
    schemas: [],
    tables: [],
    structure: true,
    data: true,
    dropObjects: false,
    createDatabase: false,
    singleTransaction: true,
    format: "plain",
    noOwner: false,
    jobs: 0,
    extra: "",
  };
}

/** The format of a file name, as `SourceFormat::from_path` decides it. */
export function formatOf(name: string): SourceFormat {
  const ext = name.split(".").pop()?.toLowerCase() ?? "";
  if (ext === "tsv" || ext === "tab") return "tsv";
  if (["xlsx", "xlsm", "xls", "xlsb", "ods"].includes(ext)) return "excel";
  if (["json", "ndjson", "jsonl"].includes(ext)) return "json";
  return "csv";
}
