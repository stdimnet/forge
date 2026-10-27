/**
 * Database tool window (DataGrip's Database Explorer): data sources → schemas → tables,
 * views, routines → columns, keys, foreign keys and indexes.
 */
import { createMemo, createSignal, For, Show, type JSX } from "solid-js";
import {
  ChevronDown,
  ChevronRight,
  Columns2,
  Copy,
  Database,
  Eye,
  FileCode,
  Folder,
  Key,
  KeyRound,
  Layers,
  Link2,
  ListOrdered,
  LoaderCircle,
  Plus,
  RefreshCw,
  Settings2,
  SquareFunction,
  SquareTerminal,
  Table2,
  Trash2,
  Unplug,
  FoldVertical,
  Activity,
  Braces,
  FileJson,
  Hash,
  List,
  ListTree,
  KeySquare,
  Search,
  Sigma,
  Type,
} from "lucide-solid";
import { dbApi, DRIVER_LABELS, DRIVERS, modelOf, type DataSourceInfo, type Driver, type RedisKey, type TableInfo } from "@/ipc/db";
import { showBalloon, shortcutLabel } from "@/keymap";
import { navigate, type VisibleRow } from "@/features/project/treeNavigation";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import {
  connect,
  dbPanel,
  disconnect,
  expanded,
  isConnected,
  loadRedisKeys,
  openCollection,
  openConsole,
  openDataSources,
  openRedisKey,
  redisKeys,
  setDbPanel,
  sourceColor,
  openTable,
  refresh,
  removeSource,
  revealSource,
  selectedNode,
  setNodeOpen,
  setSelectedNode,
  sourceById,
  sources,
  states,
  toggleNode,
  visibleSchemas,
} from "./store";
import { addFromEnv, detected } from "./envDetect";
import { SessionsView } from "./SessionsView";

type Kind =
  | "ds"
  | "schema"
  | "folder"
  | "table"
  | "view"
  | "collection"
  | "group"
  | "column"
  | "key"
  | "fk"
  | "index"
  | "routine"
  | "status"
  | "filter"
  | "rkey";

interface Row {
  key: string;
  depth: number;
  kind: Kind;
  label: string;
  hint?: string;
  expandable: boolean;
  icon: () => JSX.Element;
  ds: string;
  schema?: string;
  table?: string;
  cls?: string;
  /** Background of data source colours (production, staging, …). */
  tint?: string;
}

const DRIVER_COLORS: Record<Driver, string> = {
  mysql: "info",
  mariadb: "warning",
  postgres: "graph-1",
  sqlite: "success",
  sqlserver: "danger",
  clickhouse: "graph-4",
  oracle: "graph-0",
  redis: "graph-6",
  mongodb: "graph-2",
};

/** `59 min`, `2 d`, or "" without expiry. */
export function ttlText(ms: number): string {
  if (ms < 0) return "";
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s} s`;
  if (s < 3600) return `${Math.round(s / 60)} min`;
  if (s < 86400) return `${Math.round(s / 3600)} h`;
  return `${Math.round(s / 86400)} d`;
}

function redisIcon(type: string) {
  switch (type) {
    case "hash":
      return <Hash size={13} class="db-ic-redis hash" />;
    case "list":
      return <List size={13} class="db-ic-redis list" />;
    case "set":
      return <Braces size={13} class="db-ic-redis set" />;
    case "zset":
      return <Sigma size={13} class="db-ic-redis zset" />;
    case "stream":
      return <ListTree size={13} class="db-ic-redis stream" />;
    default:
      return <Type size={13} class="db-ic-redis string" />;
  }
}

function redisRows(id: string, schema: string, depth: number, out: Row[], tint?: string): void {
  const st = redisKeys[`${id}/${schema}`];
  out.push({ key: `filter:${id}/${schema}`, depth, kind: "filter", label: st?.pattern ?? "*", expandable: false, icon: () => <Search size={13} class="muted" />, ds: id, schema, tint });
  if (!st || st.loading) {
    out.push({ key: `status:redis:${id}/${schema}`, depth, kind: "status", label: "Loading keys…", expandable: false, icon: () => <LoaderCircle size={13} class="db-spin" />, ds: id, cls: "muted", tint });
    return;
  }
  if (st.error) {
    out.push({ key: `status:redis:${id}/${schema}`, depth, kind: "status", label: st.error, expandable: false, icon: () => <Unplug size={13} />, ds: id, cls: "db-row-error", tint });
    return;
  }
  for (const k of st.keys ?? []) {
    const key: RedisKey = k;
    const ttl = ttlText(key.ttlMs);
    out.push({
      key: `rkey:${id}/${schema}/${key.key}`,
      depth,
      kind: "rkey",
      label: key.key,
      hint: [key.type, ttl ? `TTL ${ttl}` : ""].filter(Boolean).join(" · "),
      expandable: false,
      icon: () => redisIcon(key.type),
      ds: id,
      schema,
      table: key.key,
      tint,
    });
  }
  if (st.truncated) out.push({ key: `status:more:${id}/${schema}`, depth, kind: "status", label: "More keys match; narrow the pattern", expandable: false, icon: () => <KeySquare size={13} class="muted" />, ds: id, cls: "muted", tint });
  else if (!st.keys?.length) out.push({ key: `status:none:${id}/${schema}`, depth, kind: "status", label: "No keys match", expandable: false, icon: () => <KeySquare size={13} class="muted" />, ds: id, cls: "muted", tint });
}

function tableChildren(ds: string, schema: string, t: TableInfo, depth: number, out: Row[]): void {
  const base = `${ds}/${schema}/${t.name}`;
  const group = (name: string, count: number, icon: () => JSX.Element, fill: (d: number) => void) => {
    if (!count) return;
    const key = `group:${base}/${name}`;
    out.push({ key, depth, kind: "group", label: name, hint: String(count), expandable: true, icon, ds, schema, table: t.name });
    if (expanded().has(key)) fill(depth + 1);
  };
  const doc = t.kind === "collection";
  group(doc ? "fields" : "columns", t.columns.length, () => <Folder size={14} class="db-ic-folder" />, (d) => {
    for (const c of t.columns) {
      const pk = t.primaryKey.includes(c.name);
      const fk = t.foreignKeys.some((f) => f.columns.includes(c.name));
      const extra = [c.dataType, c.autoIncrement ? "auto_increment" : "", !c.nullable && !pk ? "not null" : ""].filter(Boolean).join(" ");
      out.push({
        key: `column:${base}/${c.name}`,
        depth: d,
        kind: "column",
        label: c.name,
        hint: `${extra}${c.default != null && !c.autoIncrement ? ` = ${c.default}` : ""}`,
        expandable: false,
        icon: () => (pk ? <KeyRound size={13} class="db-ic-pk" /> : fk ? <Link2 size={13} class="db-ic-fk" /> : <Columns2 size={13} class="db-ic-column" />),
        ds,
        schema,
        table: t.name,
      });
    }
  });
  const pkIndex = t.indexes.find((i) => i.primary);
  group("keys", t.primaryKey.length && !doc ? 1 : 0, () => <Folder size={14} class="db-ic-folder" />, (d) =>
    out.push({ key: `key:${base}`, depth: d, kind: "key", label: pkIndex?.name ?? `${t.name}_pk`, hint: `(${t.primaryKey.join(", ")})`, expandable: false, icon: () => <Key size={13} class="db-ic-pk" />, ds, schema, table: t.name }),
  );
  group("foreign keys", t.foreignKeys.length, () => <Folder size={14} class="db-ic-folder" />, (d) => {
    for (const f of t.foreignKeys)
      out.push({
        key: `fk:${base}/${f.name}`,
        depth: d,
        kind: "fk",
        label: f.name,
        hint: `(${f.columns.join(", ")}) → ${f.refTable}(${f.refColumns.join(", ")})${f.onDelete && f.onDelete !== "NO ACTION" ? ` on delete ${f.onDelete.toLowerCase()}` : ""}`,
        expandable: false,
        icon: () => <Link2 size={13} class="db-ic-fk" />,
        ds,
        schema,
        table: t.name,
      });
  });
  const idx = t.indexes.filter((i) => !i.primary);
  group("indexes", idx.length, () => <Folder size={14} class="db-ic-folder" />, (d) => {
    for (const i of idx)
      out.push({
        key: `index:${base}/${i.name}`,
        depth: d,
        kind: "index",
        label: i.name,
        hint: `(${i.columns.join(", ")})${i.unique ? " UNIQUE" : ""}`,
        expandable: false,
        icon: () => <ListOrdered size={13} class="db-ic-index" />,
        ds,
        schema,
        table: t.name,
      });
  });
}

function buildRows(): Row[] {
  const out: Row[] = [];
  for (const ds of sources()) {
    const id = ds.id;
    const st = states[id];
    const dsKey = `ds:${id}`;
    const schemas = visibleSchemas(id);
    const tint = sourceColor(id)?.tint;
    const model = modelOf(ds.driver);
    out.push({
      key: dsKey,
      depth: 0,
      kind: "ds",
      label: ds.name,
      hint:
        [
          st?.status === "connected" && st.schemas.length > 1 ? `${schemas.length} of ${st.schemas.length}` : "",
          ds.options.readOnly ? "read-only" : "",
          ds.via ? `SSH ${ds.via}` : "",
        ]
          .filter(Boolean)
          .join(" · ") || undefined,
      expandable: true,
      icon: () => (
        <span class={`db-ic-ds ${DRIVER_COLORS[ds.driver]}`} classList={{ connected: st?.status === "connected" }}>
          <Database size={14} />
        </span>
      ),
      ds: id,
      tint,
    });
    if (!expanded().has(dsKey)) continue;
    if (!st || st.status === "connecting") {
      out.push({ key: `status:${id}`, depth: 1, kind: "status", label: "Connecting…", expandable: false, icon: () => <LoaderCircle size={13} class="db-spin" />, ds: id, cls: "muted", tint });
      continue;
    }
    if (st.status === "error") {
      out.push({ key: `status:${id}`, depth: 1, kind: "status", label: st.error ?? "Connection failed", expandable: false, icon: () => <Unplug size={13} />, ds: id, cls: "db-row-error", tint });
      continue;
    }
    for (const s of schemas) {
      const sKey = `schema:${id}/${s.name}`;
      const info = st.introspected[s.name];
      out.push({
        key: sKey,
        depth: 1,
        kind: "schema",
        label: s.name,
        expandable: true,
        icon: () => (model === "relational" ? <Layers size={14} class="db-ic-schema" /> : <Database size={14} class="db-ic-schema" />),
        ds: id,
        schema: s.name,
        cls: s.current ? "strong" : "",
        tint,
      });
      if (!expanded().has(sKey)) continue;
      if (model === "keyValue") {
        redisRows(id, s.name, 2, out, tint);
        continue;
      }
      if (!info) {
        out.push({ key: `status:${sKey}`, depth: 2, kind: "status", label: "Loading…", expandable: false, icon: () => <LoaderCircle size={13} class="db-spin" />, ds: id, cls: "muted", tint });
        continue;
      }
      const tables = info.tables.filter((t) => t.kind === "table");
      const views = info.tables.filter((t) => t.kind === "view");
      const collections = info.tables.filter((t) => t.kind === "collection");
      const folder = (name: string, count: number, fill: () => void) => {
        if (!count) return;
        const fKey = `folder:${id}/${s.name}/${name}`;
        out.push({ key: fKey, depth: 2, kind: "folder", label: name, hint: String(count), expandable: true, icon: () => <Folder size={14} class="db-ic-folder" />, ds: id, schema: s.name, tint });
        if (expanded().has(fKey)) fill();
      };
      folder("collections", collections.length, () => {
        for (const t of collections) {
          const cKey = `collection:${id}/${s.name}/${t.name}`;
          out.push({
            key: cKey,
            depth: 3,
            kind: "collection",
            label: t.name,
            hint: t.rowEstimate != null ? `${t.rowEstimate} documents` : undefined,
            expandable: true,
            icon: () => <FileJson size={14} class="db-ic-table" />,
            ds: id,
            schema: s.name,
            table: t.name,
            tint,
          });
          if (expanded().has(cKey)) tableChildren(id, s.name, t, 4, out);
        }
      });
      folder("tables", tables.length, () => {
        for (const t of tables) {
          const tKey = `table:${id}/${s.name}/${t.name}`;
          out.push({ key: tKey, depth: 3, kind: "table", label: t.name, hint: t.comment || undefined, expandable: true, icon: () => <Table2 size={14} class="db-ic-table" />, ds: id, schema: s.name, table: t.name, tint });
          if (expanded().has(tKey)) tableChildren(id, s.name, t, 4, out);
        }
      });
      folder("views", views.length, () => {
        for (const t of views) {
          const vKey = `view:${id}/${s.name}/${t.name}`;
          out.push({ key: vKey, depth: 3, kind: "view", label: t.name, expandable: true, icon: () => <Eye size={14} class="db-ic-table" />, ds: id, schema: s.name, table: t.name, tint });
          if (expanded().has(vKey)) tableChildren(id, s.name, t, 4, out);
        }
      });
      folder("routines", info.routines.length, () => {
        for (const r of info.routines)
          out.push({
            key: `routine:${id}/${s.name}/${r.name}`,
            depth: 3,
            kind: "routine",
            label: r.name,
            hint: r.kind === "function" ? `function${r.returns ? `: ${r.returns}` : ""}` : "procedure",
            expandable: false,
            icon: () => <SquareFunction size={14} class="db-ic-routine" />,
            ds: id,
            schema: s.name,
            tint,
          });
      });
    }
  }
  return out;
}

/** Expand a node; data sources connect first. */
async function expand(row: Row): Promise<void> {
  if (row.kind === "ds" && !isConnected(row.ds)) {
    setNodeOpen(row.key, true);
    if (await connect(row.ds)) {
      revealSource(row.ds);
      const ds = sourceById(row.ds);
      const schema = visibleSchemas(row.ds).find((x) => x.current)?.name;
      if (ds && modelOf(ds.driver) === "keyValue" && schema) void loadRedisKeys(row.ds, schema);
    }
    return;
  }
  setNodeOpen(row.key, true);
  const ds = sourceById(row.ds);
  if (row.kind === "schema" && ds && modelOf(ds.driver) === "keyValue") void loadRedisKeys(row.ds, row.schema!);
}

function activate(row: Row): void {
  if (row.kind === "table" || row.kind === "view") {
    const ds = sourceById(row.ds);
    if (ds && modelOf(ds.driver) === "document") openCollection(row.ds, row.schema!, row.table!);
    else openTable(row.ds, row.schema!, row.table!);
  } else if (row.kind === "collection") openCollection(row.ds, row.schema!, row.table!);
  else if (row.kind === "rkey") openRedisKey(row.ds, row.schema!, row.table!);
  else if (row.expandable) {
    if (expanded().has(row.key)) setNodeOpen(row.key, false);
    else void expand(row);
  }
}

export function newDataSourceMenu(): MenuItem[] {
  const items: MenuItem[] = DRIVERS.map((d) => ({
    label: DRIVER_LABELS[d],
    icon: <Database size={13} />,
    run: () => openDataSources({ newDriver: d }),
  }));
  if (detected().length) {
    items.push({ separator: true, label: "" }, { label: "Data Source from .env", icon: <Plus size={13} />, run: () => void addFromEnv() });
  }
  return items;
}

export function DatabaseActions() {
  const [menu, setMenu] = createSignal<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const current = () => sourceById(selectedNode()?.replace(/^[a-z]+:/, "").split("/")[0]) ?? sources()[0];
  const open = (e: MouseEvent, items: MenuItem[]) => {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    setMenu({ x: r.left, y: r.bottom + 2, items });
  };
  return (
    <>
      <button class="icon-btn" title={`New (${shortcutLabel("Console.TableResult.AddRow")})`} onClick={(e) => open(e, newDataSourceMenu())}>
        <Plus size={16} />
      </button>
      <button class="icon-btn" title="Refresh" disabled={!current()} onClick={() => current() && void refresh(current()!.id)}>
        <RefreshCw size={14} />
      </button>
      <button class="icon-btn" title={`Data Source Properties (${shortcutLabel("Database.Properties")})`} onClick={() => openDataSources({ select: current()?.id })}>
        <Settings2 size={15} />
      </button>
      <button class="icon-btn" title={`Jump to Query Console (${shortcutLabel("Database.OpenConsole")})`} disabled={!current()} onClick={() => current() && openConsole(current()!.id)}>
        <SquareTerminal size={15} />
      </button>
      <button class="icon-btn" title="Collapse All" onClick={() => [...expanded()].forEach((k) => setNodeOpen(k, false))}>
        <FoldVertical size={14} />
      </button>
      <button
        class="icon-btn"
        classList={{ active: dbPanel() === "sessions" }}
        title="Sessions"
        data-testid="db-sessions-toggle"
        onClick={() => setDbPanel(dbPanel() === "sessions" ? "tree" : "sessions")}
      >
        <Activity size={14} />
      </button>
      <Show when={menu()}>{(m) => <ContextMenu {...m()} onClose={() => setMenu(null)} />}</Show>
    </>
  );
}

function rowMenu(row: Row): MenuItem[] {
  const ds = sourceById(row.ds);
  const items: MenuItem[] = [];
  if (row.kind === "rkey") {
    items.push(
      { label: "Open Value", hint: "Enter", icon: <KeySquare size={13} />, run: () => openRedisKey(row.ds, row.schema!, row.table!) },
      { label: "Copy Key", icon: <Copy size={13} />, run: () => void navigator.clipboard?.writeText(row.table!) },
      { separator: true, label: "" },
    );
  }
  if (row.kind === "schema" && ds && modelOf(ds.driver) === "keyValue") {
    items.push({ label: "Refresh Keys", icon: <RefreshCw size={13} />, run: () => void loadRedisKeys(row.ds, row.schema!) }, { separator: true, label: "" });
  }
  if (row.kind === "collection" || (ds && modelOf(ds.driver) === "document" && row.kind === "view")) {
    items.push(
      { label: "Open Documents", hint: "Enter", icon: <FileJson size={13} />, run: () => openCollection(row.ds, row.schema!, row.table!) },
      { label: "Copy Name", icon: <Copy size={13} />, run: () => void navigator.clipboard?.writeText(row.table!) },
      { separator: true, label: "" },
    );
  } else if (row.kind === "table" || row.kind === "view") {
    items.push(
      { label: "Edit Data", hint: "Enter", icon: <Table2 size={13} />, run: () => openTable(row.ds, row.schema!, row.table!) },
      {
        label: "Go to DDL",
        icon: <FileCode size={13} />,
        run: () =>
          void dbApi
            .tableDdl(row.ds, row.schema!, row.table!)
            .then((ddl) => openConsole(row.ds, ddl))
            .catch((e) => showBalloon("No DDL", String(e))),
      },
      { label: "Copy Name", icon: <Copy size={13} />, run: () => void navigator.clipboard?.writeText(row.table!) },
      { separator: true, label: "" },
    );
  }
  items.push({ label: "New Query Console", hint: shortcutLabel("Database.OpenConsole"), icon: <SquareTerminal size={13} />, run: () => openConsole(row.ds) });
  items.push({ label: "Refresh", icon: <RefreshCw size={13} />, run: () => void refresh(row.ds) });
  if (row.kind === "ds") {
    if (isConnected(row.ds)) items.push({ label: "Disconnect", icon: <Unplug size={13} />, run: () => void disconnect(row.ds) });
    items.push(
      { label: "Properties", hint: shortcutLabel("Database.Properties"), icon: <Settings2 size={13} />, run: () => openDataSources({ select: row.ds }) },
      { separator: true, label: "" },
      {
        label: "Delete",
        danger: true,
        icon: <Trash2 size={13} />,
        run: () => {
          if (window.confirm(`Delete data source "${ds?.name}"?`)) void removeSource(row.ds);
        },
      },
    );
  }
  return items;
}

function EmptyState() {
  return (
    <div class="db-empty">
      <Database size={30} class="db-empty-icon" />
      <div>No data sources yet</div>
      <div class="muted">Create one to browse schemas, run queries and edit tables.</div>
      <div class="db-empty-actions">
        <button class="btn-primary" onClick={() => openDataSources({ newDriver: "mysql" })}>
          <Plus size={14} /> New Data Source
        </button>
        <Show when={detected().length}>
          <button class="btn-secondary" onClick={() => void addFromEnv()}>
            Add from .env
          </button>
        </Show>
      </div>
    </div>
  );
}

export function DatabaseView() {
  let root!: HTMLDivElement;
  const rows = createMemo(buildRows);
  const [menu, setMenu] = createSignal<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const byKey = () => new Map(rows().map((r) => [r.key, r]));

  const onKeyDown = (e: KeyboardEvent) => {
    if ((e.target as HTMLElement).closest(".db-key-filter")) return;
    const current = byKey().get(selectedNode() ?? "");
    if (e.key === "Enter" && current && ["table", "view", "collection", "rkey"].includes(current.kind)) {
      e.preventDefault();
      activate(current);
      return;
    }
    const visible: VisibleRow[] = rows().map((r) => ({ path: r.key, isDir: r.expandable, expanded: expanded().has(r.key), depth: r.depth }));
    const action = navigate(visible, selectedNode(), e.key);
    if (action.type === "none") return;
    e.preventDefault();
    const row = byKey().get(action.path);
    if (action.type === "select") {
      setSelectedNode(action.path);
      root.querySelector(`[data-key="${CSS.escape(action.path)}"]`)?.scrollIntoView({ block: "nearest" });
    } else if (action.type === "expand" && row) void expand(row);
    else if (action.type === "collapse") setNodeOpen(action.path, false);
    else if (row) activate(row);
  };

  return (
    <Show when={dbPanel() === "tree"} fallback={<SessionsView />}>
    <div class="db-tree" tabIndex={0} ref={root} onKeyDown={onKeyDown}>
      <Show when={sources().length} fallback={<EmptyState />}>
        <For each={rows()}>
          {(row) => (
            <Show
              when={row.kind !== "filter"}
              fallback={
                <div class="tree-row db-row db-key-filter" style={{ "padding-left": `${6 + row.depth * 16}px`, "--db-tint": row.tint }} classList={{ tinted: !!row.tint }} data-key={row.key}>
                  <span class="tree-chevron" />
                  {row.icon()}
                  <input
                    class="db-filter-input"
                    value={row.label}
                    placeholder="Key pattern, e.g. user:*"
                    spellcheck={false}
                    data-testid="db-key-pattern"
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void loadRedisKeys(row.ds, row.schema!, e.currentTarget.value.trim() || "*");
                    }}
                  />
                </div>
              }
            >
            <div
              class={`tree-row db-row ${row.cls ?? ""}`}
              classList={{ selected: selectedNode() === row.key, tinted: !!row.tint }}
              style={{ "padding-left": `${6 + row.depth * 16}px`, "--db-tint": row.tint }}
              data-key={row.key}
              data-kind={row.kind}
              onClick={() => setSelectedNode(row.key)}
              onDblClick={() => activate(row)}
              onContextMenu={(e) => {
                e.preventDefault();
                setSelectedNode(row.key);
                setMenu({ x: e.clientX, y: e.clientY, items: rowMenu(row) });
              }}
            >
              <span
                class="tree-chevron"
                onClick={(e) => {
                  if (!row.expandable) return;
                  e.stopPropagation();
                  setSelectedNode(row.key);
                  if (expanded().has(row.key)) toggleNode(row.key);
                  else void expand(row);
                }}
              >
                <Show when={row.expandable}>{expanded().has(row.key) ? <ChevronDown size={14} /> : <ChevronRight size={14} />}</Show>
              </span>
              {row.icon()}
              <span class="tree-label" classList={{ strong: row.kind === "ds" }}>
                {row.label}
              </span>
              <Show when={row.hint}>
                <span class="tree-hint">{row.hint}</span>
              </Show>
            </div>
            </Show>
          )}
        </For>
      </Show>
      <Show when={menu()}>{(m) => <ContextMenu {...m()} onClose={() => setMenu(null)} />}</Show>
    </div>
    </Show>
  );
}

export type { DataSourceInfo };
