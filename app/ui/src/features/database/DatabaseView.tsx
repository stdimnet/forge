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
} from "lucide-solid";
import { dbApi, DRIVER_LABELS, type DataSourceInfo, type Driver, type TableInfo } from "@/ipc/db";
import { showBalloon, shortcutLabel } from "@/keymap";
import { navigate, type VisibleRow } from "@/features/project/treeNavigation";
import { ContextMenu, type MenuItem } from "./ContextMenu";
import {
  connect,
  disconnect,
  expanded,
  isConnected,
  openConsole,
  openDataSources,
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

type Kind = "ds" | "schema" | "folder" | "table" | "view" | "group" | "column" | "key" | "fk" | "index" | "routine" | "status";

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
}

const DRIVER_COLORS: Record<Driver, string> = { mysql: "info", mariadb: "warning", postgres: "graph-1", sqlite: "success" };

function tableChildren(ds: string, schema: string, t: TableInfo, depth: number, out: Row[]): void {
  const base = `${ds}/${schema}/${t.name}`;
  const group = (name: string, count: number, icon: () => JSX.Element, fill: (d: number) => void) => {
    if (!count) return;
    const key = `group:${base}/${name}`;
    out.push({ key, depth, kind: "group", label: name, hint: String(count), expandable: true, icon, ds, schema, table: t.name });
    if (expanded().has(key)) fill(depth + 1);
  };
  group("columns", t.columns.length, () => <Folder size={14} class="db-ic-folder" />, (d) => {
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
  group("keys", t.primaryKey.length ? 1 : 0, () => <Folder size={14} class="db-ic-folder" />, (d) =>
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
    out.push({
      key: dsKey,
      depth: 0,
      kind: "ds",
      label: ds.name,
      hint: st?.status === "connected" && st.schemas.length > 1 ? `${schemas.length} of ${st.schemas.length}` : undefined,
      expandable: true,
      icon: () => (
        <span class={`db-ic-ds ${DRIVER_COLORS[ds.driver]}`} classList={{ connected: st?.status === "connected" }}>
          <Database size={14} />
        </span>
      ),
      ds: id,
    });
    if (!expanded().has(dsKey)) continue;
    if (!st || st.status === "connecting") {
      out.push({ key: `status:${id}`, depth: 1, kind: "status", label: "Connecting…", expandable: false, icon: () => <LoaderCircle size={13} class="db-spin" />, ds: id, cls: "muted" });
      continue;
    }
    if (st.status === "error") {
      out.push({ key: `status:${id}`, depth: 1, kind: "status", label: st.error ?? "Connection failed", expandable: false, icon: () => <Unplug size={13} />, ds: id, cls: "db-row-error" });
      continue;
    }
    for (const s of schemas) {
      const sKey = `schema:${id}/${s.name}`;
      const info = st.introspected[s.name];
      out.push({ key: sKey, depth: 1, kind: "schema", label: s.name, expandable: true, icon: () => <Layers size={14} class="db-ic-schema" />, ds: id, schema: s.name, cls: s.current ? "strong" : "" });
      if (!expanded().has(sKey)) continue;
      if (!info) {
        out.push({ key: `status:${sKey}`, depth: 2, kind: "status", label: "Loading…", expandable: false, icon: () => <LoaderCircle size={13} class="db-spin" />, ds: id, cls: "muted" });
        continue;
      }
      const tables = info.tables.filter((t) => t.kind === "table");
      const views = info.tables.filter((t) => t.kind === "view");
      const folder = (name: string, count: number, fill: () => void) => {
        if (!count) return;
        const fKey = `folder:${id}/${s.name}/${name}`;
        out.push({ key: fKey, depth: 2, kind: "folder", label: name, hint: String(count), expandable: true, icon: () => <Folder size={14} class="db-ic-folder" />, ds: id, schema: s.name });
        if (expanded().has(fKey)) fill();
      };
      folder("tables", tables.length, () => {
        for (const t of tables) {
          const tKey = `table:${id}/${s.name}/${t.name}`;
          out.push({ key: tKey, depth: 3, kind: "table", label: t.name, hint: t.comment || undefined, expandable: true, icon: () => <Table2 size={14} class="db-ic-table" />, ds: id, schema: s.name, table: t.name });
          if (expanded().has(tKey)) tableChildren(id, s.name, t, 4, out);
        }
      });
      folder("views", views.length, () => {
        for (const t of views) {
          const vKey = `view:${id}/${s.name}/${t.name}`;
          out.push({ key: vKey, depth: 3, kind: "view", label: t.name, expandable: true, icon: () => <Eye size={14} class="db-ic-table" />, ds: id, schema: s.name, table: t.name });
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
    if (await connect(row.ds)) revealSource(row.ds);
    return;
  }
  setNodeOpen(row.key, true);
}

function activate(row: Row): void {
  if (row.kind === "table" || row.kind === "view") openTable(row.ds, row.schema!, row.table!);
  else if (row.expandable) {
    if (expanded().has(row.key)) setNodeOpen(row.key, false);
    else void expand(row);
  }
}

export function newDataSourceMenu(): MenuItem[] {
  const drivers: Driver[] = ["mysql", "mariadb", "postgres", "sqlite"];
  const items: MenuItem[] = drivers.map((d) => ({
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
      <Show when={menu()}>{(m) => <ContextMenu {...m()} onClose={() => setMenu(null)} />}</Show>
    </>
  );
}

function rowMenu(row: Row): MenuItem[] {
  const ds = sourceById(row.ds);
  const items: MenuItem[] = [];
  if (row.kind === "table" || row.kind === "view") {
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
    const current = byKey().get(selectedNode() ?? "");
    if (e.key === "Enter" && current && (current.kind === "table" || current.kind === "view")) {
      e.preventDefault();
      openTable(current.ds, current.schema!, current.table!);
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
    <div class="db-tree" tabIndex={0} ref={root} onKeyDown={onKeyDown}>
      <Show when={sources().length} fallback={<EmptyState />}>
        <For each={rows()}>
          {(row) => (
            <div
              class={`tree-row db-row ${row.cls ?? ""}`}
              classList={{ selected: selectedNode() === row.key }}
              style={{ "padding-left": `${6 + row.depth * 16}px` }}
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
          )}
        </For>
      </Show>
      <Show when={menu()}>{(m) => <ContextMenu {...m()} onClose={() => setMenu(null)} />}</Show>
    </div>
  );
}

export type { DataSourceInfo };
