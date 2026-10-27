/**
 * State of the database object dialogs (Create/Modify Table, Drop, Rename, Generate DDL,
 * Import Data, dump / restore) and the scripts they run: the read-only / production check,
 * a confirmation for destructive changes, then the script on its own connection.
 */
import { createSignal } from "solid-js";
import { askConfirm } from "@/features/contextMenu/dialogs";
import { showBalloon } from "@/keymap";
import { dbObjectsApi, type DdlObject, type DesignWarning, type DumpAction, type DumpTool } from "@/ipc/dbObjects";
import { introspect, openConsole, refresh, states } from "../store";

export type ObjectDialog =
  | { kind: "table"; ds: string; schema: string; table?: string }
  | { kind: "drop"; ds: string; schema: string; objects: { kind: string; name: string; table?: string }[] }
  | { kind: "rename"; ds: string; schema: string; table: string; column?: string }
  | { kind: "ddl"; ds: string; objects: DdlObject[] }
  | { kind: "import"; ds: string; schema: string; table?: string }
  | { kind: "dump"; ds: string; tool: DumpTool; action: DumpAction; schema?: string; tables: string[] };

const [dialog, setDialog] = createSignal<ObjectDialog | null>(null);
export { dialog };
export const openObjectDialog = (d: ObjectDialog) => setDialog(d);
export const closeObjectDialog = () => setDialog(null);

/** Nodes selected with Ctrl/Shift+click in the Database tool window (keys of tree rows). */
const [multi, setMulti] = createSignal<Set<string>>(new Set());
export { multi, setMulti };

const text = (e: unknown) => (e instanceof Error ? e.message : String(e));

/**
 * Run a script of the object dialogs: refused on read-only data sources, confirmed on
 * production ones and when `warnings` hold destructive changes. Refreshes `schema` after.
 */
export async function runObjectScript(opts: { ds: string; schema?: string; sql: string; title: string; warnings?: DesignWarning[] }): Promise<boolean> {
  const { ds, sql, title } = opts;
  try {
    const g = await dbObjectsApi.guard(ds, sql);
    if (g.blocked) {
      showBalloon(`${title} not executed`, g.blocked);
      return false;
    }
    const destructive = (opts.warnings ?? []).filter((w) => w.destructive);
    if (destructive.length) {
      const ok = await askConfirm({
        title: "Destructive Changes",
        message: `${destructive.map((w) => w.message).join(".\n")}.\n\nExecute anyway?`,
        confirm: "Execute",
      });
      if (!ok) return false;
    }
    let confirmed = false;
    if (g.confirm) {
      if (!(await askConfirm({ title: "Production Data Source", message: g.confirm, confirm: "Execute" }))) return false;
      confirmed = true;
    }
    const r = await dbObjectsApi.runScript(ds, sql, confirmed);
    if (r.error) {
      showBalloon(`${title} failed`, `${r.error}${r.failed ? `\n${r.failed}` : ""}${r.executed ? `\n${r.executed} of ${r.total} statement(s) ran before the error.` : ""}`);
      await afterChange(ds, opts.schema);
      return false;
    }
    showBalloon(title, `${r.executed} statement${r.executed === 1 ? "" : "s"} executed in ${r.elapsedMs} ms`);
    await afterChange(ds, opts.schema);
    return true;
  } catch (e) {
    showBalloon(`${title} failed`, text(e));
    return false;
  }
}

/** Reload the changed schema (or the whole data source). */
export async function afterChange(ds: string, schema?: string): Promise<void> {
  if (schema && states[ds]?.introspected[schema]) await introspect(ds, schema).catch(() => undefined);
  else await refresh(ds).catch(() => undefined);
}

/** "Open in console" of the dialogs. */
export function scriptToConsole(ds: string, sql: string): void {
  openConsole(ds, sql);
}

/** Tables of the loaded schemas of a data source (targets of foreign keys and imports). */
export function loadedTables(ds: string): { schema: string; name: string; columns: string[]; primaryKey: string[] }[] {
  return Object.values(states[ds]?.introspected ?? {}).flatMap((s) =>
    s.tables.filter((t) => t.kind === "table").map((t) => ({ schema: s.name, name: t.name, columns: t.columns.filter((c) => !c.virtual).map((c) => c.name), primaryKey: t.primaryKey })),
  );
}
