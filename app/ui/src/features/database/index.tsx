/**
 * Database tools (DataGrip in Forge): the Database tool window, Data Sources and Drivers
 * dialog, query consoles and table data editors as editor tabs, and the `.env` suggestion.
 */
import { createEffect, createRoot, on } from "solid-js";
import { Database, SquareTerminal, Table2, Eye, KeySquare, TextSearch, GitCompare } from "lucide-solid";
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
import { DatabaseActions, DatabaseView, fullTextSearchOfSelection, quickDocOfSelection } from "./DatabaseView";
import { FullTextSearchView, SEARCH_VIEW, openFullTextSearch } from "./FullTextSearch";
import { DataSourceDialog } from "./DataSourceDialog";
import { ConsoleView } from "./ConsoleView";
import { TableEditor } from "./TableEditor";
import { RedisValueView } from "./RedisValueView";
import { registerTabColorProvider } from "@/features/scopes/model";
import { activeConsole, activeDataEditor, activeGrid, focusedConsole, focusedGrid } from "./controllers";
import { COMPARE_VIEW, CompareView } from "./CompareView";
import { checkEnv } from "./envDetect";
import {
  CONSOLE_VIEW,
  DATA_SOURCES_DIALOG,
  REDIS_VIEW,
  TABLE_VIEW,
  dataSourcesSaved,
  loadSources,
  openConsole,
  openDataSources,
  parseConsolePath,
  parseRedisPath,
  parseTablePath,
  selectedSourceId,
  sourceById,
  sourceColor,
  tabSourceId,
  tableInfo,
  type DataSourcesSaved,
  type DialogRequest,
} from "./store";
import "./database.css";
import "./schema";
import { t } from "@/i18n";

registerToolWindow({
  id: "database",
  title: t("toolwindow.database.title"),
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
    const kind = loc && tableInfo(loc.id, loc.schema, loc.table)?.kind;
    const view = kind === "view" || kind === "matview";
    return view ? <Eye size={p.size} class="db-tab-icon table" /> : <Table2 size={p.size} class="db-tab-icon table" />;
  },
  breadcrumbs: (tab: EditorTab) => {
    const loc = parseTablePath(tab.path);
    return loc ? [sourceById(loc.id)?.name ?? loc.id, loc.schema, loc.table] : [tab.name];
  },
});

registerEditorView({
  id: SEARCH_VIEW,
  component: FullTextSearchView,
  icon: (p) => <TextSearch size={p.size} class="db-tab-icon console" />,
  breadcrumbs: (tab: EditorTab) => ["Database", tab.name],
});

registerEditorView({
  id: COMPARE_VIEW,
  component: CompareView,
  icon: (p) => <GitCompare size={p.size} class="db-tab-icon table" />,
  breadcrumbs: (tab: EditorTab) => ["Compare Data", tab.name],
});

registerEditorView({
  id: REDIS_VIEW,
  component: RedisValueView,
  icon: (p) => <KeySquare size={p.size} class="db-tab-icon table" />,
  breadcrumbs: (tab: EditorTab) => {
    const loc = parseRedisPath(tab.path);
    return loc ? [sourceById(loc.id)?.name ?? loc.id, loc.schema, loc.key] : [tab.name];
  },
});

// Consoles, tables and keys of coloured data sources (production, staging, …) get tinted tabs.
registerTabColorProvider((path) => {
  const id = tabSourceId(path);
  return id ? sourceColor(id)?.tint : undefined;
});

registerSettingsPage({
  id: "tools.database",
  title: t("settings.page.tools.database"),
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
  // Data editor (DataGrip): Shift+Enter in a grid opens the value editor (see Database.Properties).
  { id: "Console.TableResult.EditValue", title: "Open Value Editor", group: "Database" },
  { id: "Console.TableResult.Transpose", title: "Transpose", group: "Database" },
  { id: "Console.TableResult.AggregateView", title: "Aggregate View", group: "Database" },
  { id: "Console.TableResult.ShowChart", title: "Show Chart", group: "Database" },
  { id: "Console.TableResult.CompareWith", title: "Compare With…", group: "Database" },
  // Ctrl+B on a cell (PHPStorm's Go to Declaration is an editor action): the referenced row.
  { id: "Console.TableResult.GotoReferencedData", title: "Go to Referenced Data", group: "Database", context: "toolwindow", shortcuts: { $default: ["Ctrl+B"], macOS: ["Cmd+B"] } },
  { id: "Database.OpenConsole", title: "Jump to Query Console…", group: "Database", context: "toolwindow", shortcuts: { $default: ["Ctrl+Shift+F10"], macOS: ["Cmd+Shift+F10"] } },
  { id: "Database.Properties", title: "Data Source Properties", group: "Database", context: "toolwindow", shortcuts: { $default: ["Shift+Enter"], macOS: ["Shift+Enter"] } },
  { id: "Database.NewDataSource", title: "New Data Source…", group: "Database" },
  // DataGrip: Full-Text Search in the Database tool window.
  { id: "DatabaseView.FullTextSearch", title: "Full-Text Search…", group: "Database", shortcuts: { $default: ["Ctrl+Alt+Shift+F"], macOS: ["Cmd+Alt+Shift+F"] } },
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
registerCommand({
  id: "DatabaseView.FullTextSearch",
  title: "Full-Text Search…",
  category: "Database",
  run: () => (inDbTree() ? fullTextSearchOfSelection() : openFullTextSearch({ ds: selectedSourceId() })),
});

// Ctrl+Q in the Database tool window documents the selected node (DDL, column info).
const quickDoc = commands().get("QuickJavaDoc");
if (quickDoc) {
  registerCommand({
    ...quickDoc,
    run: () => (inDbTree() ? quickDocOfSelection() : quickDoc.run()),
  });
}

// Shift+Enter: the value editor in a data grid, Data Source Properties in the tree.
registerCommand({
  id: "Database.Properties",
  title: "Data Source Properties",
  category: "Database",
  run: () => {
    const grid = focusedGrid();
    if (grid?.editValue) return grid.editValue();
    openDataSources({ select: selectedSourceId() });
  },
});
registerCommand({ id: "Console.TableResult.EditValue", title: "Open Value Editor", category: "Database", run: () => activeGrid()?.editValue?.() });
registerCommand({ id: "Console.TableResult.Transpose", title: "Transpose", category: "Database", run: () => activeGrid()?.transpose?.() });
registerCommand({ id: "Console.TableResult.AggregateView", title: "Aggregate View", category: "Database", run: () => activeGrid()?.aggregateView?.() });
registerCommand({ id: "Console.TableResult.ShowChart", title: "Show Chart", category: "Database", run: () => activeGrid()?.chart?.() });
registerCommand({ id: "Console.TableResult.CompareWith", title: "Compare With…", category: "Database", run: () => activeGrid()?.compare?.() });

// Ctrl+B / Alt+F7 on a data grid cell: foreign-key navigation (real and virtual keys).
registerCommand({
  id: "Console.TableResult.GotoReferencedData",
  title: "Go to Referenced Data",
  category: "Database",
  run: () => (focusedGrid() ?? activeGrid())?.gotoReferenced?.(),
});
const findUsages = commands().get("FindUsages");
registerCommand({
  id: "FindUsages",
  title: findUsages?.title ?? "Find Usages",
  category: findUsages?.category ?? "Find",
  run: async () => {
    const go = focusedGrid()?.findReferencing;
    if (go) return go();
    await findUsages?.run();
  },
});
registerCommand({ id: "Database.NewDataSource", title: "New Data Source…", category: "Database", run: () => openDataSources({ newDriver: "mysql" }) });
// Manifest command ids (Find Action, plugin contributions).
registerCommand({ id: "database.newDataSource", title: "New Data Source…", category: "Database", run: () => openDataSources({ newDriver: "mysql" }) });
registerCommand({
  id: "database.openConsole",
  title: "Open Query Console",
  category: "Database",
  run: () => {
    showToolWindow("database");
    const id = selectedSourceId();
    if (id) openConsole(id);
  },
});

registerDialog<DialogRequest>({
  id: DATA_SOURCES_DIALOG,
  title: t("database.main.dataSourcesAndDrivers"),
  width: 920,
  height: 760,
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
