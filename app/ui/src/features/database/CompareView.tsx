/**
 * Compare data (DataGrip's "Compare With…"): pick two result sets or tables, diff them by
 * key in Rust (`forge_dataedit::compare`) and show added, removed and changed rows with the
 * changed cells highlighted, in an editor tab.
 */
import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { GitCompare, X } from "lucide-solid";
import { dbApi, type Cell } from "@/ipc/db";
import { dataEditApi, type DataDiff, type DataSet, type DiffRow, type RowStatus } from "@/ipc/dataedit";
import { openViewTab, type EditorTab } from "@/state/editor";
import { renderCell, kindOf, cellText } from "./grid";
import { sourceById, states } from "./store";

export const COMPARE_VIEW = "db-compare";

/** Rows loaded per side of a table compare. */
const TABLE_LIMIT = 5000;

/** Something that can be compared: an open table editor, a console result or a table. */
export interface CompareSource {
  key: string;
  label: string;
  /** Primary key columns, the default compare key. */
  primaryKey?: string[];
  load: () => Promise<DataSet>;
}

/** Open data sets (table editors and console results register here while open). */
const [openSets, setOpenSets] = createStore<Record<string, CompareSource>>({});

export function registerCompareSource(src: CompareSource): () => void {
  setOpenSets(src.key, src);
  return () =>
    setOpenSets(
      produce((s) => {
        delete s[src.key];
      }),
    );
}

/** A table as a compare source (its first rows). */
export function tableSource(id: string, schema: string, table: string, filter?: string): CompareSource {
  const ds = sourceById(id);
  const info = states[id]?.introspected[schema]?.tables.find((t) => t.name === table);
  return {
    key: `table:${id}/${schema}/${table}${filter ? `?${filter}` : ""}`,
    label: `${ds?.name ?? id} · ${schema}.${table}${filter ? ` WHERE ${filter}` : ""}`,
    primaryKey: info?.primaryKey,
    load: async () => {
      const page = await dbApi.tableData(id, { schema, table, filter, limit: TABLE_LIMIT });
      return { columns: page.columns.map((c) => c.name), rows: page.rows };
    },
  };
}

/** Tables of connected data sources (the right side's other candidates). */
function tableSources(): CompareSource[] {
  const out: CompareSource[] = [];
  for (const [id, st] of Object.entries(states)) {
    for (const [schema, info] of Object.entries(st?.introspected ?? {})) {
      for (const t of info?.tables ?? []) if (t.kind !== "collection") out.push(tableSource(id, schema, t.name));
    }
  }
  return out;
}

interface CompareRequest {
  left: CompareSource;
  right: CompareSource;
  key: string[];
}

const requests = new Map<string, CompareRequest>();
let compareSeq = 0;

export function openCompare(req: CompareRequest): void {
  const path = `db-compare://${++compareSeq}`;
  requests.set(path, req);
  openViewTab({ path, name: `${short(req.left.label)} ↔ ${short(req.right.label)}`, view: COMPARE_VIEW });
}

const short = (label: string) => label.split(" · ").pop()!.split(" WHERE ")[0];

/** The "Compare With…" popup: pick the other side and the key. */
export function CompareDialog(props: { left: CompareSource; onClose: () => void }) {
  const candidates = createMemo(() => {
    const seen = new Set([props.left.key]);
    const list: CompareSource[] = [];
    for (const s of [...Object.values(openSets), ...tableSources()]) {
      if (!s || seen.has(s.key)) continue;
      seen.add(s.key);
      list.push(s);
    }
    return list;
  });
  const [right, setRight] = createSignal<string>("");
  const picked = () => candidates().find((c) => c.key === right()) ?? candidates()[0];
  const [leftCols] = createResource(() => props.left, (s) => s.load().then((d) => d.columns).catch(() => [] as string[]));
  const [rightCols] = createResource(picked, (s) => s.load().then((d) => d.columns).catch(() => [] as string[]));
  const common = () => (leftCols() ?? []).filter((c) => (rightCols() ?? []).includes(c));
  const [key, setKey] = createSignal<string[] | null>(null);
  const effectiveKey = () => key() ?? (props.left.primaryKey ?? []).filter((k) => common().includes(k));
  const toggle = (c: string) => {
    const k = effectiveKey();
    setKey(k.includes(c) ? k.filter((x) => x !== c) : [...k, c]);
  };
  return (
    <div class="db-overlay" onMouseDown={(e) => e.target === e.currentTarget && props.onClose()}>
      <div class="db-dialog" data-testid="db-compare-dialog" role="dialog">
        <div class="db-side-head">
          <GitCompare size={14} />
          <span class="db-side-title">Compare Data</span>
          <span class="db-spacer" />
          <button class="icon-btn" title="Close" onClick={props.onClose}>
            <X size={14} />
          </button>
        </div>
        <div class="db-dialog-body">
          <label class="db-dialog-row">
            <span class="muted">Left</span>
            <span class="db-dialog-value">{props.left.label}</span>
          </label>
          <label class="db-dialog-row">
            <span class="muted">Right</span>
            <select value={picked()?.key ?? ""} onChange={(e) => {
              setRight(e.currentTarget.value);
              setKey(null);
            }}>
              <For each={candidates()}>{(c) => <option value={c.key}>{c.label}</option>}</For>
            </select>
          </label>
          <div class="db-dialog-row">
            <span class="muted">Key</span>
            <div class="db-chart-ys">
              <For each={common()}>
                {(c) => (
                  <button class="db-ychip" classList={{ active: effectiveKey().includes(c) }} onClick={() => toggle(c)}>
                    {c}
                  </button>
                )}
              </For>
              <Show when={!effectiveKey().length}>
                <span class="muted">No key: whole rows are matched</span>
              </Show>
            </div>
          </div>
        </div>
        <div class="db-side-foot">
          <span class="db-spacer" />
          <button class="db-btn" onClick={props.onClose}>
            Cancel
          </button>
          <button
            class="db-btn primary"
            disabled={!picked()}
            onClick={() => {
              openCompare({ left: props.left, right: picked()!, key: effectiveKey() });
              props.onClose();
            }}
          >
            Compare
          </button>
        </div>
      </div>
    </div>
  );
}

const STATUS_MARK: Record<RowStatus, string> = { same: "", changed: "~", added: "+", removed: "−" };

export function CompareView(props: { tab: EditorTab }) {
  const req = requests.get(props.tab.path);
  const [filter, setFilter] = createSignal<Record<RowStatus, boolean>>({ same: false, changed: true, added: true, removed: true });
  const [diff] = createResource(async (): Promise<DataDiff | { error: string }> => {
    if (!req) return { error: "This comparison is no longer available" };
    try {
      const [l, r] = await Promise.all([req.left.load(), req.right.load()]);
      return await dataEditApi.compare(l, r, req.key);
    } catch (e) {
      return { error: e instanceof Error ? e.message : String(e) };
    }
  });
  const d = () => {
    const x = diff();
    return x && !("error" in x) ? x : null;
  };
  const rows = createMemo(() => (d()?.rows ?? []).filter((r) => filter()[r.status]).slice(0, 3000));
  const kinds = createMemo(() => {
    const x = d();
    if (!x) return [];
    return x.columns.map((_, i) => {
      const sample = x.rows.map((r) => (r.left ?? r.right)?.[i]).find((v) => v !== null && v !== undefined);
      return typeof sample === "number" ? kindOf("int") : kindOf("text");
    });
  });
  const cell = (row: DiffRow, i: number) => {
    const changed = row.changed?.includes(i);
    const v: Cell | undefined = row.status === "added" ? row.right?.[i] : row.left?.[i];
    const shown = renderCell(v ?? null, kinds()[i] ?? "text");
    return (
      <td class={`dc-td ${shown.cls ?? ""}`} classList={{ changed }} title={changed ? `${cellText(row.left?.[i] ?? null)} → ${cellText(row.right?.[i] ?? null)}` : undefined}>
        <Show when={changed} fallback={shown.text}>
          <span class="dc-old">{renderCell(row.left?.[i] ?? null, kinds()[i] ?? "text").text}</span>
          <span class="dc-arrow">→</span>
          <span class="dc-new">{renderCell(row.right?.[i] ?? null, kinds()[i] ?? "text").text}</span>
        </Show>
      </td>
    );
  };
  const toggle = (s: RowStatus) => setFilter((f) => ({ ...f, [s]: !f[s] }));
  return (
    <div class="db-editor" data-db-path={props.tab.path} data-testid="db-compare">
      <div class="db-toolbar">
        <GitCompare size={15} class="muted" />
        <span class="dc-sides">
          <span class="dc-side-l">{req?.left.label}</span>
          <span class="muted"> ↔ </span>
          <span class="dc-side-r">{req?.right.label}</span>
        </span>
        <span class="db-spacer" />
        <Show when={d()}>
          {(x) => (
            <>
              <button class="dc-count changed" classList={{ off: !filter().changed }} onClick={() => toggle("changed")}>
                {x().changed} changed
              </button>
              <button class="dc-count added" classList={{ off: !filter().added }} onClick={() => toggle("added")}>
                {x().added} added
              </button>
              <button class="dc-count removed" classList={{ off: !filter().removed }} onClick={() => toggle("removed")}>
                {x().removed} removed
              </button>
              <button class="dc-count same" classList={{ off: !filter().same }} onClick={() => toggle("same")}>
                {x().same} unchanged
              </button>
            </>
          )}
        </Show>
      </div>
      <Show when={d()}>
        {(x) => (
          <div class="dc-info muted">
            Key: {x().key.length ? x().key.join(", ") : "whole rows"}
            <Show when={x().rightOnlyColumns?.length}> · only right: {x().rightOnlyColumns!.join(", ")}</Show>
            <Show when={x().leftOnlyColumns?.length}> · only left: {x().leftOnlyColumns!.join(", ")}</Show>
            <Show when={x().duplicateKeys}> · {x().duplicateKeys} duplicate keys</Show>
          </div>
        )}
      </Show>
      <Show when={diff() && "error" in diff()!}>
        <div class="db-error">{(diff() as { error: string }).error}</div>
      </Show>
      <Show when={diff.loading}>
        <div class="db-side-empty muted">Comparing…</div>
      </Show>
      <div class="dc-scroll">
        <Show when={d()}>
          {(x) => (
            <table class="dc-table">
              <thead>
                <tr>
                  <th class="dc-mark" />
                  <For each={x().columns}>{(c) => <th classList={{ key: x().key.includes(c) }}>{c}</th>}</For>
                </tr>
              </thead>
              <tbody>
                <For each={rows()}>
                  {(row) => (
                    <tr class={`dc-row ${row.status}`}>
                      <td class="dc-mark">{STATUS_MARK[row.status]}</td>
                      <For each={x().columns}>{(_, i) => cell(row, i())}</For>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          )}
        </Show>
        <Show when={d() && !rows().length}>
          <div class="db-side-empty muted">No differences shown</div>
        </Show>
      </div>
    </div>
  );
}
