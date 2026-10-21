/**
 * Database tools (DataGrip in Forge): the Database tool window, Data Sources and Drivers
 * dialog, query consoles and table data editors as editor tabs, and the `.env` suggestion.
 */
import { createEffect, createRoot, on } from "solid-js";
import { Database, SquareTerminal, Table2, Eye } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { registerEditorView } from "@/shell/editorViews";
import { commands, registerCommand } from "@/state/commands";
import type { EditorTab } from "@/state/editor";
import { showToolWindow } from "@/state/layout";
import { onProjectDetected } from "@/state/project";
import { workspace } from "@/state/workspace";
import { registerDialog } from "@/shell/window/dialogs";
import { isDialogWindow } from "@/shell/window/mode";
import { registerKeymapActions, runAction } from "@/keymap";
import { dbApi } from "@/ipc/db";
import { DatabaseActions, DatabaseView } from "./DatabaseView";
import { DataSourceDialog } from "./DataSourceDialog";
import { ConsoleView } from "./ConsoleView";
import { TableEditor } from "./TableEditor";
import { activeConsole, activeDataEditor, focusedConsole } from "./controllers";
import { checkEnv } from "./envDetect";
import {
  CONSOLE_VIEW,
  DATA_SOURCES_DIALOG,
  TABLE_VIEW,
  dataSourcesSaved,
  loadSources,
  openConsole,
  openDataSources,
  parseConsolePath,
  parseTablePath,
  selectedSourceId,
  sourceById,
  tableInfo,
  type DataSourcesSaved,
  type DialogRequest,
} from "./store";
import "./database.css";

registerToolWindow({
  id: "database",
  title: "Database",
  icon: Database,
  anchor: "right",
  order: 10,
  component: DatabaseView,
  actions: DatabaseActions,
});

registerEditorView({
  id: CONSOLE_VIEW,
  component: ConsoleView,
  icon: (p) => <SquareTerminal size={p.size} class="db-tab-icon console" />,
  breadcrumbs: (tab: EditorTab) => {
    const loc = parseConsolePath(tab.path);
    return [sourceById(loc?.id)?.name ?? loc?.id ?? "", tab.name];
  },
});

registerEditorView({
  id: TABLE_VIEW,
  component: TableEditor,
  icon: (p) => {
    const loc = parseTablePath(p.tab.path);
    const view = loc && tableInfo(loc.id, loc.schema, loc.table)?.kind === "view";
    return view ? <Eye size={p.size} class="db-tab-icon table" /> : <Table2 size={p.size} class="db-tab-icon table" />;
  },
  breadcrumbs: (tab: EditorTab) => {
    const loc = parseTablePath(tab.path);
    return loc ? [sourceById(loc.id)?.name ?? loc.id, loc.schema, loc.table] : [tab.name];
  },
});

registerSettingsPage({
  id: "tools.database",
  title: "Database",
  group: "Tools",
  order: 20,
  keywords: ["sql", "data source", "page size", "null", ".env", "query console"],
  component: () => <SchemaPage pageId="tools.database" />,
});

// ---- actions --------------------------------------------------------------------------

registerKeymapActions([
  { id: "Console.Jdbc.Execute", title: "Execute", group: "Database", context: "editor", shortcuts: { $default: ["Ctrl+Enter"], macOS: ["Cmd+Enter"] } },
  { id: "Console.TableResult.SubmitChanges", title: "Submit", group: "Database", context: "toolwindow", shortcuts: { $default: ["Ctrl+Enter"], macOS: ["Cmd+Enter"] } },
  { id: "Console.TableResult.AddRow", title: "Add Row", group: "Database", context: "toolwindow", shortcuts: { $default: ["Alt+Insert"], macOS: ["Cmd+N"] } },
  { id: "Console.TableResult.DeleteRows", title: "Delete Rows", group: "Database", context: "toolwindow", shortcuts: { $default: ["Ctrl+Y"], macOS: ["Cmd+Backspace"] } },
  { id: "Console.TableResult.Revert", title: "Revert Selected", group: "Database", context: "toolwindow", shortcuts: { $default: ["Ctrl+Alt+Z"], macOS: ["Cmd+Alt+Z"] } },
  { id: "Console.TableResult.Reload", title: "Reload Page", group: "Database" },
  { id: "Database.OpenConsole", title: "Jump to Query Console…", group: "Database", context: "toolwindow", shortcuts: { $default: ["Ctrl+Shift+F10"], macOS: ["Cmd+Shift+F10"] } },
  { id: "Database.Properties", title: "Data Source Properties", group: "Database", context: "toolwindow", shortcuts: { $default: ["Shift+Enter"], macOS: ["Shift+Enter"] } },
  { id: "Database.NewDataSource", title: "New Data Source…", group: "Database" },
]);

const inDbTree = () => !!document.activeElement?.closest(".db-tree");

registerCommand({
  id: "Console.Jdbc.Execute",
  title: "Execute",
  category: "Database",
  run: async () => {
    await activeConsole()?.execute();
  },
});

// Ctrl+Enter is also PHPStorm's "Split Line"; inside a console it executes instead.
const splitLine = commands().get("EditorSplitLine");
if (splitLine) {
  registerCommand({
    ...splitLine,
    run: () => {
      const c = focusedConsole();
      return c ? c.execute() : splitLine.run();
    },
  });
}

registerCommand({ id: "Console.TableResult.SubmitChanges", title: "Submit", category: "Database", run: () => activeDataEditor()?.submit() });
registerCommand({
  id: "Console.TableResult.AddRow",
  title: "Add Row",
  category: "Database",
  run: () => {
    if (inDbTree()) return openDataSources({ newDriver: "mysql" });
    const data = activeDataEditor();
    if (data && !document.activeElement?.closest(".project-tree")) return data.addRow();
    // Alt+Insert elsewhere (the Project view, other tool windows) is PHPStorm's New… popup.
    return runAction("NewElement");
  },
});
registerCommand({ id: "Console.TableResult.DeleteRows", title: "Delete Rows", category: "Database", run: () => activeDataEditor()?.deleteRows() });
registerCommand({ id: "Console.TableResult.Revert", title: "Revert Selected", category: "Database", run: () => activeDataEditor()?.revert() });
registerCommand({ id: "Console.TableResult.Reload", title: "Reload Page", category: "Database", run: () => activeDataEditor()?.reload() });
registerCommand({
  id: "Database.OpenConsole",
  title: "Jump to Query Console…",
  category: "Database",
  run: () => {
    const id = selectedSourceId();
    if (id) openConsole(id);
    else openDataSources({ newDriver: "mysql" });
  },
});
registerCommand({ id: "Database.Properties", title: "Data Source Properties", category: "Database", run: () => openDataSources({ select: selectedSourceId() }) });
registerCommand({ id: "Database.NewDataSource", title: "New Data Source…", category: "Database", run: () => openDataSources({ newDriver: "mysql" }) });
// Manifest command ids (Find Action, plugin contributions).
registerCommand({ id: "database.newDataSource", title: "New Data Source…", category: "Database", run: () => openDataSources({ newDriver: "mysql" }) });
registerCommand({
  id: "database.openConsole",
  title: "Open Query Console",
  category: "Database",
  run: () => {
    showToolWindow("right", "database");
    const id = selectedSourceId();
    if (id) openConsole(id);
  },
});

registerDialog<DialogRequest>({
  id: DATA_SOURCES_DIALOG,
  title: "Data Sources and Drivers",
  width: 900,
  height: 620,
  minWidth: 680,
  minHeight: 420,
  component: DataSourceDialog,
  prepare: () => loadSources(),
  onMessage: (msg) => void dataSourcesSaved(msg as DataSourcesSaved),
});

// ---- wiring ---------------------------------------------------------------------------

// Environment file detection offers data sources in the IDE window, not in dialog windows.
if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() => {
    createEffect(
      on(
        () => workspace()?.root,
        (root) => {
          if (root) void loadSources().then(() => checkEnv(true));
        },
      ),
    );
  });
  // The backend loads a project's data sources in the background, then emits the list.
  void dbApi.onDataSources(() => void checkEnv(true));
  onProjectDetected(() => void checkEnv(true));
}
