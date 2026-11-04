/**
 * Virtualized data grid (DataGrip's result view): typed sortable headers, row numbers,
 * cell/range selection, copy, inline editing with pending-change markers.
 */
import { createEffect, createMemo, createSignal, For, on, onCleanup, onMount, Show } from "solid-js";
import { ArrowDown, ArrowUp, KeyRound, Link2 } from "lucide-solid";
import type { Cell, SortKey } from "@/ipc/db";
import { editText, measureWidth, parseInput, rangeText, renderCell, typeLabel, type GridColumn } from "./grid";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import { t } from "@/i18n";

const ROW_H = 24;
const HEAD_H = 28;
const GUTTER_W = 46;
const OVERSCAN = 12;

export interface GridSelection {
  row: number;
  col: number;
  /** Other corner of a range selection. */
  row2: number;
  col2: number;
}

export interface GridApi {
  selection: () => GridSelection | null;
  /** The shown value of a cell (pending edits included); `undefined` for an untouched new cell. */
  valueAt: (row: number, col: number) => Cell | undefined;
  /** Selected range as row-major values with its column indexes. */
  selectedCells: () => { cols: number[]; rows: number[]; values: Cell[][] } | null;
  /** Selected row indexes (loaded and inserted rows share one index space). */
  selectedRows: () => number[];
  focus: () => void;
  select: (row: number, col: number) => void;
  startEdit: () => void;
}

export interface DataGridProps {
  columns: GridColumn[];
  rows: Cell[][];
  /** New rows shown after the loaded ones (data editor). */
  inserted?: (Cell | undefined)[][];
  /** Number shown for the first row minus one (paging). */
  rowOffset?: number;
  editable?: boolean;
  edits?: Record<number, Record<number, Cell>>;
  deleted?: number[];
  onEdit?: (row: number, col: number, value: Cell) => void;
  sort?: SortKey[];
  onSort?: (column: string, additive: boolean) => void;
  nullText?: string;
  emptyText?: string;
  /** Extra context menu entries for the selection. */
  menu?: (api: GridApi) => MenuItem[];
  /** Context menu of a column header. */
  headerMenu?: (column: GridColumn, index: number) => MenuItem[];
  /** The selection changed (aggregates, value editor). */
  onSelect?: (sel: GridSelection | null) => void;
  /** Extra CSS class of a cell (e.g. compare highlights). */
  cellClass?: (row: number, col: number) => string | undefined;
  ref?: (api: GridApi) => void;
}

export function DataGrid(props: DataGridProps) {
  let scroller!: HTMLDivElement;
  const [scrollTop, setScrollTop] = createSignal(0);
  const [height, setHeight] = createSignal(400);
  const [sel, setSel] = createSignal<GridSelection | null>(null);
  const [editing, setEditing] = createSignal<{ row: number; col: number; text: string } | null>(null);
  const [widths, setWidths] = createSignal<number[]>([]);
  const [menu, setMenu] = createSignal<{ x: number; y: number } | null>(null);
  const [headerMenu, setHeaderMenu] = createSignal<{ x: number; y: number; items: MenuItem[] } | null>(null);

  const total = () => props.rows.length + (props.inserted?.length ?? 0);
  const rowAt = (i: number): (Cell | undefined)[] => (i < props.rows.length ? props.rows[i] : (props.inserted?.[i - props.rows.length] ?? []));
  const valueAt = (r: number, c: number): Cell | undefined => {
    const e = props.edits?.[r];
    if (e && c in e) return e[c];
    return rowAt(r)[c];
  };
  const isInserted = (r: number) => r >= props.rows.length;

  // Re-measure columns when the column set changes (not on every page).
  createEffect(
    on(
      () => props.columns.map((c) => c.name).join("\u0000"),
      () => setWidths(props.columns.map((c, i) => measureWidth(c, props.rows, i))),
    ),
  );

  const totalWidth = createMemo(() => GUTTER_W + widths().reduce((a, b) => a + b, 0));
  const range = createMemo(() => {
    const first = Math.max(0, Math.floor(scrollTop() / ROW_H) - OVERSCAN);
    const last = Math.min(total(), Math.ceil((scrollTop() + height()) / ROW_H) + OVERSCAN);
    return { first, last };
  });
  const visible = createMemo(() => {
    const { first, last } = range();
    return Array.from({ length: Math.max(0, last - first) }, (_, k) => first + k);
  });

  onMount(() => {
    const ro = new ResizeObserver(() => setHeight(scroller.clientHeight));
    ro.observe(scroller);
    onCleanup(() => ro.disconnect());
    setHeight(scroller.clientHeight || 400);
  });

  const norm = () => {
    const s = sel();
    if (!s) return null;
    return { r0: Math.min(s.row, s.row2), r1: Math.max(s.row, s.row2), c0: Math.min(s.col, s.col2), c1: Math.max(s.col, s.col2) };
  };
  const inSel = (r: number, c: number) => {
    const n = norm();
    return !!n && r >= n.r0 && r <= n.r1 && c >= n.c0 && c <= n.c1;
  };
  const isCursor = (r: number, c: number) => sel()?.row === r && sel()?.col === c;

  const select = (row: number, col: number, extend = false) => {
    if (!total() || !props.columns.length) return;
    row = Math.max(0, Math.min(total() - 1, row));
    col = Math.max(0, Math.min(props.columns.length - 1, col));
    const s = sel();
    setSel(extend && s ? { ...s, row2: row, col2: col } : { row, col, row2: row, col2: col });
    // Keep the cursor visible.
    const top = row * ROW_H;
    if (top < scroller.scrollTop) scroller.scrollTop = top;
    else if (top + ROW_H > scroller.scrollTop + scroller.clientHeight - HEAD_H) scroller.scrollTop = top + ROW_H - scroller.clientHeight + HEAD_H;
    const left = GUTTER_W + widths().slice(0, col).reduce((a, b) => a + b, 0);
    const w = widths()[col] ?? 0;
    if (left - GUTTER_W < scroller.scrollLeft) scroller.scrollLeft = left - GUTTER_W;
    else if (left + w > scroller.scrollLeft + scroller.clientWidth) scroller.scrollLeft = left + w - scroller.clientWidth;
  };

  const canEdit = (r: number) => !!props.editable && !props.deleted?.includes(r);

  const startEdit = (initial?: string) => {
    const s = sel();
    if (!s || !canEdit(s.row)) return;
    const v = valueAt(s.row, s.col);
    setEditing({ row: s.row, col: s.col, text: initial ?? (v === undefined ? "" : editText(v)) });
  };

  const commitEdit = () => {
    const e = editing();
    if (!e) return;
    setEditing(null);
    const col = props.columns[e.col];
    const prev = valueAt(e.row, e.col);
    const before = prev === undefined ? "" : editText(prev);
    if (e.text === before && prev !== undefined) return;
    props.onEdit?.(e.row, e.col, parseInput(e.text, col.kind, prev ?? null));
  };

  const api: GridApi = {
    selection: sel,
    valueAt,
    selectedCells: () => {
      const n = norm();
      if (!n) return null;
      const cols = Array.from({ length: n.c1 - n.c0 + 1 }, (_, k) => n.c0 + k);
      const rows = Array.from({ length: n.r1 - n.r0 + 1 }, (_, k) => n.r0 + k);
      return { cols, rows, values: rows.map((r) => cols.map((c) => valueAt(r, c) ?? null)) };
    },
    selectedRows: () => {
      const n = norm();
      if (!n) return [];
      return Array.from({ length: n.r1 - n.r0 + 1 }, (_, k) => n.r0 + k);
    },
    focus: () => scroller.focus(),
    select: (r, c) => select(r, c),
    startEdit: () => startEdit(),
  };
  props.ref?.(api);

  createEffect(on(sel, (s) => props.onSelect?.(s), { defer: true }));

  // Selection outside the data after a reload: clamp it.
  createEffect(() => {
    const s = sel();
    if (s && s.row >= total()) setSel(total() ? { row: total() - 1, col: s.col, row2: total() - 1, col2: s.col } : null);
  });

  const onKeyDown = (e: KeyboardEvent) => {
    if (editing()) return;
    const s = sel();
    const page = Math.max(1, Math.floor((scroller.clientHeight - HEAD_H) / ROW_H) - 1);
    const move = (dr: number, dc: number) => {
      e.preventDefault();
      if (!s) return select(0, 0);
      const extend = e.shiftKey;
      const base = extend ? { row: s.row2, col: s.col2 } : s;
      select(base.row + dr, base.col + dc, extend);
    };
    switch (e.key) {
      case "ArrowDown":
        return move(1, 0);
      case "ArrowUp":
        return move(-1, 0);
      case "ArrowRight":
        return move(0, 1);
      case "ArrowLeft":
        return move(0, -1);
      case "PageDown":
        return move(page, 0);
      case "PageUp":
        return move(-page, 0);
      case "Home":
        e.preventDefault();
        return select(e.ctrlKey ? 0 : (s?.row ?? 0), 0);
      case "End":
        e.preventDefault();
        return select(e.ctrlKey ? total() - 1 : (s?.row ?? 0), props.columns.length - 1);
      case "Enter":
      case "F2":
        // Shift+Enter is the value editor (keymap).
        if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return;
        e.preventDefault();
        return startEdit();
      default:
        if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey && s && canEdit(s.row)) {
          e.preventDefault();
          startEdit(e.key);
        }
    }
  };

  const onCopy = (e: ClipboardEvent) => {
    const n = norm();
    if (!n || editing()) return;
    e.preventDefault();
    const rows = Array.from({ length: total() }, (_, r) => props.columns.map((_, c) => valueAt(r, c) ?? null));
    e.clipboardData?.setData("text/plain", rangeText(props.columns, rows, n.r0, n.r1, n.c0, n.c1));
  };

  const copySelection = (header: boolean) => {
    const n = norm();
    if (!n) return;
    const rows = Array.from({ length: total() }, (_, r) => props.columns.map((_, c) => valueAt(r, c) ?? null));
    void navigator.clipboard?.writeText(rangeText(props.columns, rows, n.r0, n.r1, n.c0, n.c1, header));
  };

  const startResize = (e: MouseEvent, index: number) => {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const start = widths()[index];
    const moveFn = (ev: MouseEvent) =>
      setWidths((w) => w.map((x, i) => (i === index ? Math.max(40, start + ev.clientX - startX) : x)));
    const up = () => {
      window.removeEventListener("mousemove", moveFn);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", moveFn);
    window.addEventListener("mouseup", up);
  };

  const sortOf = (name: string) => {
    const i = props.sort?.findIndex((s) => s.column === name) ?? -1;
    return i < 0 ? null : { desc: props.sort![i].desc, n: i + 1 };
  };

  const menuItems = (): MenuItem[] => {
    const s = sel();
    const base: MenuItem[] = [
      { label: t("database.dataGrid.copy"), hint: "Ctrl+C", run: () => copySelection(false), disabled: !s },
      { label: t("database.dataGrid.copyWithHeader"), run: () => copySelection(true), disabled: !s },
    ];
    if (props.editable && s && canEdit(s.row)) {
      base.push(
        { separator: true, label: "" },
        { label: t("database.dataGrid.edit"), hint: "F2", run: () => startEdit() },
        {
          label: t("database.dataGrid.setNull"),
          run: () => {
            const n = norm();
            if (!n) return;
            for (let r = n.r0; r <= n.r1; r++) for (let c = n.c0; c <= n.c1; c++) if (canEdit(r)) props.onEdit?.(r, c, null);
          },
        },
      );
    }
    const extra = props.menu?.(api) ?? [];
    return extra.length ? [...base, { separator: true, label: "" }, ...extra] : base;
  };

  return (
    <div
      class="dg"
      ref={scroller}
      tabIndex={0}
      data-focus-context="toolwindow"
      onScroll={() => setScrollTop(scroller.scrollTop)}
      onKeyDown={onKeyDown}
      onCopy={onCopy}
    >
      <div class="dg-head" style={{ width: `${totalWidth()}px` }}>
        <div class="dg-corner" style={{ width: `${GUTTER_W}px` }} />
        <For each={props.columns}>
          {(c, i) => {
            const s = () => sortOf(c.name);
            return (
              <div
                class="dg-th"
                classList={{ sortable: !!props.onSort, sorted: !!s() }}
                style={{ width: `${widths()[i()] ?? 100}px` }}
                title={`${c.name}: ${c.dataType ?? c.typeName}${c.nullable === false ? " (not null)" : ""}${c.autoIncrement ? " auto_increment" : ""}${c.defaultValue != null ? `\ndefault ${c.defaultValue}` : ""}${props.onSort ? "\nClick to sort, Ctrl+Click to add to the sort" : ""}`}
                onClick={(e) => props.onSort?.(c.name, e.shiftKey || e.ctrlKey || e.metaKey)}
                onContextMenu={(e) => {
                  const items = props.headerMenu?.(c, i());
                  if (!items?.length) return;
                  e.preventDefault();
                  setHeaderMenu({ x: e.clientX, y: e.clientY, items });
                }}
              >
                <Show when={c.primary} fallback={<Show when={c.foreign}><Link2 size={12} class="dg-fk" /></Show>}>
                  <KeyRound size={12} class="dg-pk" />
                </Show>
                <span class="dg-th-name">{c.name}</span>
                <span class="dg-th-type">{typeLabel(c)}</span>
                <Show when={s()}>
                  {(x) => (
                    <span class="dg-sort">
                      {x().desc ? <ArrowDown size={12} /> : <ArrowUp size={12} />}
                      <Show when={(props.sort?.length ?? 0) > 1}>{x().n}</Show>
                    </span>
                  )}
                </Show>
                <span class="dg-resize" onMouseDown={(e) => startResize(e, i())} onClick={(e) => e.stopPropagation()} />
              </div>
            );
          }}
        </For>
      </div>
      <div class="dg-body" style={{ height: `${total() * ROW_H}px`, width: `${totalWidth()}px` }}>
        <For each={visible()}>
          {(r) => {
            const deleted = () => !!props.deleted?.includes(r);
            return (
              <div
                class="dg-row"
                classList={{ deleted: deleted(), inserted: isInserted(r), current: sel()?.row === r }}
                style={{ top: `${r * ROW_H}px` }}
              >
                <div
                  class="dg-rownum"
                  style={{ width: `${GUTTER_W}px` }}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    scroller.focus();
                    const s = sel();
                    if (e.shiftKey && s) setSel({ ...s, row2: r, col: 0, col2: props.columns.length - 1 });
                    else setSel({ row: r, col: 0, row2: r, col2: props.columns.length - 1 });
                  }}
                >
                  {isInserted(r) ? "+" : (props.rowOffset ?? 0) + r + 1}
                </div>
                <For each={props.columns}>
                  {(c, ci) => {
                    const v = () => valueAt(r, ci());
                    const modified = () => !!props.edits?.[r] && ci() in props.edits[r];
                    const shown = () => {
                      const val = v();
                      if (val === undefined) return { text: c.defaultValue != null ? `<default>` : c.autoIncrement ? "<generated>" : "", cls: "null" };
                      return renderCell(val, c.kind, props.nullText);
                    };
                    const isEditing = () => editing()?.row === r && editing()?.col === ci();
                    return (
                      <div
                        class={`dg-td ${shown().cls ?? ""} ${props.cellClass?.(r, ci()) ?? ""}`}
                        classList={{
                          selected: inSel(r, ci()),
                          cursor: isCursor(r, ci()),
                          modified: modified(),
                          right: c.kind === "number",
                        }}
                        style={{ width: `${widths()[ci()] ?? 100}px` }}
                        onMouseDown={(e) => {
                          if (e.button !== 0 && inSel(r, ci())) return;
                          if (isEditing()) return;
                          e.preventDefault();
                          commitEdit();
                          scroller.focus();
                          select(r, ci(), e.shiftKey);
                        }}
                        onDblClick={() => startEdit()}
                        onContextMenu={(e) => {
                          e.preventDefault();
                          if (!inSel(r, ci())) select(r, ci());
                          setMenu({ x: e.clientX, y: e.clientY });
                        }}
                      >
                        <Show when={isEditing()} fallback={shown().text}>
                          <input
                            class="dg-editor"
                            value={editing()!.text}
                            ref={(el) => queueMicrotask(() => {
                              el.focus();
                              el.setSelectionRange(el.value.length, el.value.length);
                            })}
                            onInput={(e) => setEditing((x) => (x ? { ...x, text: e.currentTarget.value } : x))}
                            onBlur={commitEdit}
                            onKeyDown={(e) => {
                              if (e.key === "Enter" || e.key === "Escape" || e.key === "Tab") e.stopPropagation();
                              if (e.key === "Enter" && !e.ctrlKey && !e.metaKey) {
                                e.preventDefault();
                                commitEdit();
                                select(r + 1, ci());
                                scroller.focus();
                              } else if (e.key === "Escape") {
                                e.preventDefault();
                                setEditing(null);
                                scroller.focus();
                              } else if (e.key === "Tab") {
                                e.preventDefault();
                                commitEdit();
                                select(r, ci() + (e.shiftKey ? -1 : 1));
                                scroller.focus();
                              }
                            }}
                          />
                        </Show>
                      </div>
                    );
                  }}
                </For>
              </div>
            );
          }}
        </For>
      </div>
      <Show when={total() === 0}>
        <div class="dg-empty">{props.emptyText ?? "No rows"}</div>
      </Show>
      <Show when={menu()}>{(m) => <ContextMenu x={m().x} y={m().y} items={menuItems()} onClose={() => setMenu(null)} />}</Show>
      <Show when={headerMenu()}>{(m) => <ContextMenu {...m()} onClose={() => setHeaderMenu(null)} />}</Show>
    </div>
  );
}
