/**
 * CSV/TSV files: the text editor and "Edit as Table", switched with the Text / Table tabs
 * at the bottom like PHPStorm's CSV editor. The table edits the same document as the text
 * editor: every change goes through Rust (`csv_edit`), which rewrites only the touched
 * cells and keeps the delimiter, quoting and final line break, then lands in the editor as
 * one undoable edit. Sorting only changes the display order.
 */
import { createEffect, createMemo, createSignal, on, onCleanup, Show } from "solid-js";
import { Columns3, Minus, Plus, Rows3, Trash2 } from "lucide-solid";
import { viewersApi, type CsvEdit, type CsvTable } from "@/ipc/viewers";
import type { Cell, SortKey } from "@/ipc/db";
import { updateContent, type EditorTab } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { CodeEditor } from "@/editor/CodeEditor";
import { viewFor } from "@/editor/views";
import { DataGrid, type GridApi } from "@/features/database/DataGrid";
import type { GridColumn } from "@/features/database/grid";
import "@/features/database/database.css";
import { columnTitles, delimiterName, minimalChange } from "./model";

export type CsvMode = "text" | "table";

const [modes, setModes] = createSignal<Record<string, CsvMode>>({});
const [headers, setHeaders] = createSignal<Record<string, boolean>>({});

export const csvModeOf = (path: string): CsvMode => modes()[path] ?? getSetting<CsvMode>("csv.defaultView", "text");
export const setCsvMode = (path: string, mode: CsvMode) => setModes((m) => ({ ...m, [path]: mode }));
const headerOf = (path: string) => headers()[path] ?? getSetting<boolean>("csv.firstRowIsHeader", true) !== false;

/** Paths shown in a CSV editor (for the Edit as Table / Text actions). */
export const csvEditors = new Set<string>();

function TableView(props: { tab: EditorTab; group?: number }) {
  const path = props.tab.path;
  const [table, setTable] = createSignal<CsvTable | null>(null);
  const [sort, setSort] = createSignal<SortKey | null>(null);
  const [order, setOrder] = createSignal<number[] | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  let delimiter: string | undefined;
  let grid: GridApi | undefined;
  let seq = 0;

  createEffect(
    on(
      () => props.tab.content,
      (text) => {
        const mine = ++seq;
        viewersApi.csvParse(text, path, delimiter).then(
          (t) => {
            if (mine !== seq) return;
            delimiter ??= t.delimiter;
            setTable(t);
            setError(null);
          },
          (e) => setError(String(e)),
        );
      },
    ),
  );

  const header = () => headerOf(path);
  const headerRows = () => (header() && (table()?.rows.length ?? 0) > 0 ? 1 : 0);
  const titles = createMemo(() => columnTitles(header() ? table()?.rows[0] : undefined, Math.max(1, table()?.columns ?? 0)));
  const columns = createMemo<GridColumn[]>(() => titles().map((name) => ({ name, typeName: "", kind: "text" })));

  // Display order of the data rows: sorted in Rust, file order otherwise.
  createEffect(() => {
    const t = table();
    const s = sort();
    const skip = headerRows();
    if (!t || !s) return setOrder(null);
    const col = titles().indexOf(s.column);
    if (col < 0) return setOrder(null);
    void viewersApi.csvSortOrder(t.rows, col, s.desc, skip).then((o) => setOrder(o.slice(skip)));
  });
  /** File row of display row `i`. */
  const fileRow = (i: number) => (order()?.[i] ?? i + headerRows());
  const rows = createMemo<Cell[][]>(() => {
    const t = table();
    if (!t) return [];
    const n = t.rows.length - headerRows();
    return Array.from({ length: Math.max(0, n) }, (_, i) => {
      const r = t.rows[fileRow(i)] ?? [];
      return Array.from({ length: columns().length }, (_, c) => r[c] ?? "");
    });
  });

  /** Apply edits through Rust and put the result into the editor as one change. */
  const edit = async (edits: CsvEdit[]) => {
    const t = table();
    if (!t) return;
    const before = props.tab.content;
    const text = await viewersApi.csvEdit(before, t.delimiter, edits);
    const view = viewFor(path, props.group) ?? viewFor(path);
    const change = view ? minimalChange(view.state.doc.toString(), text) : null;
    if (view && change) view.dispatch({ changes: change, userEvent: "input.table" });
    else if (!view) updateContent(path, text);
  };

  const selectedRows = () => [...new Set((grid?.selectedRows() ?? []).map(fileRow))];
  const selectedColumns = () => {
    const s = grid?.selection();
    if (!s) return [];
    const [a, b] = [Math.min(s.col, s.col2), Math.max(s.col, s.col2)];
    return Array.from({ length: b - a + 1 }, (_, k) => a + k);
  };
  const currentRow = () => {
    const s = grid?.selection();
    return s ? fileRow(s.row) : (table()?.rows.length ?? 0);
  };
  const currentColumn = () => grid?.selection()?.col ?? columns().length;

  const addRow = (below = true) => {
    const s = grid?.selection();
    void edit([{ op: "insertRow", at: s ? currentRow() + (below ? 1 : 0) : (table()?.rows.length ?? 0) }]);
  };
  const addColumn = (right = true) => {
    const s = grid?.selection();
    void edit([{ op: "insertColumn", at: s ? currentColumn() + (right ? 1 : 0) : columns().length }]);
  };
  const removeRows = () => selectedRows().length && void edit([{ op: "removeRows", rows: selectedRows() }]);
  const removeColumns = () => selectedColumns().length && void edit([{ op: "removeColumns", columns: selectedColumns() }]);

  const info = () => {
    const t = table();
    if (!t) return "";
    const n = t.rows.length - headerRows();
    return `${n} ${n === 1 ? "row" : "rows"}, ${t.columns} ${t.columns === 1 ? "column" : "columns"}, ${delimiterName(t.delimiter)}-separated`;
  };

  return (
    <div class="csv-table">
      <div class="csv-toolbar" role="toolbar" aria-label="Table">
        <button class="img-btn" title="Add Row" onClick={() => addRow()}>
          <Rows3 size={14} />
          <Plus size={10} class="csv-badge" />
        </button>
        <button class="img-btn" title="Delete Rows" onClick={removeRows}>
          <Rows3 size={14} />
          <Minus size={10} class="csv-badge" />
        </button>
        <button class="img-btn" title="Add Column" onClick={() => addColumn()}>
          <Columns3 size={14} />
          <Plus size={10} class="csv-badge" />
        </button>
        <button class="img-btn" title="Delete Columns" onClick={removeColumns}>
          <Columns3 size={14} />
          <Minus size={10} class="csv-badge" />
        </button>
        <span class="img-sep" />
        <label class="csv-check">
          <input type="checkbox" checked={header()} onChange={(e) => setHeaders((h) => ({ ...h, [path]: e.currentTarget.checked }))} />
          First row is header
        </label>
        <span class="img-info">{info()}</span>
      </div>
      <Show when={!error()} fallback={<div class="img-error csv-error">{error()}</div>}>
        <DataGrid
          columns={columns()}
          rows={rows()}
          editable
          nullText=""
          emptyText="No rows"
          sort={sort() ? [sort()!] : []}
          onSort={(column) =>
            setSort((s) => (s?.column !== column ? { column, desc: false } : s.desc ? null : { column, desc: true }))
          }
          onEdit={(row, column, value) => void edit([{ op: "setCell", row: fileRow(row), column, value: value === null ? "" : String(value) }])}
          ref={(api) => (grid = api)}
          menu={() => [
            { label: "Add Row Above", run: () => addRow(false) },
            { label: "Add Row Below", run: () => addRow(true) },
            { label: "Add Column Left", run: () => addColumn(false) },
            { label: "Add Column Right", run: () => addColumn(true) },
            { separator: true, label: "" },
            { label: "Delete Rows", icon: <Trash2 size={13} />, danger: true, run: removeRows },
            { label: "Delete Columns", icon: <Trash2 size={13} />, danger: true, run: removeColumns },
          ]}
        />
      </Show>
    </div>
  );
}

export function CsvEditor(props: { tab: EditorTab; group?: number }) {
  const path = props.tab.path;
  const mode = () => csvModeOf(path);
  csvEditors.add(path);
  onCleanup(() => csvEditors.delete(path));
  return (
    <div class="csv-editor">
      <div class="csv-body">
        <div class="csv-text" classList={{ hidden: mode() !== "text" }}>
          <CodeEditor tab={props.tab} group={props.group} />
        </div>
        <Show when={mode() === "table"}>
          <TableView tab={props.tab} group={props.group} />
        </Show>
      </div>
      <div class="csv-tabs" role="tablist">
        <button class="csv-tab" classList={{ active: mode() === "text" }} role="tab" aria-selected={mode() === "text"} onClick={() => setCsvMode(path, "text")}>
          Text
        </button>
        <button class="csv-tab" classList={{ active: mode() === "table" }} role="tab" aria-selected={mode() === "table"} onClick={() => setCsvMode(path, "table")}>
          Table
        </button>
      </div>
    </div>
  );
}
