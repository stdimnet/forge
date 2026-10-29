/**
 * Typed client for the database schema tools (`crates/forge-dbschema`, `commands/db.rs`):
 * ER diagrams, Compare Structure with migrations, and Explain Plan.
 */
import { invoke } from "./index";
import type { Cell, ColumnMeta, Dialect, ModelWarning, SchemaInfo, TableKind } from "./db";

export type ColumnsMode = "all" | "keys" | "none";

export interface DiagramRequest {
  schema: SchemaInfo;
  /** Tables to show; empty shows the whole schema. */
  tables?: string[];
  /** Also the tables they reference and that reference them. */
  neighbours?: boolean;
  columns?: ColumnsMode;
  hideViews?: boolean;
}

export interface DiagramPoint {
  x: number;
  y: number;
}

export interface DiagramColumn {
  name: string;
  dataType: string;
  /** The type as the box shows it (long types shortened). */
  typeLabel: string;
  primary: boolean;
  foreign: boolean;
  nullable: boolean;
  unique: boolean;
  indexed: boolean;
  virtual: boolean;
}

export interface DiagramNode {
  id: string;
  kind: TableKind;
  comment?: string;
  x: number;
  y: number;
  width: number;
  height: number;
  columns: DiagramColumn[];
  hiddenColumns: number;
  selected: boolean;
}

export interface DiagramEdge {
  name: string;
  from: string;
  to: string;
  columns: string[];
  refColumns: string[];
  points: DiagramPoint[];
  virtual: boolean;
  optional: boolean;
}

export interface Diagram {
  schema: string;
  nodes: DiagramNode[];
  edges: DiagramEdge[];
  width: number;
  height: number;
  externalEdges: number;
}

/** Layout constants of `forge_dbschema::diagram`. */
export const DIAGRAM_HEADER = 28;
export const DIAGRAM_ROW = 20;

export type DiffKind = "added" | "removed" | "changed";
export type ItemCategory = "column" | "primaryKey" | "index" | "foreignKey" | "check" | "definition" | "comment";

export interface ItemDiff {
  category: ItemCategory;
  name: string;
  kind: DiffKind;
  source?: string;
  target?: string;
  changes?: string[];
}

export interface TableDiff {
  name: string;
  tableKind: TableKind;
  kind: DiffKind;
  items?: ItemDiff[];
}

export interface SchemaDiff {
  source: string;
  target: string;
  tables: TableDiff[];
  added: number;
  removed: number;
  changed: number;
}

export interface CompareOptions {
  ignoreComments?: boolean;
  ignoreDefaults?: boolean;
  ignoreViews?: boolean;
}

export interface MigrationOptions {
  /** Drop what exists only in the target (default true). */
  drops?: boolean;
  compare?: CompareOptions;
}

export interface MigrationStatement {
  sql: string;
  destructive: boolean;
}

export interface Migration {
  dialect: Dialect;
  sql: string;
  statements: MigrationStatement[];
  warnings: string[];
  destructive: boolean;
}

export type FrameworkKind = "laravel" | "doctrine";

export interface MigrationFile {
  /** Project-relative path. */
  path: string;
  text: string;
}

/** A schema of a data source, or a DDL snapshot (SQL text). */
export interface CompareSide {
  schema?: SchemaInfo;
  ddl?: string;
  name?: string;
}

export interface CompareRequest {
  dialect: Dialect;
  source: CompareSide;
  target: CompareSide;
  options?: MigrationOptions;
  framework?: FrameworkKind;
  /** Unix seconds the migration file is named after. */
  now?: number;
}

export interface CompareResult {
  diff: SchemaDiff;
  migration: Migration;
  file?: MigrationFile;
  warnings?: ModelWarning[];
}

export interface PlanProperty {
  name: string;
  value: string;
}

export interface PlanNode {
  id: number;
  operation: string;
  object?: string;
  detail?: string;
  cost?: number;
  startupCost?: number;
  selfCost?: number;
  rows?: number;
  properties?: PlanProperty[];
  children?: PlanNode[];
}

export interface FlameBar {
  id: number;
  depth: number;
  start: number;
  width: number;
  label: string;
  cost?: number;
  rows?: number;
}

export interface Plan {
  format: string;
  root: PlanNode;
  totalCost?: number;
  /** What the flame bar widths stand for. */
  weight: "cost" | "rows" | "count";
  flame: FlameBar[];
}

export interface ExplainResult {
  /** The statement whose rows are shown. */
  sql: string;
  columns: ColumnMeta[];
  rows: Cell[][];
  plan?: Plan;
  /** Why there is no plan. */
  error?: string;
  elapsedMs: number;
}

export const dbSchemaApi = {
  diagram: (request: DiagramRequest) => invoke<Diagram>("db_diagram", { request }),
  compare: (request: CompareRequest) => invoke<CompareResult>("db_schema_compare", { request }),
  /** Explain Plan (raw: the rows only) of one statement in a console session. */
  explain: (sessionId: string, sql: string, raw: boolean, confirmed?: boolean) =>
    invoke<ExplainResult>("db_explain", { sessionId, sql, raw, confirmed }),
  /** Save an exported diagram to a file the user chose (desktop). */
  saveExport: (path: string, data: { text?: string; base64?: string }) => invoke<void>("db_save_export", { path, ...data }),
};
