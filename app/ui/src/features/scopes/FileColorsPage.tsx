/**
 * Settings | Appearance & Behavior | File Colors, like PHPStorm's page: "Enable file colors",
 * "Use in editor tabs", "Use in project view", and the Local and Shared color tables (scope
 * and color; the first scope that contains a file wins, local rows first). Share / Unshare
 * move a row between the tables. Edits go to the dialog draft.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { ArrowDown, ArrowUp, Minus, Plus, Share2, Undo2 } from "lucide-solid";
import { draftValue, setDraft } from "@/features/settings/dialogState";
import type { FileColor, NamedScope } from "@/ipc/scopes";
import { LOCAL_COLORS, swatchVar, LOCAL_SCOPES, PALETTE, SHARED_COLORS, SHARED_SCOPES, scopeEntries } from "./model";

type Table = "local" | "shared";
const keyOf = (t: Table) => (t === "shared" ? SHARED_COLORS : LOCAL_COLORS);

function Check(props: { key: string; label: string; disabled?: boolean }) {
  return (
    <label class="set-check scp-check" classList={{ disabled: props.disabled }}>
      <input
        type="checkbox"
        checked={draftValue<boolean>(props.key) !== false}
        disabled={props.disabled}
        onChange={(e) => setDraft(props.key, e.currentTarget.checked)}
      />
      <span>{props.label}</span>
    </label>
  );
}

export function FileColorsPage() {
  const rows = (t: Table) => draftValue<FileColor[]>(keyOf(t)) ?? [];
  const setRows = (t: Table, r: FileColor[]) => setDraft(keyOf(t), r);
  const [selected, setSelected] = createSignal<{ table: Table; index: number } | null>(null);
  const enabled = () => draftValue<boolean>("fileColors.enabled") !== false;
  const scopeNames = createMemo(() => {
    const set = {
      shared: draftValue<NamedScope[]>(SHARED_SCOPES) ?? [],
      local: draftValue<NamedScope[]>(LOCAL_SCOPES) ?? [],
    };
    const names = ["Non-Project Files", "Tests", ...scopeEntries(set).entries.map((e) => e.name)];
    return [...new Set(names)];
  });

  const edit = (t: Table, i: number, patch: Partial<FileColor>) =>
    setRows(
      t,
      rows(t).map((r, k) => (k === i ? { ...r, ...patch } : r)),
    );
  const add = (t: Table) => {
    const used = new Set(rows(t).map((r) => r.scope));
    const scope = scopeNames().find((n) => !used.has(n)) ?? "Project Files";
    setRows(t, [...rows(t), { scope, color: "Blue" }]);
    setSelected({ table: t, index: rows(t).length - 1 });
  };
  const remove = (t: Table) => {
    const s = selected();
    if (!s || s.table !== t) return;
    setRows(
      t,
      rows(t).filter((_, k) => k !== s.index),
    );
    setSelected(null);
  };
  const move = (t: Table, delta: number) => {
    const s = selected();
    if (!s || s.table !== t) return;
    const r = [...rows(t)];
    const j = s.index + delta;
    if (j < 0 || j >= r.length) return;
    [r[s.index], r[j]] = [r[j], r[s.index]];
    setRows(t, r);
    setSelected({ table: t, index: j });
  };
  const share = (from: Table) => {
    const s = selected();
    if (!s || s.table !== from) return;
    const to: Table = from === "local" ? "shared" : "local";
    const row = rows(from)[s.index];
    setRows(
      from,
      rows(from).filter((_, k) => k !== s.index),
    );
    setRows(to, [...rows(to), row]);
    setSelected({ table: to, index: rows(to).length - 1 });
  };

  const TableView = (props: { table: Table; title: string }) => {
    const sel = (i: number) => selected()?.table === props.table && selected()?.index === i;
    const has = () => selected()?.table === props.table;
    return (
      <div class="scp-colors">
        <div class="set-section-title">{props.title}</div>
        <div class="set-list scp-colors-list" classList={{ disabled: !enabled() }}>
          <div class="set-list-toolbar">
            <button class="icon-btn" title="Add" aria-label={`Add ${props.title}`} onClick={() => add(props.table)}>
              <Plus size={14} />
            </button>
            <button class="icon-btn" title="Remove" aria-label="Remove" disabled={!has()} onClick={() => remove(props.table)}>
              <Minus size={14} />
            </button>
            <button class="icon-btn" title="Move Up" aria-label="Move Up" disabled={!has()} onClick={() => move(props.table, -1)}>
              <ArrowUp size={14} />
            </button>
            <button class="icon-btn" title="Move Down" aria-label="Move Down" disabled={!has()} onClick={() => move(props.table, 1)}>
              <ArrowDown size={14} />
            </button>
            <button
              class="icon-btn"
              title={props.table === "local" ? "Share" : "Unshare"}
              aria-label={props.table === "local" ? "Share" : "Unshare"}
              disabled={!has()}
              onClick={() => share(props.table)}
            >
              {props.table === "local" ? <Share2 size={14} /> : <Undo2 size={14} />}
            </button>
          </div>
          <table class="set-table scp-colors-table">
            <thead>
              <tr>
                <th>Scope</th>
                <th>Color</th>
              </tr>
            </thead>
            <tbody>
              <For each={rows(props.table)}>
                {(row, i) => (
                  <tr classList={{ selected: sel(i()) }} onClick={() => setSelected({ table: props.table, index: i() })}>
                    <td>
                      <select
                        class="scp-cell-select"
                        value={row.scope}
                        aria-label="Scope"
                        onChange={(e) => edit(props.table, i(), { scope: e.currentTarget.value })}
                      >
                        <For each={scopeNames().includes(row.scope) ? scopeNames() : [row.scope, ...scopeNames()]}>
                          {(n) => <option value={n}>{n}</option>}
                        </For>
                      </select>
                    </td>
                    <td class="scp-color-cell">
                      <span class="scp-color-swatch" style={{ background: swatchVar(row.color) }} />
                      <select
                        class="scp-cell-select"
                        value={row.color}
                        aria-label="Color"
                        onChange={(e) => edit(props.table, i(), { color: e.currentTarget.value })}
                      >
                        <For each={PALETTE}>{(c) => <option value={c}>{c}</option>}</For>
                      </select>
                    </td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
          <Show when={!rows(props.table).length}>
            <div class="set-list-empty">No colors</div>
          </Show>
        </div>
      </div>
    );
  };

  return (
    <div class="scp-colors-page">
      <Check key="fileColors.enabled" label="Enable file colors" />
      <div class="set-indent scp-indent">
        <Check key="fileColors.useInEditorTabs" label="Use in editor tabs" disabled={!enabled()} />
        <Check key="fileColors.useInProjectView" label="Use in project view" disabled={!enabled()} />
      </div>
      <TableView table="local" title="Local colors" />
      <TableView table="shared" title="Shared colors" />
      <div class="set-desc">
        Shared colors are stored in the project (<code>.forge/settings.toml</code>) and shared through version control.
        Manage scopes on the Scopes page.
      </div>
    </div>
  );
}
