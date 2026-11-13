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
  submenu,
  type MenuEntry,
} from "@/features/contextMenu/state";
import { action, registerMainMenuDynamic, registerMainMenuEntries } from "@/features/mainMenu/model";
import { newMenuEntries, newTargetDir } from "@/features/project/fileMenu";
import { projectTree } from "@/features/project/ProjectView";
import { isDialogWindow } from "@/shell/window/mode";
import { workspace } from "@/state/workspace";
import type { TemplateInfo } from "@/ipc/templates";
import { codeApi, type CreateClassData } from "@/ipc/codeintel";
import { registerContextActionHandler } from "@/features/codeintel/intentions";
import { viewFor } from "@/features/lsp/navigation";
import { FileTemplatesPage } from "./FileTemplatesPage";
import { NewFileDialogHost } from "./NewFileDialog";
import { catalog, newMenuGroups, newMenuLabel, offeredTemplates, openNewFileDialog, refreshCatalog, setDialog, SETTINGS_PAGE, trackCatalog } from "./model";
import "./fileTemplates.css";
import { t } from "@/i18n";

const editTemplates = () => openSettings(SETTINGS_PAGE);

/**
 * New ▸ entries for the templates the project uses: PHP ▸ (Class, Interface, Trait, Enum, …),
 * Web ▸ and Lua ▸ submenus, user templates, then Edit File Templates….
 */
export function templateMenuEntries(dir: string, templates: TemplateInfo[] = catalog().templates): MenuEntry[] {
  const out: MenuEntry[] = [];
  for (const group of newMenuGroups(templates)) {
    const entries = group.sections.flatMap((section, i) => [
      ...(i > 0 ? [separator] : []),
      ...section.map((t) => item(newMenuLabel(t, group.label), () => openNewFileDialog(t, dir))),
    ]);
    if (group.label) out.push(submenu(group.label, entries));
    else out.push(separator, ...entries);
  }
  out.push(separator, item("Edit File Templates…", editTemplates, "EditFileTemplates"));
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

/**
 * Alt+Enter "Create class 'Foo'" on an unresolved class: the Create New PHP Class dialog with
 * the usage's name, namespace and template (Class / Interface / Trait); the directory follows
 * the namespace (PSR-4). When the class goes to another namespace, the usage gets an import.
 */
registerContextActionHandler("PhpCreateClass", async (raw) => {
  const data = raw as CreateClassData;
  if (!catalog().templates.length) await refreshCatalog();
  const files = catalog().templates.filter((t) => t.kind === "file");
  const template = files.find((t) => t.name === data.template) ?? files.find((t) => t.name === "PHP Class");
  if (!template) return;
  const dir = data.path.includes("/") ? data.path.slice(0, data.path.lastIndexOf("/")) : "";
  const wanted = data.namespace ? `${data.namespace}\\${data.name}` : data.name;
  setDialog({
    template,
    dir,
    create: {
      name: data.name,
      namespace: data.namespace,
      extends: data.extends ?? undefined,
      onCreated: async (_file, fqn) => {
        const view = viewFor(data.path);
        if (!data.import || !view || fqn.toLowerCase() === wanted.toLowerCase()) return;
        const edits = await codeApi.importClass(data.path, view.state.doc.toString(), fqn).catch(() => []);
        if (edits.length) view.dispatch({ changes: edits.map((e) => ({ from: e.start, to: e.end, insert: e.newText })), userEvent: "input" });
      },
    },
  });
});

registerSettingsPage({
  id: SETTINGS_PAGE,
  title: t("settings.page.file-and-code-templates"),
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
