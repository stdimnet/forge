/**
 * File and code templates: New ▸ <template> in the Project view's context menu, File | New
 * and the Alt+Insert "New" popup (`NewElement`), the New PHP Class / New <template> dialogs
 * and Settings | Editor | File and Code Templates. Rendering, PSR-4 mapping and the catalog
 * are computed in Rust (`forge-file-templates`).
 */
import { render } from "solid-js/web";
import { registerCommand } from "@/state/commands";
import { registerSettingsPage } from "@/features/settingsPages";
import { openSettings } from "@/features/settings/dialogState";
import {
  item,
  registerContextMenuProvider,
  resolveEntries,
  separator,
  showResolvedMenu,
  type MenuEntry,
} from "@/features/contextMenu/state";
import { action, registerMainMenuDynamic, registerMainMenuEntries } from "@/features/mainMenu/model";
import { newMenuEntries, newTargetDir } from "@/features/project/fileMenu";
import { projectTree } from "@/features/project/ProjectView";
import { isDialogWindow } from "@/shell/window/mode";
import { workspace } from "@/state/workspace";
import type { TemplateInfo } from "@/ipc/templates";
import { FileTemplatesPage } from "./FileTemplatesPage";
import { NewFileDialogHost } from "./NewFileDialog";
import { catalog, offeredTemplates, openNewFileDialog, refreshCatalog, SETTINGS_PAGE, trackCatalog } from "./model";
import "./fileTemplates.css";

const editTemplates = () => openSettings(SETTINGS_PAGE);

/** New ▸ entries for the templates the project uses, grouped, then Edit File Templates…. */
export function templateMenuEntries(dir: string, templates: TemplateInfo[] = catalog().templates): MenuEntry[] {
  const out: MenuEntry[] = [];
  for (const group of offeredTemplates(templates)) {
    out.push(...group.map((t) => item(t.name, () => openNewFileDialog(t, dir))), separator);
  }
  out.push(item("Edit File Templates…", editTemplates, "EditFileTemplates"));
  return out;
}

registerContextMenuProvider<string>("project.new", (dir) => (workspace() ? templateMenuEntries(dir) : []));

// File | New: the templates after File / Scratch File / Directory, like PHPStorm.
registerMainMenuEntries(
  "File/_New",
  [{ type: "separator" }, { type: "dynamic", source: "fileTemplates" }, { type: "separator" }, action("EditFileTemplates", "Edit File _Templates...")],
  { after: "NewDir", when: () => !!workspace() },
);
registerMainMenuDynamic("fileTemplates", () =>
  offeredTemplates(catalog().templates)
    .flat()
    .map((t) => ({
      key: `fileTemplate:${t.name}`,
      label: t.name,
      run: () => void newTargetDir().then((dir) => openNewFileDialog(t, dir)),
    })),
);

/** Alt+Insert outside the editor: the "New" popup for the selected folder. */
async function showNewPopup(): Promise<void> {
  if (!workspace()) return;
  if (!catalog().templates.length) await refreshCatalog();
  const dir = await newTargetDir();
  const selected = projectTree.selected();
  const row = selected ? document.querySelector<HTMLElement>(`.tree-row[data-path="${CSS.escape(selected)}"]`) : null;
  const rect = row?.getBoundingClientRect();
  const x = rect ? rect.left + 24 : window.innerWidth / 2 - 120;
  const y = rect ? rect.bottom : window.innerHeight / 3;
  showResolvedMenu(resolveEntries(newMenuEntries(dir)), x, y, { selectFirst: true });
}

registerCommand({ id: "NewElement", title: "New…", category: "File", run: () => void showNewPopup() });
registerCommand({ id: "EditFileTemplates", title: "Edit File Templates…", category: "File", run: editTemplates });
registerCommand({
  id: "PhpNewClass",
  title: "PHP Class",
  category: "File",
  run: () => {
    const t = catalog().templates.find((x) => x.kind === "file" && x.name === "PHP Class");
    if (t) void newTargetDir().then((dir) => openNewFileDialog(t, dir));
  },
});

registerSettingsPage({
  id: SETTINGS_PAGE,
  title: "File and Code Templates",
  group: "Editor",
  order: 35,
  keywords: ["file templates", "code templates", "includes", "velocity", "php class", "header", "new file"],
  component: FileTemplatesPage,
});

if (typeof document !== "undefined" && !isDialogWindow()) {
  trackCatalog();
  const host = document.createElement("div");
  host.id = "file-templates-overlay";
  document.body.appendChild(host);
  render(NewFileDialogHost, host);
}
