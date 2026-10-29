/**
 * Redis value viewer tab: the key's type, TTL and size, and its value: strings as text (JSON
 * pretty-printed), lists and sets as numbered rows, hashes as field/value rows, sorted sets
 * with their scores and streams as JSON.
 */
import { createResource, Match, Show, Switch } from "solid-js";
import { Copy, RefreshCw, SquareTerminal } from "lucide-solid";
import { dbApi, type Cell, type RedisValue } from "@/ipc/db";
import type { EditorTab } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { DataGrid } from "./DataGrid";
import { gridColumns } from "./grid";
import { connect, openConsole, parseRedisPath, sourceColor } from "./store";
import { ttlText } from "./DatabaseView";

const cell = (v: unknown): Cell =>
  v === null || v === undefined ? null : typeof v === "object" ? JSON.stringify(v) : (v as Cell);

/** The value as grid columns and rows (strings and streams are shown as text). */
export function valueTable(v: RedisValue): { columns: string[]; rows: Cell[][] } | null {
  const value = v.value;
  switch (v.type) {
    case "hash":
      return { columns: ["field", "value"], rows: Object.entries((value ?? {}) as Record<string, unknown>).map(([k, x]) => [k, cell(x)]) };
    case "zset":
      return {
        columns: ["#", "member", "score"],
        rows: ((value ?? []) as { member: unknown; score: unknown }[]).map((m, i) => [i + 1, cell(m.member), cell(m.score)]),
      };
    case "list":
    case "set":
      return { columns: ["#", "value"], rows: ((value ?? []) as unknown[]).map((x, i) => [i + 1, cell(x)]) };
    default:
      return null;
  }
}

/** A string value as text: JSON pretty-printed. */
export function valueText(v: RedisValue): string {
  if (typeof v.value === "string") {
    try {
      const parsed = JSON.parse(v.value);
      if (parsed && typeof parsed === "object") return JSON.stringify(parsed, null, 2);
    } catch {
      // not JSON
    }
    return v.value;
  }
  return JSON.stringify(v.value, null, 2);
}

export function RedisValueView(props: { tab: EditorTab }) {
  const loc = parseRedisPath(props.tab.path)!;
  const [value, { refetch }] = createResource(async () => {
    await connect(loc.id);
    return dbApi.redisValue(loc.id, loc.schema, loc.key);
  });
  const color = () => sourceColor(loc.id);
  const table = () => (value() ? valueTable(value()!) : null);
  return (
    <div class="db-redis" classList={{ colored: !!color() }} style={{ "--db-tint": color()?.tint, "--db-accent": color()?.accent }} data-testid="db-redis-value">
      <div class="db-toolbar">
        <button class="icon-btn db-tool" title="Reload" onClick={() => void refetch()}>
          <RefreshCw size={14} />
        </button>
        <button class="icon-btn db-tool" title="Copy Value" disabled={!value()} onClick={() => void navigator.clipboard?.writeText(valueText(value()!))}>
          <Copy size={14} />
        </button>
        <button class="icon-btn db-tool" title="Open in Console" onClick={() => openConsole(loc.id, `TYPE ${loc.key}\nTTL ${loc.key}\n`)}>
          <SquareTerminal size={14} />
        </button>
        <span class="db-sep" />
        <span class="db-redis-key" title={loc.key}>
          {loc.key}
        </span>
        <Show when={value()}>
          {(v) => (
            <>
              <span class={`db-redis-type ${v().type}`}>{v().type}</span>
              <span class="muted db-status">
                {v().type === "string" ? `${v().length} bytes` : `${v().length} element${v().length === 1 ? "" : "s"}`}
                {v().truncated ? ` (first ${table()?.rows.length ?? 0} shown)` : ""} · {v().ttlMs >= 0 ? `TTL ${ttlText(v().ttlMs)}` : "no expiry"}
              </span>
            </>
          )}
        </Show>
        <span class="db-spacer" />
        <span class="muted db-status">{loc.schema}</span>
      </div>
      <Switch>
        <Match when={value.error}>
          <div class="db-redis-error">{String(value.error instanceof Error ? value.error.message : value.error)}</div>
        </Match>
        <Match when={table()}>
          {(t) => (
            <DataGrid
              columns={gridColumns(t().columns.map((name) => ({ name, typeName: name === "#" || name === "score" ? "INT" : "TEXT" })))}
              rows={t().rows}
              nullText={getSetting<string>("database.showNullAs", "<null>") ?? "<null>"}
              emptyText="The key is empty"
            />
          )}
        </Match>
        <Match when={value()}>{(v) => <pre class="db-redis-text">{valueText(v())}</pre>}</Match>
      </Switch>
    </div>
  );
}
