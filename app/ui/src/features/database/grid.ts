/**
 * Data grid model, free of DOM and Solid: column kinds, cell rendering, parsing typed input
 * and turning pending edits into row changes.
 */
import { isBinary, type Cell, type ColumnInfo, type ColumnMeta, type RowChange, type TableInfo } from "@/ipc/db";

/** Plain text of a cell (editing, clipboard): NULL as empty, binary as `0x…`. */
export function cellText(v: Cell): string {
  if (v === null) return "";
  if (isBinary(v)) return `0x${v.$hex.toUpperCase()}`;
  return String(v);
}

export type ColumnKind = "number" | "bool" | "date" | "binary" | "json" | "text";

export interface GridColumn {
  name: string;
  /** Driver result type (`BIGINT UNSIGNED`). */
  typeName: string;
  /** Declared column type from the table (`decimal(10,2)`), when known. */
  dataType?: string;
  kind: ColumnKind;
  primary?: boolean;
  foreign?: boolean;
  nullable?: boolean;
  autoIncrement?: boolean;
  defaultValue?: string | null;
}

export function kindOf(type: string): ColumnKind {
  const t = type.toLowerCase();
  if (/^(bool|boolean|tinyint\(1\))/.test(t)) return "bool";
  if (/^(tinyint|smallint|mediumint|int|integer|bigint|int2|int4|int8|decimal|numeric|float|double|real|float4|float8|money|year|serial|bigserial|oid)\b/.test(t)) return "number";
  if (/^(date|time|datetime|timestamp|timestamptz|interval)/.test(t)) return "date";
  if (/^(blob|tinyblob|mediumblob|longblob|binary|varbinary|bytea|bit|geometry)/.test(t)) return "binary";
  if (/^(json|jsonb)/.test(t)) return "json";
  return "text";
}

/** Grid columns from result metadata, enriched with the table's declared columns. */
export function gridColumns(meta: ColumnMeta[], table?: TableInfo): GridColumn[] {
  return meta.map((m) => {
    const info: ColumnInfo | undefined = table?.columns.find((c) => c.name === m.name);
    return {
      name: m.name,
      typeName: m.typeName,
      dataType: info?.dataType,
      kind: kindOf(info?.dataType ?? m.typeName),
      primary: table?.primaryKey.includes(m.name),
      foreign: table?.foreignKeys.some((f) => f.columns.includes(m.name)),
      nullable: info?.nullable,
      autoIncrement: info?.autoIncrement,
      defaultValue: info?.default,
    };
  });
}

/** Short type label shown in the header (`bigint unsigned` → `bigint`). */
export function typeLabel(c: GridColumn): string {
  const t = (c.dataType ?? c.typeName).toLowerCase();
  if (t.startsWith("enum(")) return "enum";
  return t.replace(/\s+unsigned$/, "").replace(/\(.*\)/, (m) => (t.startsWith("varchar") || t.startsWith("char") || t.startsWith("decimal") ? m : ""));
}

export interface RenderedCell {
  text: string;
  /** CSS modifier: `null`, `binary`, `number`, `bool`. */
  cls?: string;
}

export function renderCell(v: Cell, kind: ColumnKind, nullText = "<null>"): RenderedCell {
  if (v === null) return { text: nullText, cls: "null" };
  if (isBinary(v)) {
    const hex = v.$hex.toUpperCase();
    return { text: hex.length > 32 ? `0x${hex.slice(0, 32)}…` : `0x${hex}`, cls: "binary" };
  }
  if (typeof v === "boolean") return { text: String(v), cls: "bool" };
  if (kind === "number" || typeof v === "number") return { text: String(v), cls: "number" };
  const s = String(v);
  return { text: s.length > 300 ? `${s.slice(0, 300)}…` : s.replace(/\n/g, "↵") };
}

/** Text shown in the cell editor. */
export const editText = (v: Cell) => cellText(v);

/** Parse what the user typed into a cell of `kind`. */
export function parseInput(text: string, kind: ColumnKind, previous: Cell): Cell {
  if (kind === "bool") {
    const t = text.trim().toLowerCase();
    if (["true", "1", "yes", "t"].includes(t)) return true;
    if (["false", "0", "no", "f"].includes(t)) return false;
    return text;
  }
  if (kind === "number") {
    const t = text.trim();
    if (/^-?\d+$/.test(t) && Math.abs(Number(t)) <= Number.MAX_SAFE_INTEGER && typeof previous !== "string") return Number(t);
    if (/^-?\d+(\.\d+)?(e[+-]?\d+)?$/i.test(t)) return typeof previous === "number" ? Number(t) : t;
    return text;
  }
  if (kind === "binary" && /^0x[0-9a-f]*$/i.test(text.trim())) return { $hex: text.trim().slice(2).toLowerCase() };
  return text;
}

export const sameCell = (a: Cell, b: Cell) => JSON.stringify(a) === JSON.stringify(b);

/** Pending edits of a data editor page. Row indexes refer to the loaded page. */
export interface Pending {
  /** row → column → new value */
  edits: Record<number, Record<number, Cell>>;
  deleted: number[];
  /** New rows appended after the loaded ones; `undefined` cells get the column default. */
  inserted: (Cell | undefined)[][];
}

export const emptyPending = (): Pending => ({ edits: {}, deleted: [], inserted: [] });

export function pendingCount(p: Pending): number {
  const edited = Object.keys(p.edits).filter((r) => !p.deleted.includes(Number(r))).length;
  return edited + p.deleted.length + p.inserted.length;
}

/** Row changes for the backend; refused (throws) for updates/deletes without a primary key. */
export function buildChanges(columns: GridColumn[], rows: Cell[][], pk: string[], p: Pending): RowChange[] {
  const out: RowChange[] = [];
  const keyOf = (row: Cell[]) => {
    if (!pk.length) throw new Error("This table has no primary key, so changed rows cannot be identified.");
    return Object.fromEntries(pk.map((k) => [k, row[columns.findIndex((c) => c.name === k)]]));
  };
  for (const [r, cols] of Object.entries(p.edits)) {
    const row = Number(r);
    if (p.deleted.includes(row)) continue;
    const values = Object.fromEntries(Object.entries(cols).map(([c, v]) => [columns[Number(c)].name, v]));
    if (Object.keys(values).length) out.push({ kind: "update", key: keyOf(rows[row]), values });
  }
  for (const row of [...p.deleted].sort((a, b) => a - b)) out.push({ kind: "delete", key: keyOf(rows[row]), values: {} });
  for (const ins of p.inserted) {
    const values: Record<string, Cell> = {};
    ins.forEach((v, i) => {
      // Untouched cells of a new row get the column default.
      if (v !== undefined) values[columns[i].name] = v;
    });
    out.push({ kind: "insert", key: {}, values });
  }
  return out;
}

/** Tab-separated text of a cell range (clipboard), with a header line when asked. */
export function rangeText(columns: GridColumn[], rows: Cell[][], r0: number, r1: number, c0: number, c1: number, header = false): string {
  const lines: string[] = [];
  if (header) lines.push(columns.slice(c0, c1 + 1).map((c) => c.name).join("\t"));
  for (let r = r0; r <= r1; r++) lines.push(rows[r].slice(c0, c1 + 1).map(cellText).join("\t"));
  return lines.join("\n");
}

/** Initial column width from the header and a sample of values. */
export function measureWidth(c: GridColumn, rows: Cell[][], index: number, charWidth = 7.2): number {
  let chars = c.name.length + typeLabel(c).length + 4;
  for (const r of rows.slice(0, 60)) chars = Math.max(chars, renderCell(r[index], c.kind).text.length);
  return Math.round(Math.min(340, Math.max(64, chars * charWidth + 22)));
}
