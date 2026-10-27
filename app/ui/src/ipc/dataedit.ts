/**
 * Data editor IPC (`forge_dataedit` through `app/src-tauri/src/commands/dataedit.rs` and
 * `db.rs`): local sort/filter of result sets, aggregates, data compare, chart series,
 * foreign-key navigation, the value editor and the DML preview of pending changes.
 */
import { invoke } from "./index";
import type { Cell, Dialect, ForeignKey, RowChange, SortKey } from "./db";

export type FilterOp = "eq" | "ne" | "lt" | "le" | "gt" | "ge" | "isNull" | "isNotNull" | "like" | "notLike" | "in";

/** "Filter by value" and foreign-key navigation conditions. */
export interface ColumnFilter {
  column: string;
  op: FilterOp;
  value?: Cell;
  values?: Cell[];
}

export interface LocalView {
  order?: SortKey[];
  filters?: ColumnFilter[];
  search?: string;
}

export interface Aggregate {
  count: number;
  nulls: number;
  distinct: number;
  numeric: boolean;
  sum?: string;
  avg?: string;
  min?: Cell;
  max?: Cell;
}

export interface ColumnAggregate extends Aggregate {
  column: string;
}

export interface Aggregates {
  total: Aggregate;
  columns: ColumnAggregate[];
}

export interface DataSet {
  columns: string[];
  rows: Cell[][];
}

export type RowStatus = "same" | "changed" | "added" | "removed";

export interface DiffRow {
  status: RowStatus;
  left?: Cell[];
  right?: Cell[];
  /** Indexes into `columns` of the changed cells. */
  changed?: number[];
}

export interface DataDiff {
  columns: string[];
  key: string[];
  rows: DiffRow[];
  same: number;
  changed: number;
  added: number;
  removed: number;
  leftOnlyColumns?: string[];
  rightOnlyColumns?: string[];
  duplicateKeys: number;
}

export type ChartKind = "bar" | "line" | "area" | "pie" | "scatter";
export type ChartAgg = "sum" | "avg" | "min" | "max" | "count";

export interface ChartSpec {
  kind: ChartKind;
  x: string;
  y: string[];
  group?: string | null;
  agg: ChartAgg;
  limit?: number;
}

export interface ChartSeries {
  name: string;
  values: (number | null)[];
  xs?: number[];
}

export interface ChartData {
  kind: ChartKind;
  categories: string[];
  series: ChartSeries[];
  min: number;
  max: number;
  xMin: number;
  xMax: number;
  skipped: number;
  truncated: number;
}

/** Where foreign-key navigation goes: the table and its filter. */
export interface Navigation {
  schema: string;
  table: string;
  filters: ColumnFilter[];
  /** The WHERE text for the filter row. */
  filter: string;
  label: string;
}

export interface IncomingKey {
  schema: string;
  table: string;
  fk: ForeignKey;
}

export type EditorKind =
  | { kind: "text" | "json" | "xml" | "binary" | "number" | "bool" | "date" | "dateTime" | "time" }
  | { kind: "enum"; values: string[]; multiple: boolean };

export interface ValueError {
  message: string;
  line: number;
  column: number;
}

export interface Formatted {
  text: string | null;
  error?: ValueError;
}

export interface BinaryInfo {
  size: number;
  mime?: string;
  lines: { offset: string; hex: string; ascii: string }[];
  truncated: boolean;
  text?: string;
}

export interface KeyChoice {
  columns: string[];
  source: string;
}

export interface ChangePreview {
  statements: string[];
  key?: KeyChoice;
  /** Refused: a read-only data source. */
  blocked?: string;
  /** Ask first: a production data source. */
  confirm?: string;
}

/** The `DEFAULT` marker of the value editor: the column gets its default. */
export const DEFAULT_MARKER = { $default: true } as const;
export const isDefaultMarker = (v: unknown): boolean => typeof v === "object" && v !== null && (v as { $default?: boolean }).$default === true;

export const dataEditApi = {
  /** Row indexes to show after local sort and filter (console results). */
  view: (columns: string[], rows: Cell[][], view: LocalView) => invoke<number[]>("db_data_view", { request: { columns, rows, view } }),
  aggregate: (columns: string[], rows: Cell[][]) => invoke<Aggregates>("db_data_aggregate", { request: { columns, rows } }),
  compare: (left: DataSet, right: DataSet, key: string[]) => invoke<DataDiff>("db_data_compare", { request: { left, right, key } }),
  chart: (columns: string[], rows: Cell[][], spec: ChartSpec) => invoke<ChartData>("db_data_chart", { request: { columns, rows, spec } }),
  navigate: (request: {
    dialect: Dialect;
    schema: string;
    table: string;
    fk: ForeignKey;
    columns: string[];
    row: Cell[];
    direction: "referenced" | "referencing";
  }) => invoke<Navigation>("db_fk_navigate", { request }),
  incoming: (schema: string, table: string, tables: { schema: string; table: string; foreignKeys: ForeignKey[] }[]) =>
    invoke<IncomingKey[]>("db_fk_incoming", { request: { schema, table, tables } }),
  editor: (dataType: string, value: Cell) => invoke<EditorKind>("db_value_editor", { request: { dataType, value } }),
  format: (language: "json" | "xml", text: string, indent = 2) => invoke<Formatted>("db_value_format", { request: { language, text, indent } }),
  binary: (value: Cell, maxBytes?: number) => invoke<BinaryInfo | null>("db_value_binary", { request: { value, maxBytes } }),
  temporal: (kind: EditorKind, text: string) => invoke<string | null>("db_value_temporal", { request: { kind, text } }),
  previewChanges: (id: string, schema: string, table: string, changes: RowChange[]) =>
    invoke<ChangePreview>("db_preview_changes", { id, schema, table, changes }),
};
