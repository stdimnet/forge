/**
 * Database objects in the Database tool window (DataGrip): New › Table, Modify Table
 * (Ctrl+F6), Rename, Drop (with cascade), SQL Scripts › Generate DDL to the clipboard, a
 * console or a file (and the SQL Generator), Import Data from File, and dump / restore with
 * the native tools. Menus are built here; the dialogs render in `ObjectDialogs`.
 */
import { Match, Switch } from "solid-js";
import { Portal } from "solid-js/web";
import { ArchiveRestore, Copy, DatabaseBackup, FileCode, FileDown, FileUp, PencilLine, Plus, ScrollText, SquareTerminal, TableProperties, Trash2 } from "lucide-solid";
import { modelOf, type DataSourceInfo } from "@/ipc/db";
import type { DdlObject, DumpAction, DumpTool } from "@/ipc/dbObjects";
import { registerKeymapActions, runAction, shortcutLabel } from "@/keymap";
import { registerCommand } from "@/state/commands";
import type { MenuItem } from "../ContextMenu";
import { defaultSchema, selectedNode, sourceById } from "../store";
import { DumpDialog } from "./DumpDialog";
import { ImportDialog } from "./ImportDialog";
import { DropDialog, GenerateDdlDialog, RenameDialog, generateDdlTo } from "./ScriptDialogs";
import { TableDialog } from "./TableDialog";
import { dialog, openObjectDialog } from "./state";
import "./objects.css";
import { t as tr } from "@/i18n";

export { multi, setMulti } from "./state";

/** What the menus need of a Database tree row. */
export interface ObjectRow {
  key: string;
  kind: string;
  ds: string;
  schema?: string;
  table?: string;
  /** Column / key / index / trigger name. */
  name?: string;
  label: string;
  isVirtual?: boolean;
}

export function ObjectDialogs() {
  // Outside the tree: its key handling must not see the dialogs' keys.
  return (
    <Portal>
    <Switch>
      <Match when={dialog()?.kind === "table" && dialog()} keyed>
        {(d) => d.kind === "table" && <TableDialog ds={d.ds} schema={d.schema} table={d.table} />}
      </Match>
      <Match when={dialog()?.kind === "drop" && dialog()} keyed>
        {(d) => d.kind === "drop" && <DropDialog ds={d.ds} schema={d.schema} objects={d.objects} />}
      </Match>
      <Match when={dialog()?.kind === "rename" && dialog()} keyed>
        {(d) => d.kind === "rename" && <RenameDialog ds={d.ds} schema={d.schema} table={d.table} column={d.column} />}
      </Match>
      <Match when={dialog()?.kind === "ddl" && dialog()} keyed>
        {(d) => d.kind === "ddl" && <GenerateDdlDialog ds={d.ds} objects={d.objects} />}
      </Match>
      <Match when={dialog()?.kind === "import" && dialog()} keyed>
        {(d) => d.kind === "import" && <ImportDialog ds={d.ds} schema={d.schema} table={d.table} />}
      </Match>
      <Match when={dialog()?.kind === "dump" && dialog()} keyed>
        {(d) => d.kind === "dump" && <DumpDialog ds={d.ds} tool={d.tool} action={d.action} schema={d.schema} tables={d.tables} />}
      </Match>
    </Switch>
    </Portal>
  );
}

const TABLE_KINDS = ["table", "view", "matview"];
const DDL_KINDS = ["schema", "table", "view", "matview", "routine", "sequence", "type", "index", "trigger"];

/** The DDL object of a row, if it has one. */
function ddlObject(r: ObjectRow): DdlObject | null {
  if (!r.schema || !DDL_KINDS.includes(r.kind)) return null;
  if (r.kind === "schema") return { schema: r.schema, kind: "schema", name: "" };
  if (r.kind === "index" || r.kind === "trigger") return r.table ? { schema: r.schema, kind: r.kind, table: r.table, name: r.name ?? r.label } : null;
  return { schema: r.schema, kind: r.kind, name: r.table ?? r.label };
}

const dumpTools = (ds: DataSourceInfo): { tool: DumpTool; action: DumpAction; label: string }[] => {
  switch (ds.driver) {
    case "mysql":
    case "mariadb":
      return [
        { tool: "mysqldump", action: "dump", label: tr("database.objects.main.exportWithMysqldump") },
        { tool: "mysql", action: "restore", label: tr("database.objects.main.restoreWithMysql") },
      ];
    case "postgres":
      return [
        { tool: "pgDump", action: "dump", label: tr("database.objects.main.exportWithPgDump") },
        { tool: "pgRestore", action: "restore", label: tr("database.objects.main.restoreWithPgRestore") },
        { tool: "psql", action: "restore", label: tr("database.objects.main.restoreWithPsql") },
      ];
    case "sqlite":
      return [
        { tool: "sqlite3", action: "dump", label: tr("database.objects.main.exportWithSqlite3") },
        { tool: "sqlite3", action: "restore", label: tr("database.objects.main.restoreWithSqlite3") },
      ];
    case "sqlserver":
      return [
        { tool: "sqlcmd", action: "dump", label: tr("database.objects.main.backUpWithSqlcmd") },
        { tool: "sqlcmd", action: "restore", label: tr("database.objects.main.restoreWithSqlcmd") },
      ];
    default:
      return [];
  }
};

/**
 * Object entries of a row's context menu; `selection` are the rows selected with it
 * (Ctrl+click) for Generate DDL and Drop of several objects.
 */
export function objectMenu(row: ObjectRow, selection: ObjectRow[]): MenuItem[] {
  const ds = sourceById(row.ds);
  if (!ds || modelOf(ds.driver) !== "relational") return [];
  const live = ds.driver !== "ddl";
  const schema = row.schema ?? defaultSchema(row.ds);
  const items: MenuItem[] = [];
  const rows = selection.length > 1 && selection.some((r) => r.key === row.key) ? selection : [row];
  if (live && ["ds", "schema", "folder", "table"].includes(row.kind) && schema) {
    items.push({ label: tr("database.objects.main.new"), icon: <Plus size={13} />, items: [{ label: tr("database.objects.main.table"), icon: <TableProperties size={13} />, run: () => openObjectDialog({ kind: "table", ds: row.ds, schema }) }] });
  }
  if (live && row.kind === "table" && row.table) {
    items.push({ label: tr("database.objects.main.modifyTable"), hint: shortcutLabel("Database.ModifyTable"), icon: <TableProperties size={13} />, run: () => openObjectDialog({ kind: "table", ds: row.ds, schema, table: row.table }) });
  }
  if (live && (row.kind === "table" || row.kind === "view") && row.table) {
    items.push({ label: tr("database.objects.main.rename"), icon: <PencilLine size={13} />, run: () => openObjectDialog({ kind: "rename", ds: row.ds, schema, table: row.table! }) });
  }
  if (live && row.kind === "column" && !row.isVirtual && row.table && row.name) {
    items.push({ label: tr("database.objects.main.renameColumn"), icon: <PencilLine size={13} />, run: () => openObjectDialog({ kind: "rename", ds: row.ds, schema, table: row.table!, column: row.name }) });
  }
  const droppable = rows.filter((r) => [...TABLE_KINDS, "routine", "sequence", "type", "index", "trigger"].includes(r.kind) && r.schema === schema);
  if (live && droppable.length && droppable.length === rows.length) {
    items.push({
      label: droppable.length > 1 ? tr("database.objects.main.dropCountObjects", { count: droppable.length }) : tr("tools.database.dropEllipsis"),
      danger: true,
      icon: <Trash2 size={13} />,
      run: () =>
        openObjectDialog({
          kind: "drop",
          ds: row.ds,
          schema,
          objects: droppable.map((r) => ({ kind: r.kind, name: r.kind === "index" || r.kind === "trigger" ? (r.name ?? r.label) : (r.table ?? r.label), table: r.table })),
        }),
    });
  }
  const objects = rows.map(ddlObject).filter((o): o is DdlObject => !!o);
  if (objects.length) {
    items.push({
      label: tr("database.objects.main.sqlScripts"),
      icon: <ScrollText size={13} />,
      items: [
        { label: tr("database.objects.main.generateDdlToClipboard"), icon: <Copy size={13} />, run: () => void generateDdlTo("clipboard", row.ds, objects) },
        { label: tr("database.objects.main.generateDdlToQueryConsole"), icon: <SquareTerminal size={13} />, run: () => void generateDdlTo("console", row.ds, objects) },
        { label: tr("database.objects.main.generateDdlToFile"), icon: <FileDown size={13} />, run: () => void generateDdlTo("file", row.ds, objects) },
        { separator: true, label: "" },
        { label: tr("database.objects.main.sqlGenerator"), hint: shortcutLabel("Database.GenerateDdl"), icon: <FileCode size={13} />, run: () => openObjectDialog({ kind: "ddl", ds: row.ds, objects }) },
      ],
    });
  }
  if (live && ["ds", "schema", "folder", "table"].includes(row.kind)) {
    const tools = dumpTools(ds);
    const tables = rows.filter((r) => r.kind === "table" && r.table).map((r) => (ds.driver === "postgres" ? `${schema}.${r.table}` : r.table!));
    items.push({
      label: tr("database.objects.main.importExport"),
      icon: <DatabaseBackup size={13} />,
      items: [
        { label: tr("database.objects.main.importDataFromFile"), icon: <FileUp size={13} />, run: () => openObjectDialog({ kind: "import", ds: row.ds, schema, table: row.kind === "table" ? row.table : undefined }) },
        ...(tools.length ? [{ separator: true, label: "" }] : []),
        ...tools.map((t) => ({
          label: t.label,
          icon: t.action === "dump" ? <DatabaseBackup size={13} /> : <ArchiveRestore size={13} />,
          run: () => openObjectDialog({ kind: "dump", ds: row.ds, tool: t.tool, action: t.action, schema, tables: t.action === "dump" ? tables : [] }),
        })),
      ],
    });
  }
  if (items.length) items.push({ separator: true, label: "" });
  return items;
}

// ---- actions ------------------------------------------------------------------------------

registerKeymapActions([
  { id: "Database.NewTable", title: "New Table…", group: "Database" },
  // DataGrip: Modify Table in the Database tool window (Change Signature in the editor).
  { id: "Database.ModifyTable", title: "Modify Table…", group: "Database", context: "toolwindow", shortcuts: { $default: ["Ctrl+F6"], macOS: ["Cmd+F6"] } },
  { id: "Database.GenerateDdl", title: "SQL Generator…", group: "Database" },
  { id: "Database.ImportData", title: "Import Data from File…", group: "Database" },
  { id: "Database.DumpData", title: "Export with Dump Tool…", group: "Database" },
]);

const inDbTree = () => !!document.activeElement?.closest(".db-tree");

/** The tree selection as `ds`, `schema`, `table` (from the row key `kind:ds/schema/table…`). */
function selection(): { ds?: string; schema?: string; table?: string; kind?: string } {
  const key = selectedNode();
  if (!key) return {};
  const m = /^([a-z]+):([^/]+)(?:\/([^/]+))?(?:\/([^/]+))?/.exec(key);
  if (!m) return {};
  return { kind: m[1], ds: m[2], schema: m[3], table: m[4] };
}

function current(): { ds: string; schema: string; table?: string } | null {
  const s = selection();
  const ds = s.ds && sourceById(s.ds);
  if (!ds || modelOf(ds.driver) !== "relational" || ds.driver === "ddl") return null;
  const schema = s.schema ?? defaultSchema(ds.id);
  return schema ? { ds: ds.id, schema, table: s.kind === "table" ? s.table : undefined } : null;
}

registerCommand({
  id: "Database.NewTable",
  title: "New Table…",
  category: "Database",
  run: () => {
    const c = current();
    if (c) openObjectDialog({ kind: "table", ds: c.ds, schema: c.schema });
  },
});
registerCommand({
  id: "Database.ModifyTable",
  title: "Modify Table…",
  category: "Database",
  run: () => {
    const c = current();
    if (inDbTree() && c?.table) openObjectDialog({ kind: "table", ds: c.ds, schema: c.schema, table: c.table });
    else if (!inDbTree()) void runAction("ChangeSignature");
  },
});
registerCommand({
  id: "Database.GenerateDdl",
  title: "SQL Generator…",
  category: "Database",
  run: () => {
    const c = current();
    if (c) openObjectDialog({ kind: "ddl", ds: c.ds, objects: [c.table ? { schema: c.schema, kind: "table", name: c.table } : { schema: c.schema, kind: "schema", name: "" }] });
  },
});
registerCommand({
  id: "Database.ImportData",
  title: "Import Data from File…",
  category: "Database",
  run: () => {
    const c = current();
    if (c) openObjectDialog({ kind: "import", ds: c.ds, schema: c.schema, table: c.table });
  },
});
registerCommand({
  id: "Database.DumpData",
  title: "Export with Dump Tool…",
  category: "Database",
  run: () => {
    const c = current();
    const ds = c && sourceById(c.ds);
    const tool = ds && dumpTools(ds).find((t) => t.action === "dump");
    if (c && tool) openObjectDialog({ kind: "dump", ds: c.ds, tool: tool.tool, action: "dump", schema: c.schema, tables: [] });
  },
});
