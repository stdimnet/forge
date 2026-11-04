/**
 * Emmet (PHPStorm's Emmet support): abbreviations that expand with the expand key (Tab by
 * default) in HTML-like files, JSX and CSS, the abbreviation preview, Surround with Emmet,
 * Update Tag with Emmet, Remove Tag, Next/Previous Emmet Edit Point and the Settings |
 * Editor | Emmet pages. Parsing and expansion are Rust (`crates/forge-emmet`).
 *
 * Shortcuts come from the keymap: the commands use PHPStorm's action ids.
 */
import { render } from "solid-js/web";
import { registerCommand } from "@/state/commands";
import { registerEditorExtension } from "@/editor/extensions";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerKeymapActions } from "@/keymap/store";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { emmetExtension } from "./editor";
import { goToEditPoint, removeTag, surroundWithEmmet, updateTag } from "./actions";
import { AbbreviationPopupHost } from "./AbbreviationPopup";
import { PAGE_ID } from "./model";
import "./emmet.css";
import { t } from "@/i18n";

registerEditorExtension(({ path }) => emmetExtension(path));

registerKeymapActions([{ id: "EmmetRemoveTag", title: "Remove Tag", group: "Emmet", context: "editor" }]);

registerCommand({ id: "SurroundWithEmmet", title: "Surround with Emmet…", category: "Code", run: () => surroundWithEmmet() });
registerCommand({ id: "EmmetUpdateTag", title: "Update Tag with Emmet…", category: "Code", run: updateTag });
registerCommand({ id: "EmmetRemoveTag", title: "Remove Tag", category: "Code", run: removeTag });
registerCommand({ id: "EmmetNextEditPoint", title: "Next Emmet Edit Point", category: "Navigate", run: () => goToEditPoint(true) });
registerCommand({ id: "EmmetPreviousEditPoint", title: "Previous Emmet Edit Point", category: "Navigate", run: () => goToEditPoint(false) });

registerSettingsPage({
  id: PAGE_ID,
  title: t("settings.page.emmet"),
  group: "Editor",
  order: 36,
  keywords: ["abbreviation", "expand", "zen coding", "tab", "html", "css", "jsx"],
  component: () => <SchemaPage pageId={PAGE_ID} />,
});
registerSettingsPage({
  id: "editor.emmet.html",
  title: t("settings.page.editor.emmet.html"),
  group: "Editor",
  parent: PAGE_ID,
  order: 0,
  keywords: ["emmet", "abbreviation preview", "href", "blade", "twig"],
  component: () => <SchemaPage pageId="editor.emmet.html" />,
});
registerSettingsPage({
  id: "editor.emmet.css",
  title: t("settings.page.editor.emmet.css"),
  group: "Editor",
  parent: PAGE_ID,
  order: 1,
  keywords: ["emmet", "scss", "less", "sass", "property"],
  component: () => <SchemaPage pageId="editor.emmet.css" />,
});
registerSettingsPage({
  id: "editor.emmet.jsx",
  title: t("settings.page.editor.emmet.jsx"),
  group: "Editor",
  parent: PAGE_ID,
  order: 2,
  keywords: ["emmet", "react", "className", "tsx"],
  component: () => <SchemaPage pageId="editor.emmet.jsx" />,
});

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "emmet-overlay";
  document.body.appendChild(host);
  render(AbbreviationPopupHost, host);
}
