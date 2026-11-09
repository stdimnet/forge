/**
 * Data editor tab of a table or view (DataGrip's table editor): paging, the WHERE filter
 * with column completion and "Filter by value" conditions, header sort (Ctrl+Click for
 * several columns), all applied server-side; cell editing and the value editor with pending
 * changes, their DML preview and Submit / Revert; foreign-key navigation (Ctrl+B, Alt+F7);
 * the transposed view, aggregates, compare data, charts, export and DDL.
 */
import { createEffect, createMemo, createSignal, on, onCleanup, Show } from "solid-js";
import { createStore, produce, reconcile } from "solid-js/store";
import {
  ArrowDownUp,
  ArrowRightLeft,
  ChartColumn,
  ChevronFirst,
  ChevronLast,
  ChevronLeft,
  ChevronRight,
  Download,
  FileCode,
  Filter,
  GitCompare,
  Minus,
  PanelRight,
  Plus,
  RefreshCw,
  ScrollText,
  Sigma,
  Undo2,
  Upload,
} from "lucide-solid";
import { dbApi, dialectOf, type Cell, type ForeignKey, type QueryPage, type SortKey, type TableInfo } from "@/ipc/db";
import { dataEditApi, type ChangePreview, type ColumnFilter } from "@/ipc/dataedit";
import type { EditorTab } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { showBalloon, shortcutLabel } from "@/keymap";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { DataGrid, type GridApi } from "./DataGrid";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { buildChanges, emptyPending, gridColumns, isDefault, nextSort, pendingCount, type Pending } from "./grid";
import { connect, openConsole, openTable, parseTablePath, sourceById, states, tableInfo, tableRequest } from "./store";
import { dataEditors, gridControllers, newGridId } from "./controllers";
import { exportMenu } from "./exporting";
import { AggregatePanel, AggregateStatus, FilterChips, filterByValueItems, PendingPanel, TransposedGrid, useAggregates, WhereInput } from "./dataTools";
import { ValueEditor } from "./ValueEditor";
import { ChartView } from "./ChartView";
import { CompareDialog, registerCompareSource, tableSource } from "./CompareView";
import { t as tr } from "@/i18n";
import { latestOnly } from "@/state/async";

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** `name desc, id` → sort keys. */
export function parseOrder(text: string): SortKey[] {
  return text
    .split(",")
    .map((p) => p.trim())
    .filter(Boolean)
    .map((p) => {
      const m = /^[`"]?([^`"\s]+)[`"]?(?:\s+(asc|desc))?$/i.exec(p);
      return m ? { column: m[1], desc: m[2]?.toLowerCase() === "desc" } : null;
    })
    .filter((k): k is SortKey => !!k);
}

export const formatOrder = (keys: SortKey[]) => keys.map((k) => `${k.column}${k.desc ? " DESC" : ""}`).join(", ");

function Tool(props: { title: string; disabled?: boolean; active?: boolean; onClick: (e: MouseEvent) => void; children: unknown; class?: string }) {
  return (
    <button
      class={`icon-btn db-tool ${props.class ?? ""}`}
      classList={{ active: !!props.active }}
      title={props.title}
      aria-pressed={props.active}
      disabled={props.disabled}
      onClick={(e) => props.onClick(e)}
    >
      {props.children as never}
    </button>
  );
}

type Side = "value" | "aggregate" | null;

export function TableEditor(props: { tab: EditorTab }) {
  const loc = parseTablePath(props.tab.path)!;
  const ds = () => sourceById(loc.id);
  const pageSize = () => ds()?.options.pageSize ?? getSetting<number>("database.pageSize", 500) ?? 500;
  const nullText = () => getSetting<string>("database.showNullAs", "<null>") ?? "<null>";
  const gridId = newGridId();

  const [info, setInfo] = createSignal<TableInfo | undefined>(tableInfo(loc.id, loc.schema, loc.table));
  const [page, setPage] = createSignal<QueryPage | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [offset, setOffset] = createSignal(0);
  const [where, setWhere] = createSignal("");
  const [columnFilters, setColumnFilters] = createSignal<ColumnFilter[]>([]);
  const [orderText, setOrderText] = createSignal("");
  const [sort, setSort] = createSignal<SortKey[]>([]);
  const [pending, setPending] = createStore<Pending>(emptyPending());
  const [status, setStatus] = createSignal("");
  const [menu, setMenu] = createSignal<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [cursor, setCursor] = createSignal<{ row: number; col: number } | null>(null);
  const [selected, setSelected] = createSignal<{ columns: string[]; rows: Cell[][] } | null>(null);
  const [transposed, setTransposed] = createSignal(false);
  const [chart, setChart] = createSignal(false);
  const [side, setSide] = createSignal<Side>(null);
  const [showPending, setShowPending] = createSignal(false);
  const [preview, setPreview] = createSignal<ChangePreview | null>(null);
  const [previewError, setPreviewError] = createSignal<string | null>(null);
  const [previewLoading, setPreviewLoading] = createSignal(false);
  const [comparing, setComparing] = createSignal(false);
  let grid: GridApi | undefined;

  const dialect = () => dialectOf(ds()?.driver ?? "mysql", ds()?.options.ddlDialect);
  const columns = createMemo(() => gridColumns(page()?.columns ?? [], info()));
  const editable = () => info()?.kind !== "view" && !ds()?.options.readOnly;
  const changes = () => pendingCount(pending);
  const loaded = () => page()?.rows.length ?? 0;
  const total = () => loaded() + pending.inserted.length;

  // A filtered open (foreign-key navigation) of this table.
  const pendingRequest = tableRequest();
  if (pendingRequest?.path === props.tab.path && pendingRequest.options.filter !== undefined) setWhere(pendingRequest.options.filter);

  // Only the newest load applies its reply: a slower earlier one (another page or filter)
  // would show the wrong rows and throw away the edits made since.
  const loads = latestOnly();
  async function load(nextOffset = offset()): Promise<void> {
    const current = loads.begin();
    setLoading(true);
    setError(null);
    try {
      if (!(await connect(loc.id))) throw new Error(tr("database.tableEditor.cannotConnectTo", { name: ds()?.name ?? loc.id }));
      if (!info()) setInfo(tableInfo(loc.id, loc.schema, loc.table) ?? (await dbApi.tableInfo(loc.id, loc.schema, loc.table)));
      const res = await dbApi.tableData(loc.id, {
        schema: loc.schema,
        table: loc.table,
        filter: where().trim() || undefined,
        columnFilters: columnFilters(),
        order: sort(),
        offset: nextOffset,
        limit: pageSize(),
      });
      if (!current()) return;
      setOffset(nextOffset);
      setPage(res);
      setPending(reconcile(emptyPending()));
      const n = res.total ?? res.rows.length;
      setStatus(tr("database.tableEditor.rowsStatus", { count: n, ms: res.elapsedMs }));
    } catch (e) {
      if (current()) setError(errorText(e));
    } finally {
      if (current()) setLoading(false);
    }
  }

  void load(0);

  createEffect(
    on(
      tableRequest,
      (r) => {
        if (r?.path !== props.tab.path || r.options.filter === undefined) return;
        setWhere(r.options.filter);
        setColumnFilters([]);
        void load(0);
      },
      { defer: true },
    ),
  );

  const valueAt = (row: number, col: number): Cell | undefined => {
    const e = pending.edits[row];
    if (e && col in e) return e[col];
    return row < loaded() ? page()!.rows[row][col] : pending.inserted[row - loaded()]?.[col];
  };

  const edit = (row: number, col: number, v: Cell) => {
    if (row >= loaded()) {
      setPending("inserted", row - loaded(), col, v);
      return;
    }
    const original = page()!.rows[row][col];
    setPending(
      produce((p) => {
        const cols = { ...(p.edits[row] ?? {}) };
        if (JSON.stringify(original) === JSON.stringify(v)) delete cols[col];
        else cols[col] = v;
        if (Object.keys(cols).length) p.edits[row] = cols;
        else delete p.edits[row];
      }),
    );
  };

  /** The row changes for the backend: original rows as keys, Rust picks the key columns. */
  const rowChanges = () => buildChanges(columns(), page()?.rows ?? [], pending);

  // The DML preview follows the pending changes while it is open.
  createEffect(
    on([showPending, () => JSON.stringify(pending)], async ([open]) => {
      if (!open) return;
      if (!changes()) {
        setPreview(null);
        setPreviewError(null);
        return;
      }
      setPreviewLoading(true);
      try {
        setPreview(await dataEditApi.previewChanges(loc.id, loc.schema, loc.table, rowChanges()));
        setPreviewError(null);
      } catch (e) {
        setPreview(null);
        setPreviewError(errorText(e));
      } finally {
        setPreviewLoading(false);
      }
    }),
  );

  async function submit(): Promise<void> {
    if (!changes() || !page()) return;
    try {
      const list = rowChanges();
      const p = await dataEditApi.previewChanges(loc.id, loc.schema, loc.table, list);
      if (p.blocked) throw new Error(p.blocked);
      let confirmed = false;
      if (p.confirm) {
        confirmed = await askConfirm({ title: tr("database.tableEditor.productionDataSource"), message: `${p.confirm}\n\n${p.statements.join(";\n")};`, confirm: tr("database.tableEditor.submit") });
        if (!confirmed) return;
      }
      const res = await dbApi.applyChanges(loc.id, loc.schema, loc.table, list, confirmed);
      await load();
      setShowPending(false);
      setStatus(`${res.statements.length} change${res.statements.length === 1 ? "" : "s"} submitted`);
    } catch (e) {
      showBalloon(tr("database.tableEditor.cannotSubmitChangesToTable", { table: loc.table }), errorText(e), 9000);
    }
  }

  const revert = () => {
    setPending(reconcile(emptyPending()));
    setStatus("Changes reverted");
  };

  const addRow = () => {
    if (!editable() || !page()) return;
    setPending("inserted", (rows) => [...rows, columns().map(() => undefined)]);
    queueMicrotask(() => {
      const r = loaded() + pending.inserted.length - 1;
      const firstEditable = Math.max(0, columns().findIndex((c) => !c.autoIncrement));
      setTransposed(false);
      grid?.select(r, firstEditable);
      grid?.focus();
    });
  };

  const selectedRows = () => (transposed() ? (cursor() ? [cursor()!.row] : []) : (grid?.selectedRows() ?? []));

  const deleteRows = () => {
    if (!editable()) return;
    const rows = selectedRows();
    setPending(
      produce((p) => {
        // New rows are simply dropped; loaded rows are marked (toggling like DataGrip).
        const drop = rows.filter((r) => r >= loaded()).map((r) => r - loaded());
        p.inserted = p.inserted.filter((_, i) => !drop.includes(i));
        for (const r of rows.filter((r) => r < loaded())) {
          if (p.deleted.includes(r)) p.deleted = p.deleted.filter((x) => x !== r);
          else p.deleted.push(r);
        }
      }),
    );
  };

  const applySort = (keys: SortKey[]) => {
    setSort(keys);
    setOrderText(formatOrder(keys));
    void load(0);
  };

  const onHeaderSort = (column: string, additive: boolean) => applySort(nextSort(sort(), column, additive));

  const addFilter = (f: ColumnFilter) => {
    setColumnFilters((fs) => [...fs.filter((x) => x.column !== f.column), f]);
    void load(0);
  };

  // ---- foreign keys ---------------------------------------------------------------------

  const rowValues = (row: number) => columns().map((_, c) => {
    // Navigation uses the stored row, not pending edits.
    const v = row < loaded() ? page()!.rows[row][c] : valueAt(row, c);
    return v === undefined || isDefault(v) ? null : v;
  });

  const keysOf = (column: string): ForeignKey[] => (info()?.foreignKeys ?? []).filter((f) => f.columns.includes(column));

  const navigate = async (fk: ForeignKey, direction: "referenced" | "referencing", from: { schema: string; table: string }) => {
    const c = cursor();
    if (!c || c.row >= total()) return;
    try {
      const nav = await dataEditApi.navigate({
        dialect: dialect(),
        schema: from.schema,
        table: from.table,
        fk,
        columns: columns().map((x) => x.name),
        row: rowValues(c.row),
        direction,
      });
      openTable(loc.id, nav.schema, nav.table, { filter: nav.filter });
    } catch (e) {
      showBalloon(tr("database.tableEditor.cannotNavigate"), errorText(e));
    }
  };

  const cellPoint = () => {
    const el = document.querySelector<HTMLElement>(`[data-grid-id="${gridId}"] .cursor`);
    const r = el?.getBoundingClientRect();
    return r ? { x: r.left + 8, y: r.bottom + 2 } : { x: 200, y: 200 };
  };

  const referencedItems = (): MenuItem[] => {
    const c = cursor();
    if (!c) return [];
    const col = columns()[c.col]?.name;
    return keysOf(col).map((fk) => ({
      label: tr("tools.database.goTo", { refTable: fk.refTable, columns: fk.columns.join(", "), value: fk.virtual ? " · virtual" : "" }),
      hint: shortcutLabel("Console.TableResult.GotoReferencedData"),
      run: () => void navigate(fk, "referenced", { schema: loc.schema, table: loc.table }),
    }));
  };

  const referencingItems = async (): Promise<MenuItem[]> => {
    const st = states[loc.id];
    const tables = Object.entries(st?.introspected ?? {}).flatMap(([schema, s]) =>
      (s?.tables ?? []).map((t) => ({ schema, table: t.name, foreignKeys: t.foreignKeys })),
    );
    const incoming = await dataEditApi.incoming(loc.schema, loc.table, tables);
    return incoming.map((i) => ({
      label: `${i.table} (${i.fk.columns.join(", ")})${i.fk.virtual ? " · virtual" : ""}`,
      run: () => void navigate(i.fk, "referencing", { schema: i.schema, table: i.table }),
    }));
  };

  const gotoReferenced = () => {
    const items = referencedItems();
    if (!items.length) return showBalloon(tr("database.tableEditor.noForeignKey"), tr("tools.database.theColumnOfThisCell"));
    if (items.length === 1) return items[0].run?.();
    setMenu({ ...cellPoint(), items });
  };

  const findReferencing = async () => {
    if (!cursor()) return;
    const items = await referencingItems();
    if (!items.length) return showBalloon(tr("database.tableEditor.noReferencingTables"), tr("database.tableEditor.noForeignKeyReferencesTable", { table: loc.table }));
    if (items.length === 1) return items[0].run?.();
    setMenu({ ...cellPoint(), items: [{ label: tr("database.tableEditor.findReferencingRowsIn"), disabled: true }, ...items] });
  };

  // ---- side panels and modes ------------------------------------------------------------

  const editValue = () => {
    if (!cursor()) grid?.select(0, 0);
    setChart(false);
    setSide((s) => (s === "value" ? null : "value"));
  };

  const toggleTransposed = () => {
    setChart(false);
    setTransposed((t) => !t);
    const c = cursor();
    if (!transposed() && c) queueMicrotask(() => grid?.select(c.row, c.col));
  };

  const aggregates = useAggregates(selected);

  const compareSource = () => tableSource(loc.id, loc.schema, loc.table, where().trim() || undefined);
  onCleanup(registerCompareSource({ ...tableSource(loc.id, loc.schema, loc.table), label: tr("tools.database.open2", { value: ds()?.name ?? loc.id, schema: loc.schema, table: loc.table }) }));

  dataEditors.set(props.tab.path, { submit, revert, addRow, deleteRows, reload: () => load() });
  gridControllers.set(gridId, {
    gotoReferenced,
    findReferencing: () => void findReferencing(),
    editValue,
    transpose: toggleTransposed,
    aggregateView: () => setSide((s) => (s === "aggregate" ? null : "aggregate")),
    chart: () => setChart((c) => !c),
    compare: () => setComparing(true),
  });
  onCleanup(() => {
    dataEditors.delete(props.tab.path);
    gridControllers.delete(gridId);
  });

  // Reload when the page size setting changes.
  createEffect(on(pageSize, () => void load(0), { defer: true }));

  const first = () => (page()?.rows.length ? offset() + 1 : 0);
  const last = () => offset() + (page()?.rows.length ?? 0);
  const totalRows = () => page()?.total;

  const exportSource = () => ({
    dialect: dialect(),
    table: loc.table,
    columns: columns().map((c) => c.name),
    rows: page()?.rows ?? [],
  });

  const openDdl = async () => {
    try {
      const ddl = await dbApi.tableDdl(loc.id, loc.schema, loc.table);
      openConsole(loc.id, ddl);
    } catch (e) {
      showBalloon(tr("database.tableEditor.noDdlForTable", { table: loc.table }), errorText(e));
    }
  };

  const onSelect = () => {
    const s = grid?.selection();
    setCursor(s ? { row: s.row, col: s.col } : null);
    const cells = grid?.selectedCells();
    setSelected(cells ? { columns: cells.cols.map((c) => columns()[c]?.name ?? ""), rows: cells.values } : null);
  };

  const cellMenu = (): MenuItem[] => {
    const c = cursor();
    const items: MenuItem[] = [];
    if (c) {
      const col = columns()[c.col];
      items.push({ label: tr("database.tableEditor.editValue"), hint: shortcutLabel("Database.Properties"), run: editValue });
      if (editable() && (col?.defaultValue != null || col?.autoIncrement) && c.row < total())
        items.push({ label: tr("database.tableEditor.setDefault"), run: () => edit(c.row, c.col, { $default: true }) });
      items.push({ separator: true, label: "" });
      if (col && c.row < loaded()) items.push(...filterByValueItems(col.name, page()!.rows[c.row][c.col], addFilter));
      const refs = referencedItems();
      if (refs.length) items.push({ separator: true, label: "" }, ...refs);
      items.push({ label: tr("database.tableEditor.findReferencingRows"), hint: shortcutLabel("FindUsages"), run: () => void findReferencing() });
      items.push({ separator: true, label: "" });
    }
    if (editable())
      items.push(
        { label: tr("database.tableEditor.addRow"), hint: shortcutLabel("Console.TableResult.AddRow"), run: addRow },
        { label: tr("database.tableEditor.deleteRows"), hint: shortcutLabel("Console.TableResult.DeleteRows"), run: deleteRows, danger: true },
        { separator: true, label: "" },
      );
    items.push({ label: tr("database.tableEditor.compareWith"), run: () => setComparing(true) });
    return items;
  };

  const headerMenu = (column: { name: string }): MenuItem[] => [
    { label: tr("database.tableEditor.sortAscending"), run: () => applySort([{ column: column.name, desc: false }]) },
    { label: tr("database.tableEditor.sortDescending"), run: () => applySort([{ column: column.name, desc: true }]) },
    { label: tr("database.tableEditor.addToSort"), hint: "Ctrl+Click", run: () => applySort([...sort().filter((k) => k.column !== column.name), { column: column.name, desc: false }]) },
    { label: tr("database.tableEditor.clearSort"), disabled: !sort().length, run: () => applySort([]) },
    { separator: true, label: "" },
    { label: tr("database.tableEditor.filterByNameIsNull", { name: column.name }), run: () => addFilter({ column: column.name, op: "isNull" }) },
    { label: tr("database.tableEditor.filterByNameIsNot", { name: column.name }), run: () => addFilter({ column: column.name, op: "isNotNull" }) },
  ];

  return (
    <div class="db-editor" data-db-path={props.tab.path}>
      <div class="db-toolbar">
        <Tool title={tr("database.tableEditor.firstPage")} disabled={offset() === 0 || loading()} onClick={() => void load(0)}>
          <ChevronFirst size={16} />
        </Tool>
        <Tool title={tr("database.tableEditor.previousPage")} disabled={offset() === 0 || loading()} onClick={() => void load(Math.max(0, offset() - pageSize()))}>
          <ChevronLeft size={16} />
        </Tool>
        <span class="db-paging" title={tr("database.tableEditor.pageSizePagesize", { pageSize: pageSize() })}>
          {first()}-{last()}
          <Show when={totalRows() !== undefined}>
            <span class="muted"> of {totalRows()}</span>
          </Show>
        </span>
        <Tool title={tr("database.tableEditor.nextPage")} disabled={!page()?.hasMore || loading()} onClick={() => void load(offset() + pageSize())}>
          <ChevronRight size={16} />
        </Tool>
        <Tool
          title={tr("database.tableEditor.lastPage")}
          disabled={!page()?.hasMore || totalRows() === undefined || loading()}
          onClick={() => void load(Math.max(0, Math.floor(((totalRows() ?? 1) - 1) / pageSize()) * pageSize()))}
        >
          <ChevronLast size={16} />
        </Tool>
        <span class="db-sep" />
        <Tool title={tr("database.tableEditor.reloadPage")} class={loading() ? "spinning" : ""} onClick={() => void load()}>
          <RefreshCw size={15} />
        </Tool>
        <span class="db-sep" />
        <Tool title={tr("tools.database.addRow", { value: shortcutLabel("Console.TableResult.AddRow") })} disabled={!editable()} onClick={addRow}>
          <Plus size={16} />
        </Tool>
        <Tool title={tr("tools.database.deleteRows", { value: shortcutLabel("Console.TableResult.DeleteRows") })} disabled={!editable()} onClick={deleteRows}>
          <Minus size={16} />
        </Tool>
        <span class="db-sep" />
        <Tool title={tr("tools.database.revert", { value: shortcutLabel("Console.TableResult.Revert") })} disabled={!changes()} onClick={revert}>
          <Undo2 size={15} />
        </Tool>
        <Tool title={tr("database.tableEditor.previewPendingChanges")} disabled={!changes()} active={showPending()} onClick={() => setShowPending((v) => !v)}>
          <ScrollText size={15} />
        </Tool>
        <button
          class="db-submit"
          classList={{ active: changes() > 0 }}
          disabled={!changes()}
          title={tr("tools.database.submit", { value: shortcutLabel("Console.TableResult.SubmitChanges") })}
          onClick={() => void submit()}
        >
          <Upload size={14} />
          {tr("database.tableEditor.submit")}
          <Show when={changes() > 0}>
            <span class="db-badge">{changes()}</span>
          </Show>
        </button>
        <span class="db-sep" />
        <Tool title={tr("database.tableEditor.transpose")} active={transposed()} onClick={toggleTransposed}>
          <ArrowRightLeft size={15} />
        </Tool>
        <Tool title={tr("tools.database.valueEditor", { value: shortcutLabel("Database.Properties") })} active={side() === "value"} onClick={editValue}>
          <PanelRight size={15} />
        </Tool>
        <Tool title={tr("database.tableEditor.aggregateView")} active={side() === "aggregate"} onClick={() => setSide((s) => (s === "aggregate" ? null : "aggregate"))}>
          <Sigma size={15} />
        </Tool>
        <Tool title={tr("database.tableEditor.showChart")} active={chart()} onClick={() => setChart((c) => !c)}>
          <ChartColumn size={15} />
        </Tool>
        <Tool title={tr("database.tableEditor.compareWith")} onClick={() => setComparing(true)}>
          <GitCompare size={15} />
        </Tool>
        <span class="db-sep" />
        <Tool title={tr("database.tableEditor.goToDdl")} onClick={() => void openDdl()}>
          <FileCode size={15} />
        </Tool>
        <Tool
          title={tr("database.tableEditor.exportData")}
          onClick={(e) => {
            const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
            setMenu({ x: r.left, y: r.bottom + 2, items: exportMenu(exportSource) });
          }}
        >
          <Download size={15} />
        </Tool>
        <span class="db-spacer" />
        <Show when={info() && !info()!.primaryKey.length && info()!.kind === "table"}>
          <span class="db-chip warning" title={tr("database.tableEditor.withoutAPrimaryKeyRows")}>
            {tr("database.tableEditor.noPrimaryKey")}
          </span>
        </Show>
        <Show when={ds()?.options.readOnly}>
          <span class="db-chip">{tr("database.tableEditor.readOnly")}</span>
        </Show>
        <span class="db-status muted">{status()}</span>
      </div>
      <div class="db-filter">
        <label class="db-filter-field">
          <Filter size={13} class="muted" />
          <span class="db-filter-kw">{"WHERE"}</span>
          <WhereInput
            value={where()}
            columns={columns().map((c) => c.name)}
            placeholder={tr("database.tableEditor.conditionEGStatusPaid")}
            onInput={setWhere}
            onSubmit={() => void load(0)}
          />
        </label>
        <label class="db-filter-field">
          <ArrowDownUp size={13} class="muted" />
          <span class="db-filter-kw">{tr("tools.database.orderBy")}</span>
          <input
            class="db-filter-input"
            placeholder={tr("database.tableEditor.columnDesc")}
            spellcheck={false}
            value={orderText()}
            onInput={(e) => setOrderText(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && applySort(parseOrder(orderText()))}
          />
        </label>
      </div>
      <FilterChips
        filters={columnFilters()}
        onRemove={(i) => {
          setColumnFilters((fs) => fs.filter((_, k) => k !== i));
          void load(0);
        }}
        onClear={() => {
          setColumnFilters([]);
          void load(0);
        }}
      />
      <Show when={error()}>
        <div class="db-error">{error()}</div>
      </Show>
      <div class="db-main" data-grid-id={gridId}>
        <div class="db-main-view">
          <Show when={chart()}>
            <ChartView columns={columns()} rows={page()?.rows ?? []} />
          </Show>
          <Show when={!chart() && transposed()}>
            <TransposedGrid
              columns={columns()}
              count={total()}
              valueAt={valueAt}
              rowLabel={(r) => (r >= loaded() ? "+" : String(offset() + r + 1))}
              selected={cursor()}
              onSelect={(row, col) => {
                setCursor({ row, col });
                const v = valueAt(row, col);
                setSelected({ columns: [columns()[col].name], rows: [[v === undefined ? null : v]] });
              }}
              onOpen={(row, col) => {
                setCursor({ row, col });
                setSide("value");
              }}
              modified={(r, c) => !!pending.edits[r] && c in pending.edits[r]}
              deleted={(r) => pending.deleted.includes(r)}
              nullText={nullText()}
            />
          </Show>
          <Show when={!chart() && !transposed()}>
            <DataGrid
              columns={columns()}
              rows={page()?.rows ?? []}
              inserted={pending.inserted}
              rowOffset={offset()}
              editable={editable()}
              edits={pending.edits}
              deleted={pending.deleted}
              onEdit={edit}
              sort={sort()}
              onSort={onHeaderSort}
              nullText={nullText()}
              emptyText={loading() ? tr("database.tableEditor.loading") : error() ? "" : tr("database.tableEditor.noRows")}
              ref={(api) => (grid = api)}
              onSelect={onSelect}
              headerMenu={headerMenu}
              menu={cellMenu}
            />
          </Show>
        </div>
        <Show when={side() === "value" && cursor() && columns()[cursor()!.col]}>
          <ValueEditor
            column={columns()[cursor()!.col]}
            value={valueAt(cursor()!.row, cursor()!.col)}
            where={cursor()!.row >= loaded() ? "new row" : `row ${offset() + cursor()!.row + 1}`}
            editable={editable() && !pending.deleted.includes(cursor()!.row)}
            onApply={(v) => edit(cursor()!.row, cursor()!.col, v)}
            onClose={() => setSide(null)}
          />
        </Show>
        <Show when={side() === "aggregate"}>
          <AggregatePanel aggregates={aggregates()} onClose={() => setSide(null)} />
        </Show>
      </div>
      <Show when={showPending() && changes() > 0}>
        <PendingPanel
          count={changes()}
          preview={preview()}
          error={previewError()}
          loading={previewLoading()}
          onSubmit={() => void submit()}
          onRevert={revert}
          onClose={() => setShowPending(false)}
        />
      </Show>
      <AggregateStatus aggregate={aggregates()?.total} onOpen={() => setSide("aggregate")} />
      <Show when={menu()}>{(m) => <ContextMenu {...m()} onClose={() => setMenu(null)} />}</Show>
      <Show when={comparing()}>
        <CompareDialog left={compareSource()} onClose={() => setComparing(false)} />
      </Show>
    </div>
  );
}
