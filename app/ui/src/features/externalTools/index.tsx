/**
 * External Tools (Settings | Tools | External Tools): programs with macro arguments, shown
 * under Tools | External Tools, in the editor and Project view context menus, in the keymap
 * as actions (`Tool_<group>_<name>`, bindable in Settings | Keymap) and as the "Run External
 * tool" Before launch task of run configurations.
 */
import { createEffect, createRoot, on } from "solid-js";
import type { EditorView } from "@codemirror/view";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerCommand, unregisterCommand } from "@/state/commands";
import { item, registerContextMenuProvider, separator, submenu, type MenuEntry } from "@/features/contextMenu/state";
import { registerMainMenuDynamic, registerMainMenuEntries } from "@/features/mainMenu/model";
import { registerBeforeLaunchHandler } from "@/features/run";
import type { FileEntry } from "@/ipc/workspace";
import type { ExternalTool } from "@/ipc/runTools";
import { workspace } from "@/state/workspace";
import { ExternalToolsPage } from "./ExternalToolsPage";
import { externalTools, toolActionId, toolGroups, DEFAULT_GROUP } from "./model";
import { runExternalTool, runToolAndWait } from "./run";
import "./externalTools.css";
import { t } from "@/i18n";

export { runExternalTool } from "./run";

registerSettingsPage({
  id: "tools.external-tools",
  title: t("settings.page.tools.external-tools"),
  group: "Tools",
  order: 30,
  keywords: ["external tools", "program", "arguments", "macro", "output filters", "$FilePath$", "$ProjectFileDir$"],
  component: ExternalToolsPage,
});

// Every enabled tool is an action: Tools menu, keymap, Find Action.
createRoot(() => {
  let registered: string[] = [];
  createEffect(
    on(
      () => toolGroups(),
      (groups) => {
        const ids: string[] = [];
        for (const g of groups) {
          for (const tool of g.tools) {
            const id = toolActionId(tool);
            ids.push(id);
            registerCommand({ id, title: tool.name, category: "External Tools", run: () => void runExternalTool(tool) });
          }
        }
        registered.filter((id) => !ids.includes(id)).forEach(unregisterCommand);
        registered = ids;
      },
    ),
  );
});

/** Context menu entries: one submenu per group (PHPStorm's "External Tools"). */
function toolMenu(path?: string): MenuEntry[] {
  const groups = toolGroups();
  if (!groups.length || !workspace()) return [];
  return [
    separator,
    ...groups.map((g) =>
      submenu(
        g.name,
        g.tools.map((t: ExternalTool) => item(t.name, () => void runExternalTool(t, path), toolActionId(t))),
      ),
    ),
  ];
}

registerContextMenuProvider<EditorView>("editor", () => toolMenu());
registerContextMenuProvider<FileEntry | null>("project", (entry) => (entry ? toolMenu(entry.path) : []));

registerMainMenuEntries("Tools", [{ type: "group", label: "External Tools", items: [{ type: "dynamic", source: "externalTools" }] }], {
  after: "IdeScriptingConsole",
  when: () => toolGroups().length > 0,
});
registerMainMenuDynamic("externalTools", () =>
  toolGroups().flatMap((g) =>
    g.tools.map((t) => ({
      key: toolActionId(t),
      label: t.name,
      action: toolActionId(t),
      detail: g.name === DEFAULT_GROUP ? undefined : g.name,
      run: () => void runExternalTool(t),
    })),
  ),
);

// "Run External tool" Before launch tasks.
registerBeforeLaunchHandler("externalTool", (task) => runToolAndWait(task.value));

/** For tests and the Before launch list: all tools in settings order. */
export { externalTools };
