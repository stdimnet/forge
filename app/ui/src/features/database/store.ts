/**
 * Database tool window state: data sources, their connection state and introspected
 * schemas, tree expansion, and the console / table editor tabs.
 */
import { createSignal } from "solid-js";
import { closeDialog, openDialogWindow } from "@/shell/window/dialogs";
import { createStore, produce } from "solid-js/store";
import { dbApi, type DataSourceInfo, type SchemaInfo, type SchemaName, type TableInfo } from "@/ipc/db";
import { openViewTab } from "@/state/editor";
import { balloonsFor } from "@/keymap";

const showBalloon = balloonsFor("Database");

export const CONSOLE_VIEW = "db-console";
export const TABLE_VIEW = "db-table";

const [sources, setSources] = createSignal<DataSourceInfo[]>([]);
export { sources };

export type ConnectionStatus = "idle" | "connecting" | "connected" | "error";

export interface SourceState {
  status: ConnectionStatus;
  error?: string;
  schemas: SchemaName[];
  introspected: Record<string, SchemaInfo>;
  loading: Record<string, boolean>;
}

const [states, setStates] = createStore<Record<string, SourceState>>({});
export { states };

const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e));

export const sourceById = (id: string | undefined) => sources().find((s) => s.id === id);

let subscribed = false;

/** Load the data sources and follow backend changes. */
export async function loadSources(): Promise<void> {
  if (!subscribed) {
    subscribed = true;
    void dbApi.onDataSources((list) => setSources(list));
  }
  try {
    setSources(await dbApi.list());
  } catch (e) {
    console.warn("data sources not loaded", e);
  }
}

/** Schemas shown in the tree: the configured ones, otherwise the connection's current one. */
export function visibleSchemas(id: string): SchemaName[] {
  const st = states[id];
  const ds = sourceById(id);
  if (!st || !ds) return [];
  const wanted = ds.options.schemas;
  if (wanted.includes("*")) return st.schemas;
  if (wanted.length) return st.schemas.filter((s) => wanted.includes(s.name));
  const current = st.schemas.filter((s) => s.current);
  return current.length ? current : st.schemas.slice(0, 1);
}

/** Schema new consoles and unqualified names use. */
export function defaultSchema(id: string): string {
  return states[id]?.schemas.find((s) => s.current)?.name ?? visibleSchemas(id)[0]?.name ?? "";
}

export function tableInfo(id: string, schema: string, table: string): TableInfo | undefined {
  return states[id]?.introspected[schema]?.tables.find((t) => t.name === table);
}

export const isConnected = (id: string) => states[id]?.status === "connected";

export async function introspect(id: string, schema: string): Promise<void> {
  setStates(id, "loading", schema, true);
  try {
    const info = await dbApi.introspect(id, schema);
    setStates(id, "introspected", schema, info);
  } catch (e) {
    showBalloon(`Cannot load schema ${schema}`, errorText(e));
  } finally {
    setStates(id, "loading", schema, false);
  }
}

/** Connect (once) and load the visible schemas. Returns whether it worked. */
export async function connect(id: string, force = false): Promise<boolean> {
  const st = states[id];
  if (!force && st && (st.status === "connected" || st.status === "connecting")) return st.status === "connected";
  setStates(id, { status: "connecting", error: undefined, schemas: st?.schemas ?? [], introspected: st?.introspected ?? {}, loading: {} });
  try {
    const schemas = await dbApi.schemas(id);
    setStates(id, { status: "connected", schemas });
    await Promise.all(visibleSchemas(id).map((s) => introspect(id, s.name)));
    return true;
  } catch (e) {
    setStates(id, { status: "error", error: errorText(e) });
    showBalloon(`Connection to ${sourceById(id)?.name ?? id} failed`, errorText(e), 8000);
    return false;
  }
}

/** "Refresh" (Ctrl+F5): reconnect and reload the metadata. */
export const refresh = (id: string) => connect(id, true);

export async function disconnect(id: string): Promise<void> {
  await dbApi.disconnect(id).catch(() => undefined);
  setStates(
    produce((s) => {
      delete s[id];
    }),
  );
}

export async function removeSource(id: string): Promise<void> {
  await dbApi.remove(id);
  await disconnect(id);
}

// ---- tree ------------------------------------------------------------------------------

const [expanded, setExpanded] = createSignal<Set<string>>(new Set());
const [selectedNode, setSelectedNode] = createSignal<string | null>(null);
export { expanded, selectedNode, setSelectedNode };

export function setNodeOpen(key: string, open: boolean): void {
  setExpanded((prev) => {
    const next = new Set(prev);
    if (open) next.add(key);
    else next.delete(key);
    return next;
  });
}

export const toggleNode = (key: string) => setNodeOpen(key, !expanded().has(key));

/** Expand a data source and its default schema/tables folder (after connecting). */
export function revealSource(id: string): void {
  setNodeOpen(`ds:${id}`, true);
  const schema = defaultSchema(id);
  if (schema) {
    setNodeOpen(`schema:${id}/${schema}`, true);
    setNodeOpen(`folder:${id}/${schema}/tables`, true);
  }
}

// ---- tabs ------------------------------------------------------------------------------

const consoleCount = new Map<string, number>();

export const consolePath = (id: string, n: number) => `db-console://${id}/${n}`;
export const tablePath = (id: string, schema: string, table: string) => `db-table://${id}/${schema}/${table}`;

export function parseConsolePath(path: string): { id: string; n: number } | null {
  const m = /^db-console:\/\/(.+)\/(\d+)$/.exec(path);
  return m ? { id: m[1], n: Number(m[2]) } : null;
}

export function parseTablePath(path: string): { id: string; schema: string; table: string } | null {
  const m = /^db-table:\/\/([^/]+)\/([^/]*)\/(.+)$/.exec(path);
  return m ? { id: m[1], schema: m[2], table: m[3] } : null;
}

/** Open a new query console for a data source (like DataGrip's "console", "console_1"). */
export function openConsole(id: string, sql = ""): string {
  const n = (consoleCount.get(id) ?? 0) + 1;
  consoleCount.set(id, n);
  const path = consolePath(id, n);
  openViewTab({ path, name: n === 1 ? "console" : `console_${n - 1}`, view: CONSOLE_VIEW, content: sql });
  return path;
}

/** Consoles over files (SQL scratches): the data source attached to each tab path. */
const attached = new Map<string, string>();

/**
 * Open a file (e.g. a SQL scratch at `scratch://orders.sql`) as a query console of a data
 * source. The tab keeps its path, so it is saved like a file.
 */
export function openFileConsole(file: { path: string; name: string; content: string }, dataSourceId: string): void {
  attached.set(file.path, dataSourceId);
  openViewTab({ ...file, view: CONSOLE_VIEW });
}

/** Data source of a console tab (`db-console://id/n` or a file console). */
export const consoleSourceId = (path: string): string | undefined => parseConsolePath(path)?.id ?? attached.get(path);

/** Open the data editor of a table or view. */
export function openTable(id: string, schema: string, table: string): void {
  openViewTab({ path: tablePath(id, schema, table), name: table, view: TABLE_VIEW });
}

// ---- dialog ----------------------------------------------------------------------------

export interface DialogRequest {
  /** Data source to select, or a driver for a new one. */
  select?: string;
  newDriver?: DataSourceInfo["driver"];
}

export const DATA_SOURCES_DIALOG = "data-sources";

/** Messages of the dialog to the window that opened it: data sources it saved. */
export interface DataSourcesSaved {
  saved: string[];
}

/** Open the Data Sources and Drivers dialog (its own window in the desktop app). */
export function openDataSources(req: DialogRequest = {}): void {
  void openDialogWindow(DATA_SOURCES_DIALOG, req);
}

export function closeDataSources(): void {
  closeDialog(DATA_SOURCES_DIALOG);
}

/** The dialog saved data sources (possibly in another window): reload and reconnect them. */
export async function dataSourcesSaved(msg: DataSourcesSaved): Promise<void> {
  await loadSources();
  for (const id of msg.saved) if (states[id]) void refresh(id);
}

/** Data source of the selected tree node, else the first one. */
export function selectedSourceId(): string | undefined {
  const key = selectedNode();
  const id = key ? key.slice(key.indexOf(":") + 1).split("/")[0] : undefined;
  return sourceById(id)?.id ?? sources()[0]?.id;
}
