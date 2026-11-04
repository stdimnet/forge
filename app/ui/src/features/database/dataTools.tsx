/**
 * Parts of DataGrip's data editor shared by table editors and console results: the WHERE
 * field with column completion, "Filter by value" chips, the aggregates status line and
 * Aggregate view, the transposed (row-as-columns) view and the pending changes panel with
 * the DML preview. The numbers and SQL come from Rust (`forge_dataedit`); this only renders.
 */
import { createEffect, createMemo, createSignal, For, on, onCleanup, Show } from "solid-js";
import { KeyRound, Link2, Upload, Undo2, X } from "lucide-solid";
import type { Cell } from "@/ipc/db";
import { dataEditApi, type Aggregate, type Aggregates, type ChangePreview, type ColumnFilter } from "@/ipc/dataedit";
import { cellText, renderCell, typeLabel, type GridColumn } from "./grid";
import { t } from "@/i18n";

// ---- filter row -----------------------------------------------------------------------

const KEYWORDS = ["AND", "OR", "NOT", "IS NULL", "IS NOT NULL", "LIKE", "IN", "BETWEEN"];

/** The identifier being typed before the caret. */
function wordAt(text: string, caret: number): { start: number; word: string } {
  let start = caret;
  while (start > 0 && /[\w$]/.test(text[start - 1])) start--;
  return { start, word: text.slice(start, caret) };
}

/** WHERE condition field with completion of column names (DataGrip's filter row). */
export function WhereInput(props: {
  value: string;
  columns: string[];
  placeholder?: string;
  onInput: (v: string) => void;
  onSubmit: () => void;
}) {
  let input!: HTMLInputElement;
  const [items, setItems] = createSignal<string[]>([]);
  const [active, setActive] = createSignal(0);
  const [start, setStart] = createSignal(0);
  /** Popup position: under the word being completed (monospace field). */
  const left = () => Math.min(start() * 7.2, Math.max(0, (input?.clientWidth ?? 300) - 180));

  const refresh = () => {
    const caret = input.selectionStart ?? input.value.length;
    const { start: s, word } = wordAt(input.value, caret);
    setStart(s);
    if (!word) return setItems([]);
    const w = word.toLowerCase();
    const cols = props.columns.filter((c) => c.toLowerCase().startsWith(w) && c.toLowerCase() !== w);
    const more = props.columns.filter((c) => !c.toLowerCase().startsWith(w) && c.toLowerCase().includes(w));
    const kws = KEYWORDS.filter((k) => k.toLowerCase().startsWith(w));
    setItems([...cols, ...more, ...kws].slice(0, 12));
    setActive(0);
  };

  const accept = (item: string) => {
    const caret = input.selectionStart ?? input.value.length;
    const v = input.value;
    const next = v.slice(0, start()) + item + v.slice(caret);
    props.onInput(next);
    setItems([]);
    queueMicrotask(() => {
      input.value = next;
      const pos = start() + item.length;
      input.setSelectionRange(pos, pos);
      input.focus();
    });
  };

  const away = () => setItems([]);
  onCleanup(away);

  return (
    <span class="db-where">
      <input
        ref={input}
        class="db-filter-input"
        placeholder={props.placeholder}
        spellcheck={false}
        value={props.value}
        data-testid="db-where"
        onInput={(e) => {
          props.onInput(e.currentTarget.value);
          refresh();
        }}
        onBlur={() => setTimeout(away, 120)}
        onKeyDown={(e) => {
          // Keys of the completion popup inside the field (not shortcuts).
          const open = items().length > 0;
          if (open && e.key === "ArrowDown") {
            e.preventDefault();
            setActive((a) => (a + 1) % items().length);
          } else if (open && e.key === "ArrowUp") {
            e.preventDefault();
            setActive((a) => (a - 1 + items().length) % items().length);
          } else if (open && (e.key === "Enter" || e.key === "Tab")) {
            e.preventDefault();
            e.stopPropagation();
            accept(items()[active()]);
          } else if (open && e.key === "Escape") {
            e.stopPropagation();
            setItems([]);
          } else if (e.key === "Enter") {
            e.preventDefault();
            props.onSubmit();
          }
        }}
      />
      <Show when={items().length}>
        <div class="db-complete" role="listbox" style={{ left: `${left()}px` }}>
          <For each={items()}>
            {(it, i) => (
              <div
                class="db-complete-item"
                classList={{ active: i() === active(), kw: KEYWORDS.includes(it) }}
                onMouseDown={(e) => {
                  e.preventDefault();
                  accept(it);
                }}
              >
                {it}
              </div>
            )}
          </For>
        </div>
      </Show>
    </span>
  );
}

const OP_TEXT: Record<ColumnFilter["op"], string> = {
  eq: "=",
  ne: "<>",
  lt: "<",
  le: "<=",
  gt: ">",
  ge: ">=",
  isNull: "IS NULL",
  isNotNull: "IS NOT NULL",
  like: "LIKE",
  notLike: "NOT LIKE",
  in: "IN",
};

const literal = (v: Cell | undefined) => (typeof v === "number" || typeof v === "boolean" ? String(v) : v === null || v === undefined ? "NULL" : `'${cellText(v)}'`);

/** Short text of a column filter for its chip. */
export function filterLabel(f: ColumnFilter): string {
  if (f.op === "isNull" || f.op === "isNotNull" || ((f.op === "eq" || f.op === "ne") && (f.value === null || f.value === undefined)))
    return `${f.column} ${f.op === "isNotNull" || f.op === "ne" ? "IS NOT NULL" : "IS NULL"}`;
  if (f.op === "in") return `${f.column} IN (${(f.values ?? []).map(literal).join(", ")})`;
  return `${f.column} ${OP_TEXT[f.op]} ${literal(f.value)}`;
}

export function FilterChips(props: { filters: ColumnFilter[]; onRemove: (index: number) => void; onClear: () => void }) {
  return (
    <Show when={props.filters.length}>
      <div class="db-chips" data-testid="db-filter-chips">
        <For each={props.filters}>
          {(f, i) => (
            <span class="db-filter-chip" title={t("database.dataTools.filterByValue")}>
              {filterLabel(f)}
              <button class="db-chip-x" title={t("database.dataTools.removeFilter")} onClick={() => props.onRemove(i())}>
                <X size={11} />
              </button>
            </span>
          )}
        </For>
        <Show when={props.filters.length > 1}>
          <button class="link-btn db-chips-clear" onClick={props.onClear}>
            {t("database.dataTools.clearAll")}
          </button>
        </Show>
      </div>
    </Show>
  );
}

/** "Filter by value" entries of a cell's context menu. */
export function filterByValueItems(column: string, value: Cell | undefined, add: (f: ColumnFilter) => void) {
  if (value === undefined) return [];
  if (value === null)
    return [
      { label: t("database.dataTools.filterByColumnIsNull", { column }), run: () => add({ column, op: "isNull" }) },
      { label: t("database.dataTools.filterByColumnIsNot", { column }), run: () => add({ column, op: "isNotNull" }) },
    ];
  const shown = cellText(value);
  const short = shown.length > 24 ? `${shown.slice(0, 24)}…` : shown;
  return [
    { label: t("database.dataTools.filterByColumnShort", { column, short }), run: () => add({ column, op: "eq", value }) },
    { label: t("database.dataTools.filterByColumnShort2", { column, short }), run: () => add({ column, op: "ne", value }) },
    { label: t("database.dataTools.filterByColumnIsNull", { column }), run: () => add({ column, op: "isNull" }) },
  ];
}

// ---- aggregates -----------------------------------------------------------------------

const valueText = (v: Cell | undefined) => (v === undefined ? "" : v === null ? "<null>" : cellText(v));

/**
 * Aggregates of the selected cells, recomputed in Rust when the selection or the data
 * changes (debounced).
 */
export function useAggregates(source: () => { columns: string[]; rows: Cell[][] } | null) {
  const [result, setResult] = createSignal<Aggregates | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let seq = 0;
  createEffect(
    on(source, (s) => {
      clearTimeout(timer);
      if (!s || !s.rows.length) return setResult(null);
      const n = ++seq;
      timer = setTimeout(() => {
        void dataEditApi
          .aggregate(s.columns, s.rows)
          .then((r) => n === seq && setResult(r))
          .catch(() => n === seq && setResult(null));
      }, 60);
    }),
  );
  onCleanup(() => clearTimeout(timer));
  return result;
}

/** The status line under the grid: count, sum, avg, min, max, distinct of the selection. */
export function AggregateStatus(props: { aggregate: Aggregate | null | undefined; onOpen?: () => void }) {
  const parts = createMemo(() => {
    const a = props.aggregate;
    if (!a || a.count < 2) return [];
    const out: [string, string][] = [["Count", String(a.count)]];
    if (a.numeric) {
      out.push(["Sum", a.sum ?? ""], ["Avg", a.avg ?? ""]);
    }
    out.push(["Min", valueText(a.min)], ["Max", valueText(a.max)], ["Distinct", String(a.distinct)]);
    if (a.nulls) out.push(["Nulls", String(a.nulls)]);
    return out;
  });
  return (
    <Show when={parts().length}>
      <button class="db-aggbar" title={t("database.dataTools.aggregateView")} data-testid="db-aggbar" onClick={() => props.onOpen?.()}>
        <For each={parts()}>
          {([k, v]) => (
            <span class="db-agg">
              <span class="muted">{k}:</span> {v}
            </span>
          )}
        </For>
      </button>
    </Show>
  );
}

/** DataGrip's Aggregate view: per selected column. */
export function AggregatePanel(props: { aggregates: Aggregates | null; onClose: () => void }) {
  return (
    <div class="db-side" data-testid="db-aggregate-view">
      <div class="db-side-head">
        <span class="db-side-title">{t("database.dataTools.aggregateView")}</span>
        <span class="db-spacer" />
        <button class="icon-btn" title={t("common.close")} onClick={props.onClose}>
          <X size={14} />
        </button>
      </div>
      <Show when={props.aggregates} fallback={<div class="db-side-empty muted">{t("database.dataTools.selectCellsToAggregateThem")}</div>}>
        {(a) => (
          <div class="db-side-body">
            <For each={a().columns}>
              {(c) => (
                <div class="db-agg-col">
                  <div class="db-agg-name">{c.column}</div>
                  <table class="db-agg-table">
                    <tbody>
                      <tr>
                        <td>{t("database.dataTools.count")}</td>
                        <td>{c.count}</td>
                      </tr>
                      <Show when={c.numeric}>
                        <tr>
                          <td>{t("database.dataTools.sum")}</td>
                          <td>{c.sum}</td>
                        </tr>
                        <tr>
                          <td>{t("database.dataTools.avg")}</td>
                          <td>{c.avg}</td>
                        </tr>
                      </Show>
                      <tr>
                        <td>{t("database.dataTools.min")}</td>
                        <td>{valueText(c.min)}</td>
                      </tr>
                      <tr>
                        <td>{t("database.dataTools.max")}</td>
                        <td>{valueText(c.max)}</td>
                      </tr>
                      <tr>
                        <td>{t("database.dataTools.distinct")}</td>
                        <td>{c.distinct}</td>
                      </tr>
                      <tr>
                        <td>{t("database.dataTools.nulls")}</td>
                        <td>{c.nulls}</td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              )}
            </For>
          </div>
        )}
      </Show>
    </div>
  );
}

// ---- transposed view -----------------------------------------------------------------

/** Rows as columns: one line per column, one column per row (DataGrip's Transpose). */
export function TransposedGrid(props: {
  columns: GridColumn[];
  count: number;
  valueAt: (row: number, col: number) => Cell | undefined;
  rowLabel: (row: number) => string;
  selected: { row: number; col: number } | null;
  onSelect: (row: number, col: number) => void;
  onOpen?: (row: number, col: number) => void;
  modified?: (row: number, col: number) => boolean;
  deleted?: (row: number) => boolean;
  nullText?: string;
}) {
  const LIMIT = 200;
  const rows = () => Array.from({ length: Math.min(props.count, LIMIT) }, (_, i) => i);
  return (
    <div class="dg dg-transposed" tabIndex={0} data-testid="db-transposed" data-focus-context="toolwindow">
      <table class="dgt">
        <thead>
          <tr>
            <th class="dgt-corner" />
            <For each={rows()}>{(r) => <th class="dgt-rownum" classList={{ deleted: props.deleted?.(r) }}>{props.rowLabel(r)}</th>}</For>
          </tr>
        </thead>
        <tbody>
          <For each={props.columns}>
            {(c, ci) => (
              <tr>
                <th class="dgt-col" title={`${c.name}: ${c.dataType ?? c.typeName}`}>
                  <Show when={c.primary} fallback={<Show when={c.foreign}><Link2 size={12} class="dg-fk" /></Show>}>
                    <KeyRound size={12} class="dg-pk" />
                  </Show>
                  <span class="dg-th-name">{c.name}</span>
                  <span class="dg-th-type">{typeLabel(c)}</span>
                </th>
                <For each={rows()}>
                  {(r) => {
                    const shown = () => {
                      const v = props.valueAt(r, ci());
                      return v === undefined ? { text: "", cls: "null" } : renderCell(v, c.kind, props.nullText);
                    };
                    return (
                      <td
                        class={`dgt-td ${shown().cls ?? ""}`}
                        classList={{
                          cursor: props.selected?.row === r && props.selected?.col === ci(),
                          modified: props.modified?.(r, ci()),
                          deleted: props.deleted?.(r),
                          right: c.kind === "number",
                        }}
                        onMouseDown={() => props.onSelect(r, ci())}
                        onDblClick={() => props.onOpen?.(r, ci())}
                      >
                        {shown().text}
                      </td>
                    );
                  }}
                </For>
              </tr>
            )}
          </For>
        </tbody>
      </table>
      <Show when={props.count > LIMIT}>
        <div class="dgt-more muted">Showing the first {LIMIT} of {props.count} rows</div>
      </Show>
    </div>
  );
}

// ---- pending changes ------------------------------------------------------------------

/** Pending changes with the generated DML (Rust) and Submit / Revert. */
export function PendingPanel(props: {
  count: number;
  preview: ChangePreview | null;
  error: string | null;
  loading: boolean;
  onSubmit: () => void;
  onRevert: () => void;
  onClose: () => void;
}) {
  return (
    <div class="db-pending" data-testid="db-pending">
      <div class="db-side-head">
        <span class="db-side-title">
          Pending Changes <span class="db-badge">{props.count}</span>
        </span>
        <Show when={props.preview?.key}>
          {(k) => (
            <span class="muted db-pending-key">
              rows identified by {k().source} ({k().columns.join(", ")})
            </span>
          )}
        </Show>
        <span class="db-spacer" />
        <button class="db-btn" disabled={!props.count} onClick={props.onRevert}>
          <Undo2 size={13} /> {t("database.dataTools.revert")}
        </button>
        <button class="db-btn primary" disabled={!props.count || !!props.error || !!props.preview?.blocked || props.loading} onClick={props.onSubmit}>
          <Upload size={13} /> {t("database.dataTools.submit")}
        </button>
        <button class="icon-btn" title={t("common.close")} onClick={props.onClose}>
          <X size={14} />
        </button>
      </div>
      <Show when={props.preview?.blocked}>
        <div class="db-pending-note danger">{props.preview!.blocked}</div>
      </Show>
      <Show when={props.preview?.confirm}>
        <div class="db-pending-note warning">{t("database.dataTools.productionDataSourceSubmitAsks")}</div>
      </Show>
      <Show when={props.error}>
        <div class="db-pending-note danger">{props.error}</div>
      </Show>
      <pre class="db-dml" data-testid="db-dml">
        <For each={props.preview?.statements ?? []}>{(s) => <div class="db-dml-line">{s};</div>}</For>
      </pre>
    </div>
  );
}
