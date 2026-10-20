/**
 * Data editor tab of a table or view (DataGrip's table editor): paging, WHERE / ORDER BY
 * filter row, sorting, cell editing with pending changes, Submit / Revert, add and delete
 * rows, export and DDL.
 */
import { createEffect, createMemo, createSignal, on, onCleanup, Show } from "solid-js";
import { createStore, produce, reconcile } from "solid-js/store";
import {
  ArrowDownUp,
  ChevronFirst,
  ChevronLast,
  ChevronLeft,
  ChevronRight,
  Download,
  FileCode,
  Filter,
  Minus,
  Plus,
  RefreshCw,
  Undo2,
  Upload,
} from "lucide-solid";
import { dbApi, dialectOf, type Cell, type QueryPage, type SortKey, type TableInfo } from "@/ipc/db";
import type { EditorTab } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { showBalloon, shortcutLabel } from "@/keymap";
import { DataGrid, type GridApi } from "./DataGrid";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { buildChanges, emptyPending, gridColumns, pendingCount, type Pending } from "./grid";
import { connect, openConsole, parseTablePath, sourceById, tableInfo } from "./store";
import { dataEditors } from "./controllers";
import { exportMenu } from "./exporting";

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

function Tool(props: { title: string; disabled?: boolean; onClick: (e: MouseEvent) => void; children: unknown; class?: string }) {
  return (
    <button class={`icon-btn db-tool ${props.class ?? ""}`} title={props.title} disabled={props.disabled} onClick={(e) => props.onClick(e)}>
      {props.children as never}
    </button>
  );
}

export function TableEditor(props: { tab: EditorTab }) {
  const loc = parseTablePath(props.tab.path)!;
  const ds = () => sourceById(loc.id);
  const pageSize = () => ds()?.options.pageSize ?? getSetting<number>("database.pageSize", 500) ?? 500;
  const nullText = () => getSetting<string>("database.showNullAs", "<null>") ?? "<null>";

  const [info, setInfo] = createSignal<TableInfo | undefined>(tableInfo(loc.id, loc.schema, loc.table));
  const [page, setPage] = createSignal<QueryPage | null>(null);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [offset, setOffset] = createSignal(0);
  const [where, setWhere] = createSignal("");
  const [orderText, setOrderText] = createSignal("");
  const [sort, setSort] = createSignal<SortKey[]>([]);
  const [pending, setPending] = createStore<Pending>(emptyPending());
  const [status, setStatus] = createSignal("");
  const [menu, setMenu] = createSignal<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let grid: GridApi | undefined;

  const dialect = () => dialectOf(ds()?.driver ?? "mysql");
  const columns = createMemo(() => gridColumns(page()?.columns ?? [], info()));
  const editable = () => info()?.kind !== "view" && !ds()?.options.readOnly;
  const changes = () => pendingCount(pending);

  async function load(nextOffset = offset()): Promise<void> {
    setLoading(true);
    setError(null);
    try {
      if (!(await connect(loc.id))) throw new Error(`Cannot connect to ${ds()?.name ?? loc.id}`);
      if (!info()) setInfo(tableInfo(loc.id, loc.schema, loc.table) ?? (await dbApi.tableInfo(loc.id, loc.schema, loc.table)));
      const res = await dbApi.tableData(loc.id, {
        schema: loc.schema,
        table: loc.table,
        filter: where().trim() || undefined,
        order: sort(),
        offset: nextOffset,
        limit: pageSize(),
      });
      setOffset(nextOffset);
      setPage(res);
      setPending(reconcile(emptyPending()));
      setStatus(`${res.total ?? res.rows.length} rows · ${res.elapsedMs} ms`);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setLoading(false);
    }
  }

  void load(0);

  const edit = (row: number, col: number, v: Cell) => {
    const loaded = page()?.rows.length ?? 0;
    if (row >= loaded) {
      setPending("inserted", row - loaded, col, v);
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

  async function submit(): Promise<void> {
    if (!changes() || !page()) return;
    try {
      const list = buildChanges(columns(), page()!.rows, info()?.primaryKey ?? [], pending);
      const res = await dbApi.applyChanges(loc.id, loc.schema, loc.table, list);
      await load();
      setStatus(`${res.statements.length} change${res.statements.length === 1 ? "" : "s"} submitted`);
    } catch (e) {
      showBalloon(`Cannot submit changes to ${loc.table}`, errorText(e), 9000);
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
      const r = (page()?.rows.length ?? 0) + pending.inserted.length - 1;
      const firstEditable = Math.max(0, columns().findIndex((c) => !c.autoIncrement));
      grid?.select(r, firstEditable);
      grid?.focus();
    });
  };

  const deleteRows = () => {
    if (!editable()) return;
    const loaded = page()?.rows.length ?? 0;
    const rows = grid?.selectedRows() ?? [];
    setPending(
      produce((p) => {
        // New rows are simply dropped; loaded rows are marked (toggling like DataGrip).
        const drop = rows.filter((r) => r >= loaded).map((r) => r - loaded);
        p.inserted = p.inserted.filter((_, i) => !drop.includes(i));
        for (const r of rows.filter((r) => r < loaded)) {
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

  const onHeaderSort = (column: string, additive: boolean) => {
    const cur = sort();
    const existing = cur.find((k) => k.column === column);
    let next: SortKey[];
    if (!existing) next = additive ? [...cur, { column, desc: false }] : [{ column, desc: false }];
    else if (!existing.desc) next = cur.map((k) => (k.column === column ? { ...k, desc: true } : k));
    else next = cur.filter((k) => k.column !== column);
    if (!additive && existing) next = next.filter((k) => k.column === column);
    applySort(next);
  };

  dataEditors.set(props.tab.path, { submit, revert, addRow, deleteRows, reload: () => load() });
  onCleanup(() => dataEditors.delete(props.tab.path));

  // Reload when the page size setting changes.
  createEffect(on(pageSize, () => void load(0), { defer: true }));

  const first = () => (page()?.rows.length ? offset() + 1 : 0);
  const last = () => offset() + (page()?.rows.length ?? 0);
  const total = () => page()?.total;

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
      showBalloon(`No DDL for ${loc.table}`, errorText(e));
    }
  };

  return (
    <div class="db-editor" data-db-path={props.tab.path}>
      <div class="db-toolbar">
        <Tool title="First Page" disabled={offset() === 0 || loading()} onClick={() => void load(0)}>
          <ChevronFirst size={16} />
        </Tool>
        <Tool title="Previous Page" disabled={offset() === 0 || loading()} onClick={() => void load(Math.max(0, offset() - pageSize()))}>
          <ChevronLeft size={16} />
        </Tool>
        <span class="db-paging" title={`Page size: ${pageSize()}`}>
          {first()}-{last()}
          <Show when={total() !== undefined}>
            <span class="muted"> of {total()}</span>
          </Show>
        </span>
        <Tool title="Next Page" disabled={!page()?.hasMore || loading()} onClick={() => void load(offset() + pageSize())}>
          <ChevronRight size={16} />
        </Tool>
        <Tool
          title="Last Page"
          disabled={!page()?.hasMore || total() === undefined || loading()}
          onClick={() => void load(Math.max(0, Math.floor(((total() ?? 1) - 1) / pageSize()) * pageSize()))}
        >
          <ChevronLast size={16} />
        </Tool>
        <span class="db-sep" />
        <Tool title="Reload Page" class={loading() ? "spinning" : ""} onClick={() => void load()}>
          <RefreshCw size={15} />
        </Tool>
        <span class="db-sep" />
        <Tool title={`Add Row (${shortcutLabel("Console.TableResult.AddRow")})`} disabled={!editable()} onClick={addRow}>
          <Plus size={16} />
        </Tool>
        <Tool title={`Delete Rows (${shortcutLabel("Console.TableResult.DeleteRows")})`} disabled={!editable()} onClick={deleteRows}>
          <Minus size={16} />
        </Tool>
        <span class="db-sep" />
        <Tool title={`Revert (${shortcutLabel("Console.TableResult.Revert")})`} disabled={!changes()} onClick={revert}>
          <Undo2 size={15} />
        </Tool>
        <button
          class="db-submit"
          classList={{ active: changes() > 0 }}
          disabled={!changes()}
          title={`Submit (${shortcutLabel("Console.TableResult.SubmitChanges")})`}
          onClick={() => void submit()}
        >
          <Upload size={14} />
          Submit
          <Show when={changes() > 0}>
            <span class="db-badge">{changes()}</span>
          </Show>
        </button>
        <span class="db-sep" />
        <Tool title="Go to DDL" onClick={() => void openDdl()}>
          <FileCode size={15} />
        </Tool>
        <Tool title="Export Data" onClick={(e) => {
          const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
          setMenu({ x: r.left, y: r.bottom + 2, items: exportMenu(exportSource) });
        }}>
          <Download size={15} />
        </Tool>
        <span class="db-spacer" />
        <Show when={info() && !info()!.primaryKey.length && info()!.kind === "table"}>
          <span class="db-chip warning" title="Rows cannot be identified, so only new rows can be submitted">No primary key</span>
        </Show>
        <Show when={ds()?.options.readOnly}>
          <span class="db-chip">Read-only</span>
        </Show>
        <span class="db-status muted">{status()}</span>
      </div>
      <div class="db-filter">
        <label class="db-filter-field">
          <Filter size={13} class="muted" />
          <span class="db-filter-kw">WHERE</span>
          <input
            class="db-filter-input"
            placeholder="condition, e.g. status = 'paid' AND total > 100"
            spellcheck={false}
            value={where()}
            onInput={(e) => setWhere(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && void load(0)}
          />
        </label>
        <label class="db-filter-field">
          <ArrowDownUp size={13} class="muted" />
          <span class="db-filter-kw">ORDER BY</span>
          <input
            class="db-filter-input"
            placeholder="column [DESC], …"
            spellcheck={false}
            value={orderText()}
            onInput={(e) => setOrderText(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && applySort(parseOrder(orderText()))}
          />
        </label>
      </div>
      <Show when={error()}>
        <div class="db-error">{error()}</div>
      </Show>
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
        emptyText={loading() ? "Loading…" : error() ? "" : "No rows"}
        ref={(api) => (grid = api)}
        menu={() =>
          editable()
            ? [
                { label: "Add Row", hint: shortcutLabel("Console.TableResult.AddRow"), run: addRow },
                { label: "Delete Rows", hint: shortcutLabel("Console.TableResult.DeleteRows"), run: deleteRows, danger: true },
              ]
            : []
        }
      />
      <Show when={menu()}>{(m) => <ContextMenu {...m()} onClose={() => setMenu(null)} />}</Show>
    </div>
  );
}
