/**
 * The SQL editor (DataGrip's in Forge): dialects per file and folder with the status bar
 * widget and Settings | Languages & Frameworks | SQL Dialects, resolve against the data
 * source model with the SQL inspections and their quick fixes, Ctrl+B / Find Usages /
 * alias rename, Reformat Code with the SQL formatter, user parameters, query history and
 * Run SQL Script. The logic is Rust (`forge-sqlcode`); this feature renders it.
 */
import { createEffect, createRoot, on } from "solid-js";
import { render } from "solid-js/web";
import { registerLanguage, languageIdFor } from "@/editor/languages";
import { registerEditorExtension } from "@/editor/extensions";
import { registerStatusItem } from "@/features/statusItems";
import { registerSettingsPage } from "@/features/settingsPages";
import { item, registerContextMenuProvider, type MenuEntry } from "@/features/contextMenu/state";
import { registerKeymapActions } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { getSetting } from "@/state/settings";
import { isLibraryPath } from "@/ipc/codeintel";
import type { FileEntry } from "@/ipc/workspace";
import { isDialogWindow } from "@/shell/window/mode";
import { activeConsole } from "@/features/database/controllers";
import { consoleSourceId, states } from "@/features/database/store";
import { refreshAllFileSql, fileSql, sqlFileLanguage } from "./language";
import { reanalyze, sqlInspection } from "./inspector";
import { DialectStatus, showDialectStatus, SQL_DIALECTS_PAGE } from "./DialectStatus";
import { SqlDialectsPage } from "./SqlDialectsPage";
import { openRunFile, SqlDialogsHost } from "./SqlDialogs";
import "./navigation";
import "./sqlEditor.css";

registerLanguage({ id: "sql", load: (path) => sqlFileLanguage(path) });

// `.sql` files in the editor (consoles add the same through `sqlConsoleExtensions`).
registerEditorExtension(({ path }) => (languageIdFor(path) === "sql" && !consoleSourceId(path) && !isLibraryPath(path) ? sqlInspection(path) : null));

registerStatusItem({ id: "sql.dialect", align: "right", order: 8, component: DialectStatus, when: showDialectStatus });

registerSettingsPage({
  id: SQL_DIALECTS_PAGE,
  title: "SQL Dialects",
  group: "Languages & Frameworks",
  order: 60,
  keywords: ["sql", "dialect", "mysql", "postgresql", "sqlite", "sql server", "oracle", "clickhouse", "resolution", "data source"],
  component: SqlDialectsPage,
});

// ---- Run SQL Script and Query History ---------------------------------------------------------

const isSqlFile = (path: string) => /\.(sql|ddl)$/i.test(path);

/** Run SQL Script for a `.sql` file (the data source its mapping names comes first). */
export function runSqlScript(path: string): void {
  openRunFile(path, fileSql[path]?.dataSourceId);
}

registerKeymapActions([
  { id: "Sql.RunScript", title: "Run SQL Script…", group: "Database" },
  { id: "Console.History.Browse", title: "Browse Query History", group: "Database", context: "editor", shortcuts: { $default: ["Ctrl+Alt+E"], macOS: ["Cmd+Alt+E"] } },
]);

const runActive = () => {
  const tab = activeTab();
  if (tab && isSqlFile(tab.path) && !consoleSourceId(tab.path)) runSqlScript(tab.path);
};
const browseHistory = () => activeConsole()?.history?.();

registerCommand({ id: "Sql.RunScript", title: "Run SQL Script…", category: "Database", run: runActive });
registerCommand({ id: "sql.runFile", title: "Run SQL Script…", category: "Database", run: runActive });
registerCommand({ id: "Console.History.Browse", title: "Browse Query History", category: "Database", run: browseHistory });
registerCommand({ id: "sql.queryHistory", title: "Browse Query History", category: "Database", run: browseHistory });

registerContextMenuProvider<FileEntry | null>("project", (entry): MenuEntry[] =>
  entry && entry.kind !== "dir" && isSqlFile(entry.name) ? [item(`Run '${entry.name}'…`, () => runSqlScript(entry.path), "Sql.RunScript")] : [],
);

registerContextMenuProvider<unknown>("editor", (): MenuEntry[] => {
  const tab = activeTab();
  if (!tab || !isSqlFile(tab.path) || consoleSourceId(tab.path)) return [];
  return [item(`Run '${tab.name}'…`, () => runSqlScript(tab.path), "Sql.RunScript")];
});

// ---- wiring ---------------------------------------------------------------------------

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(() => {
    // SQL Dialects settings changed: dialects of open files, then their problems.
    createEffect(
      on(
        () => JSON.stringify([getSetting("sql.dialect.default"), getSetting("sql.dialect.project"), getSetting("sql.dialect.mappings")]),
        () => {
          refreshAllFileSql();
          reanalyze();
        },
        { defer: true },
      ),
    );
    // A data source connected or a schema was introspected: resolve again.
    createEffect(
      on(
        () =>
          Object.entries(states)
            .map(([id, s]) => `${id}:${s?.status}:${Object.keys(s?.introspected ?? {}).join(",")}`)
            .join("|"),
        () => reanalyze(),
        { defer: true },
      ),
    );
  });
  const host = document.createElement("div");
  host.id = "sql-editor-overlays";
  document.body.append(host);
  render(SqlDialogsHost, host);
}
