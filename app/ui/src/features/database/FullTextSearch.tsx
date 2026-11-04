/**
 * Full-Text Search (DataGrip's Ctrl+Alt+Shift+F in the Database tool window): an editor tab
 * with the text, options and scope (data sources, schemas, table patterns) on top, and the
 * matching rows per table below while the backend searches table by table (`db_search`,
 * progress as `db://search`). Searching only reads data.
 */
import { createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { ChevronDown, ChevronRight, Database, Layers, LoaderCircle, Search, Square, Table2, TriangleAlert } from "lucide-solid";
import { dbApi, modelOf, type SearchMode, type SearchProgress, type SearchSummary, type TableMatches } from "@/ipc/db";
import type { EditorTab } from "@/state/editor";
import { openViewTab } from "@/state/editor";
import { renderCell, kindOf } from "./grid";
import { connect, ensureSchema, openTable, sourceById, sources, states, visibleSchemas } from "./store";
import { t as tr } from "@/i18n";

export const SEARCH_VIEW = "db-search";

export interface SearchStart {
  /** Data source, schema and table the search starts with (the selected node). */
  ds?: string;
  schema?: string;
  table?: string;
}

const starts = new Map<string, SearchStart>();
let seq = 0;

/** Open a Full-Text Search tab scoped to the selected node. */
export function openFullTextSearch(start: SearchStart = {}): void {
  const path = `db-search://${++seq}`;
  starts.set(path, start);
  openViewTab({ path, name: "Full-Text Search", view: SEARCH_VIEW });
}

interface ScopeState {
  /** Data source id → selected schemas (empty: all visible). */
  sources: Record<string, string[]>;
  tables: string;
}

export function FullTextSearchView(props: { tab: EditorTab }) {
  const start = starts.get(props.tab.path) ?? {};
  const searchable = () => sources().filter((s) => modelOf(s.driver) === "relational" && s.driver !== "ddl");
  const initial: ScopeState = {
    sources: start.ds ? { [start.ds]: start.schema ? [start.schema] : [] } : Object.fromEntries(searchable().slice(0, 1).map((s) => [s.id, []])),
    tables: start.table ?? "",
  };
  const [scope, setScope] = createStore<ScopeState>(initial);
  const [text, setText] = createSignal("");
  const [mode, setMode] = createSignal<SearchMode>("contains");
  const [caseSensitive, setCaseSensitive] = createSignal(false);
  const [allColumns, setAllColumns] = createSignal(false);
  const [limit, setLimit] = createSignal(100);
  const [running, setRunning] = createSignal<string | null>(null);
  const [progress, setProgress] = createSignal<SearchProgress | null>(null);
  const [results, setResults] = createStore<TableMatches[]>([]);
  const [summary, setSummary] = createSignal<SearchSummary | null>(null);
  const [error, setError] = createSignal("");
  const [collapsed, setCollapsed] = createSignal<Set<string>>(new Set());

  for (const id of Object.keys(initial.sources)) void connect(id);

  const stop = dbApi.onSearch((p) => {
    if (p.searchId !== running()) return;
    setProgress(p);
    if (p.result) setResults(produce((r) => r.push(p.result!)));
  });
  onCleanup(() => {
    void stop.then((f) => f());
    const id = running();
    if (id) void dbApi.cancelSearch(id);
  });

  const toggleSource = (id: string, on: boolean) => {
    setScope(
      "sources",
      produce((m) => {
        if (on) {
          m[id] = [];
          void connect(id);
        } else delete m[id];
      }),
    );
  };
  const toggleSchema = (id: string, schema: string, on: boolean) => {
    setScope(
      "sources",
      id,
      produce((list: string[]) => {
        const i = list.indexOf(schema);
        if (on && i < 0) list.push(schema);
        if (!on && i >= 0) list.splice(i, 1);
      }),
    );
    if (on) ensureSchema(id, schema);
  };

  const run = async () => {
    if (!text() || running()) return;
    const id = `fts-${Date.now()}-${++seq}`;
    setResults([]);
    setSummary(null);
    setError("");
    setProgress(null);
    setRunning(id);
    try {
      const s = await dbApi.search(id, {
        scopes: Object.entries(scope.sources).map(([dataSource, schemas]) => ({
          dataSource,
          schemas: [...schemas],
          tables: scope.tables
            .split(/[,\s]+/)
            .map((x) => x.trim())
            .filter(Boolean),
        })),
        text: text(),
        mode: mode(),
        caseSensitive: caseSensitive(),
        allColumns: allColumns(),
        limit: limit(),
      });
      setSummary(s);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setRunning(null);
    }
  };

  const cancel = () => {
    const id = running();
    if (id) void dbApi.cancelSearch(id);
  };

  const pct = () => {
    const p = progress();
    return p && p.total ? Math.round((p.done / p.total) * 100) : 0;
  };
  const key = (r: TableMatches) => `${r.dataSource}/${r.schema}/${r.table}`;
  const matched = createMemo(() => results.filter((r) => r.rows.length));
  const failed = createMemo(() => results.filter((r) => r.error));

  return (
    <div class="fts" data-testid="db-full-text-search">
      <div class="fts-form">
        <div class="fts-row">
          <Search size={15} class="muted" />
          <input
            class="dsd-input fts-text"
            placeholder={tr("database.fullTextSearch.textToFindInThe")}
            value={text()}
            spellcheck={false}
            data-testid="fts-text"
            onInput={(e) => setText(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void run();
            }}
          />
          <select class="dsd-input fts-mode" value={mode()} onChange={(e) => setMode(e.currentTarget.value as SearchMode)}>
            <option value="contains">{tr("database.fullTextSearch.contains")}</option>
            <option value="exact">{tr("database.fullTextSearch.exactMatch")}</option>
            <option value="regex">{tr("database.fullTextSearch.regularExpression")}</option>
          </select>
          <Show
            when={running()}
            fallback={
              <button class="btn-primary" disabled={!text() || !Object.keys(scope.sources).length} onClick={() => void run()} data-testid="fts-run">
                {tr("database.fullTextSearch.search")}
              </button>
            }
          >
            <button class="btn-secondary" onClick={cancel} data-testid="fts-cancel">
              <Square size={12} /> {tr("common.cancel")}
            </button>
          </Show>
        </div>
        <div class="fts-row fts-options">
          <label class="dsd-check">
            <input type="checkbox" checked={caseSensitive()} onChange={(e) => setCaseSensitive(e.currentTarget.checked)} />
            {tr("database.fullTextSearch.matchCase")}
          </label>
          <label class="dsd-check" title={tr("database.fullTextSearch.alsoSearchNumbersDatesAnd")}>
            <input type="checkbox" checked={allColumns()} onChange={(e) => setAllColumns(e.currentTarget.checked)} />
            {tr("database.fullTextSearch.allColumnTypes")}
          </label>
          <label class="dsd-check">
            {tr("database.fullTextSearch.rowsPerTable")}
            <input class="dsd-input dsd-port" value={limit()} inputMode="numeric" onInput={(e) => setLimit(Math.max(1, Number(e.currentTarget.value) || 100))} />
          </label>
          <label class="dsd-check fts-tables">
            {tr("database.fullTextSearch.tables")}
            <input class="dsd-input" placeholder={tr("database.fullTextSearch.allOrUsersOrderAudit")} value={scope.tables} spellcheck={false} onInput={(e) => setScope("tables", e.currentTarget.value)} />
          </label>
        </div>
        <div class="fts-scope">
          <For each={searchable()} fallback={<span class="muted">{tr("database.fullTextSearch.noSqlDataSourcesTo")}</span>}>
            {(ds) => {
              const on = () => scope.sources[ds.id] !== undefined;
              return (
                <div class="fts-scope-source">
                  <label class="dsd-check">
                    <input type="checkbox" checked={on()} onChange={(e) => toggleSource(ds.id, e.currentTarget.checked)} />
                    <Database size={13} class="db-ic-schema" />
                    <b>{ds.name}</b>
                  </label>
                  <Show when={on()}>
                    <For each={visibleSchemas(ds.id)}>
                      {(s) => (
                        <label class="dsd-check fts-schema">
                          <input type="checkbox" checked={(scope.sources[ds.id] ?? []).includes(s.name)} onChange={(e) => toggleSchema(ds.id, s.name, e.currentTarget.checked)} />
                          <Layers size={12} class="db-ic-schema" />
                          {s.name}
                        </label>
                      )}
                    </For>
                    <Show when={!(scope.sources[ds.id] ?? []).length}>
                      <span class="muted fts-all">{tr("database.fullTextSearch.allShownSchemas")}</span>
                    </Show>
                  </Show>
                </div>
              );
            }}
          </For>
        </div>
      </div>
      <Show when={running() || summary()}>
        <div class="fts-progress" data-testid="fts-progress">
          <div class="fts-bar">
            <div class="fts-bar-fill" classList={{ done: !running() }} style={{ width: `${running() ? pct() : 100}%` }} />
          </div>
          <span class="fts-status">
            <Show
              when={running()}
              fallback={
                <Show when={summary()}>
                  {(s) =>
                    `${s().rows} rows in ${s().matchedTables} of ${s().tables} tables · ${s().elapsedMs} ms${s().cancelled ? " · cancelled" : ""}${s().errors ? ` · ${s().errors} errors` : ""}`
                  }
                </Show>
              }
            >
              <LoaderCircle size={12} class="db-spin" /> {progress() ? `${progress()!.done} / ${progress()!.total} · ${progress()!.current}` : "Preparing…"}
            </Show>
          </span>
        </div>
      </Show>
      <Show when={error()}>
        <div class="fts-error">
          <TriangleAlert size={14} /> {error()}
        </div>
      </Show>
      <div class="fts-results">
        <For each={matched()}>
          {(r) => {
            const open = () => !collapsed().has(key(r));
            const toggle = () =>
              setCollapsed((prev) => {
                const next = new Set(prev);
                if (next.has(key(r))) next.delete(key(r));
                else next.add(key(r));
                return next;
              });
            const table = () => states[r.dataSource]?.introspected[r.schema]?.tables.find((t) => t.name === r.table);
            const kinds = () => r.columns.map((c) => kindOf(table()?.columns.find((x) => x.name === c)?.dataType ?? "text"));
            return (
              <div class="fts-table">
                <div class="fts-table-head" onClick={toggle} onDblClick={() => openTable(r.dataSource, r.schema, r.table)} title={tr("database.fullTextSearch.doubleClickToOpenThe")}>
                  {open() ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                  <Table2 size={14} class="db-ic-table" />
                  <b>{r.table}</b>
                  <span class="muted">
                    {sourceById(r.dataSource)?.name} · {r.schema}
                  </span>
                  <span class="fts-count">
                    {r.rows.length}
                    {r.truncated ? "+" : ""} {r.rows.length === 1 && !r.truncated ? "row" : "rows"}
                  </span>
                </div>
                <Show when={open()}>
                  <div class="fts-grid-wrap">
                    <table class="fts-grid">
                      <thead>
                        <tr>
                          <For each={r.columns}>{(c) => <th>{c}</th>}</For>
                        </tr>
                      </thead>
                      <tbody>
                        <For each={r.rows}>
                          {(row, i) => (
                            <tr onDblClick={() => openTable(r.dataSource, r.schema, r.table)}>
                              <For each={row}>
                                {(cell, j) => {
                                  const shown = renderCell(cell, kinds()[j()] ?? "text");
                                  return (
                                    <td class={shown.cls ?? ""} classList={{ hit: r.matches[i()]?.includes(j()) }}>
                                      {shown.text}
                                    </td>
                                  );
                                }}
                              </For>
                            </tr>
                          )}
                        </For>
                      </tbody>
                    </table>
                  </div>
                </Show>
              </div>
            );
          }}
        </For>
        <For each={failed()}>
          {(r) => (
            <div class="fts-table-error">
              <TriangleAlert size={13} />
              <b>{r.table || sourceById(r.dataSource)?.name}</b>
              <span class="muted">{r.error}</span>
            </div>
          )}
        </For>
        <Show when={summary() && !matched().length && !failed().length}>
          <div class="muted fts-none">No rows contain “{text()}”.</div>
        </Show>
      </div>
    </div>
  );
}
