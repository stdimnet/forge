/**
 * The grid of a console result: a query's rows cannot be re-queried with another ORDER BY
 * or WHERE safely, so header sort and "Filter by value" run locally (in Rust,
 * `forge_dataedit::local`). Also the transposed view, the value viewer, aggregates, charts
 * and Compare With… like the table editor, read-only.
 */
import { createEffect, createMemo, createResource, createSignal, on, onCleanup, Show } from "solid-js";
import { ArrowRightLeft, ChartColumn, GitCompare, PanelRight, Sigma } from "lucide-solid";
import type { Cell, SortKey } from "@/ipc/db";
import { dataEditApi, type ColumnFilter } from "@/ipc/dataedit";
import { shortcutLabel } from "@/keymap";
import { DataGrid, type GridApi } from "./DataGrid";
import type { MenuItem } from "./ContextMenu";
import { nextSort, type GridColumn } from "./grid";
import { AggregatePanel, AggregateStatus, FilterChips, filterByValueItems, TransposedGrid, useAggregates } from "./dataTools";
import { ValueEditor } from "./ValueEditor";
import { ChartView } from "./ChartView";
import { CompareDialog, registerCompareSource, type CompareSource } from "./CompareView";
import { gridControllers, newGridId } from "./controllers";

/** Toolbar buttons of a result grid (rendered by the result view's toolbar). */
export interface ResultGridTools {
  transposed: () => boolean;
  chart: () => boolean;
  side: () => "value" | "aggregate" | null;
  toggleTransposed: () => void;
  toggleChart: () => void;
  toggleSide: (s: "value" | "aggregate") => void;
  compare: () => void;
}

export function ResultTools(props: { tools: ResultGridTools }) {
  const t = props.tools;
  return (
    <>
      <button class="icon-btn db-tool" classList={{ active: t.transposed() }} title="Transpose" onClick={t.toggleTransposed}>
        <ArrowRightLeft size={14} />
      </button>
      <button class="icon-btn db-tool" classList={{ active: t.side() === "value" }} title={`Value Viewer (${shortcutLabel("Console.TableResult.EditValue")})`} onClick={() => t.toggleSide("value")}>
        <PanelRight size={14} />
      </button>
      <button class="icon-btn db-tool" classList={{ active: t.side() === "aggregate" }} title="Aggregate View" onClick={() => t.toggleSide("aggregate")}>
        <Sigma size={14} />
      </button>
      <button class="icon-btn db-tool" classList={{ active: t.chart() }} title="Show Chart" onClick={t.toggleChart}>
        <ChartColumn size={14} />
      </button>
      <button class="icon-btn db-tool" title="Compare With…" onClick={t.compare}>
        <GitCompare size={14} />
      </button>
    </>
  );
}

export function ResultGrid(props: {
  columns: GridColumn[];
  rows: Cell[][];
  rowOffset: number;
  nullText: string;
  /** Label of the result for Compare With… (`console · SELECT …`). */
  label: string;
  /** The rows in display order (export). */
  onShown?: (rows: Cell[][]) => void;
  tools?: (t: ResultGridTools) => void;
}) {
  const gridId = newGridId();
  const [sort, setSort] = createSignal<SortKey[]>([]);
  const [filters, setFilters] = createSignal<ColumnFilter[]>([]);
  const [transposed, setTransposed] = createSignal(false);
  const [chart, setChart] = createSignal(false);
  const [side, setSide] = createSignal<"value" | "aggregate" | null>(null);
  const [comparing, setComparing] = createSignal(false);
  const [cursor, setCursor] = createSignal<{ row: number; col: number } | null>(null);
  const [selected, setSelected] = createSignal<{ columns: string[]; rows: Cell[][] } | null>(null);
  let grid: GridApi | undefined;

  const names = () => props.columns.map((c) => c.name);
  const [order] = createResource(
    () => ({ rows: props.rows, sort: sort(), filters: filters(), columns: names() }),
    async ({ rows, sort: s, filters: f, columns }) =>
      !s.length && !f.length ? null : dataEditApi.view(columns, rows, { order: s, filters: f }).catch(() => null),
  );
  const rows = createMemo(() => {
    const o = order.latest;
    return o ? o.map((i) => props.rows[i]) : props.rows;
  });
  createEffect(on(rows, (r) => props.onShown?.(r)));

  const aggregates = useAggregates(selected);
  const valueAt = (r: number, c: number) => rows()[r]?.[c];

  const tools: ResultGridTools = {
    transposed,
    chart,
    side,
    toggleTransposed: () => {
      setChart(false);
      setTransposed((t) => !t);
    },
    toggleChart: () => setChart((c) => !c),
    toggleSide: (s) => setSide((x) => (x === s ? null : s)),
    compare: () => setComparing(true),
  };
  props.tools?.(tools);

  const source = (): CompareSource => ({
    key: `result:${gridId}`,
    label: props.label,
    load: async () => ({ columns: names(), rows: rows() }),
  });
  onCleanup(registerCompareSource(source()));

  gridControllers.set(gridId, {
    editValue: () => tools.toggleSide("value"),
    transpose: tools.toggleTransposed,
    aggregateView: () => tools.toggleSide("aggregate"),
    chart: tools.toggleChart,
    compare: tools.compare,
  });
  onCleanup(() => gridControllers.delete(gridId));

  const addFilter = (f: ColumnFilter) => setFilters((fs) => [...fs.filter((x) => x.column !== f.column), f]);

  const menu = (): MenuItem[] => {
    const c = cursor();
    const col = c ? props.columns[c.col] : undefined;
    return [
      { label: "Value Viewer", hint: shortcutLabel("Console.TableResult.EditValue"), run: () => setSide("value") },
      ...(col && c ? [{ separator: true, label: "" }, ...filterByValueItems(col.name, valueAt(c.row, c.col), addFilter)] : []),
      { separator: true, label: "" },
      { label: "Compare With…", run: () => setComparing(true) },
    ];
  };

  return (
    <div class="db-result-body">
      <FilterChips filters={filters()} onRemove={(i) => setFilters((fs) => fs.filter((_, k) => k !== i))} onClear={() => setFilters([])} />
      <div class="db-main" data-grid-id={gridId}>
        <div class="db-main-view">
          <Show when={chart()}>
            <ChartView columns={props.columns} rows={rows()} />
          </Show>
          <Show when={!chart() && transposed()}>
            <TransposedGrid
              columns={props.columns}
              count={rows().length}
              valueAt={valueAt}
              rowLabel={(r) => String(props.rowOffset + r + 1)}
              selected={cursor()}
              onSelect={(row, col) => {
                setCursor({ row, col });
                setSelected({ columns: [props.columns[col].name], rows: [[valueAt(row, col) ?? null]] });
              }}
              onOpen={(row, col) => {
                setCursor({ row, col });
                setSide("value");
              }}
              nullText={props.nullText}
            />
          </Show>
          <Show when={!chart() && !transposed()}>
            <DataGrid
              columns={props.columns}
              rows={rows()}
              rowOffset={props.rowOffset}
              sort={sort()}
              onSort={(column, additive) => setSort((s) => nextSort(s, column, additive))}
              nullText={props.nullText}
              emptyText={filters().length ? "No rows match the filter" : "The query returned no rows"}
              ref={(api) => (grid = api)}
              onSelect={() => {
                const s = grid?.selection();
                setCursor(s ? { row: s.row, col: s.col } : null);
                const cells = grid?.selectedCells();
                setSelected(cells ? { columns: cells.cols.map((c) => props.columns[c]?.name ?? ""), rows: cells.values } : null);
              }}
              menu={menu}
            />
          </Show>
        </div>
        <Show when={side() === "value" && cursor() && props.columns[cursor()!.col]}>
          <ValueEditor
            column={props.columns[cursor()!.col]}
            value={valueAt(cursor()!.row, cursor()!.col)}
            where={`row ${props.rowOffset + cursor()!.row + 1}`}
            editable={false}
            onApply={() => undefined}
            onClose={() => setSide(null)}
          />
        </Show>
        <Show when={side() === "aggregate"}>
          <AggregatePanel aggregates={aggregates()} onClose={() => setSide(null)} />
        </Show>
      </div>
      <AggregateStatus aggregate={aggregates()?.total} onOpen={() => setSide("aggregate")} />
      <Show when={comparing()}>
        <CompareDialog left={source()} onClose={() => setComparing(false)} />
      </Show>
    </div>
  );
}
