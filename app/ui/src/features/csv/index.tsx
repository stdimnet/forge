/**
 * CSV/TSV: a text editor with "Edit as Table" (Text / Table tabs at the bottom), parsing and
 * writing in Rust (`forge-viewers`). Settings | Editor | CSV/TSV.
 */
import { registerFileEditor } from "@/shell/editorViews";
import { registerCommand } from "@/state/commands";
import { activeTab } from "@/state/editor";
import { registerKeymapActions } from "@/keymap/store";
import { registerSettingsPage } from "@/features/settingsPages";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { CsvEditor, csvEditors, setCsvMode, type CsvMode } from "./CsvEditor";
import "./csv.css";
import { t } from "@/i18n";

export const isCsvPath = (path: string) => /\.(csv|tsv|tab)$/i.test(path) && !path.includes("://");

registerFileEditor({ id: "csv", accepts: isCsvPath, component: CsvEditor });

const ACTIONS: { id: string; title: string; mode: CsvMode }[] = [
  { id: "Csv.EditAsTable", title: "Edit as Table", mode: "table" },
  { id: "Csv.EditAsText", title: "Edit as Text", mode: "text" },
];

registerKeymapActions(ACTIONS.map((a) => ({ id: a.id, title: a.title, group: "CSV" })));
for (const a of ACTIONS) {
  registerCommand({
    id: a.id,
    title: a.title,
    category: "CSV",
    run: () => {
      const path = activeTab()?.path;
      if (path && csvEditors.has(path)) setCsvMode(path, a.mode);
    },
  });
}

registerSettingsPage({
  id: "editor.csv",
  title: t("settings.page.editor.csv"),
  group: "Editor",
  order: 61,
  keywords: ["table", "edit as table", "header", "tsv", "delimiter"],
  component: () => <SchemaPage pageId="editor.csv" />,
});
