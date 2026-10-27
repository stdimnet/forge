/**
 * DataFrame tables (pandas output and View as DataFrame): the data grid of the database
 * and CSV editors with sorting, a filter and pages. Sorting, filtering and paging run in
 * Rust (`jupyter_dataframe_query`, numeric-aware like the CSV editor's sort).
 */
import { createMemo, createResource, createSignal, Show } from "solid-js";
import { ChevronLeft, ChevronRight, ChevronsLeft, ChevronsRight, Search } from "lucide-solid";
import { jupyterApi, type DataFrame } from "@/ipc/jupyter";
import type { Cell, SortKey } from "@/ipc/db";
import { getSetting } from "@/state/settings";
import { DataGrid } from "@/features/database/DataGrid";
import type { GridColumn } from "@/features/database/grid";
import "@/features/database/database.css";

const ROW_H = 24;
const CHROME_H = 30;

/** Column titles: unnamed index columns are blank in pandas; the grid needs unique names. */
export function frameColumns(frame: DataFrame): string[] {
  const seen = new Map<string, number>();
  return frame.columns.map((c, i) => {
    const base = c.trim() || (i < frame.indexColumns ? (frame.indexColumns > 1 ? `index ${i + 1}` : "index") : `column ${i + 1}`);
    const n = seen.get(base) ?? 0;
    seen.set(base, n + 1);
    return n ? `${base} (${n + 1})` : base;
  });
}

function kindOf(dtype: string | undefined): GridColumn["kind"] {
  if (!dtype) return "text";
  if (/int|float|number|integer|decimal|uint/.test(dtype)) return "number";
  if (/bool/.test(dtype)) return "bool";
  if (/date|time/.test(dtype)) return "date";
  return "text";
}

export function DataFrameView(props: { frame: DataFrame; fill?: boolean; title?: string }) {
  const [filter, setFilter] = createSignal("");
  const [page, setPage] = createSignal(0);
  const [sort, setSort] = createSignal<{ column: number; descending: boolean } | null>(null);
  const pageSize = () => Number(getSetting<number>("jupyter.dataframe.pageSize", 50)) || 50;
  const names = createMemo(() => frameColumns(props.frame));
  const columns = createMemo<GridColumn[]>(() =>
    names().map((name, i) => ({ name, typeName: props.frame.dtypes?.[i] ?? "", kind: i < props.frame.indexColumns ? "text" : kindOf(props.frame.dtypes?.[i]) })),
  );
  const [result] = createResource(
    () => ({ frame: props.frame, sort: sort(), filter: filter(), page: page(), pageSize: pageSize() }),
    (q) => jupyterApi.dataframeQuery(q.frame, { sort: q.sort, filter: q.filter, page: q.page, pageSize: q.pageSize }),
  );
  const rows = createMemo<Cell[][]>(() => result.latest?.rows ?? []);
  const sortKeys = createMemo<SortKey[]>(() => {
    const s = sort();
    return s ? [{ column: names()[s.column], desc: s.descending }] : [];
  });
  const onSort = (name: string) => {
    const column = names().indexOf(name);
    setSort((s) => (s?.column !== column ? { column, descending: false } : s.descending ? null : { column, descending: true }));
    setPage(0);
  };
  const shape = () => {
    const f = props.frame;
    const rowsTotal = f.totalRows ?? f.rows.length;
    const cols = f.totalColumns ?? f.columns.length - f.indexColumns;
    const shown = f.totalRows !== undefined && f.totalRows > f.rows.length ? ` (${f.rows.length} loaded)` : "";
    return `${rowsTotal} rows × ${cols} columns${shown}`;
  };
  const gridHeight = () => (props.fill ? undefined : `${Math.min(rows().length, 12) * ROW_H + CHROME_H + 2}px`);
  const r = () => result.latest;
  return (
    <div class="nb-df" classList={{ fill: props.fill }} data-testid="dataframe" data-focus-context="notebook-output">
      <div class="nb-df-bar">
        <Show when={props.title}>
          <span class="nb-df-title">{props.title}</span>
        </Show>
        <span class="nb-df-shape muted">{shape()}</span>
        <span class="spacer" />
        <label class="nb-df-filter" title="Rows containing the text; `column: text` filters one column">
          <Search size={12} />
          <input
            type="text"
            placeholder="Filter"
            value={filter()}
            spellcheck={false}
            onInput={(e) => {
              setFilter(e.currentTarget.value);
              setPage(0);
            }}
          />
        </label>
        <Show when={(r()?.pages ?? 1) > 1}>
          <span class="nb-df-pager">
            <button class="icon-btn" title="First Page" disabled={page() === 0} onClick={() => setPage(0)}>
              <ChevronsLeft size={13} />
            </button>
            <button class="icon-btn" title="Previous Page" disabled={page() === 0} onClick={() => setPage((p) => Math.max(0, p - 1))}>
              <ChevronLeft size={13} />
            </button>
            <span class="muted">
              {(r()?.page ?? 0) * pageSize() + 1}–{Math.min(((r()?.page ?? 0) + 1) * pageSize(), r()?.matched ?? 0)} of {r()?.matched ?? 0}
            </span>
            <button class="icon-btn" title="Next Page" disabled={page() >= (r()?.pages ?? 1) - 1} onClick={() => setPage((p) => p + 1)}>
              <ChevronRight size={13} />
            </button>
            <button class="icon-btn" title="Last Page" disabled={page() >= (r()?.pages ?? 1) - 1} onClick={() => setPage((r()?.pages ?? 1) - 1)}>
              <ChevronsRight size={13} />
            </button>
          </span>
        </Show>
      </div>
      <div class="nb-df-grid" style={{ height: gridHeight() }}>
        <DataGrid
          columns={columns()}
          rows={rows()}
          rowOffset={(r()?.page ?? 0) * pageSize()}
          sort={sortKeys()}
          onSort={onSort}
          nullText=""
          emptyText={filter() ? "No rows match the filter" : "No rows"}
        />
      </div>
    </div>
  );
}
