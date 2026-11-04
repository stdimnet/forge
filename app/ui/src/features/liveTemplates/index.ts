/**
 * Live templates (PHPStorm's Live Templates): abbreviations that expand with the expand key
 * (Tab by default) and in completion, Insert Live Template (Ctrl+J), Surround With
 * (Ctrl+Alt+T), Surround with Live Template (Ctrl+Alt+J), Unwrap/Remove (Ctrl+Shift+Delete),
 * Tools | Save as Live Template and the Settings | Editor | Live Templates page. Template sets,
 * contexts, expansion and the statement analysis are Rust (`crates/forge-livetemplates`).
 *
 * Shortcuts come from the keymap: the commands use PHPStorm's action ids.
 */
import { render } from "solid-js/web";
import { registerCommand } from "@/state/commands";
import { registerEditorExtension } from "@/editor/extensions";
import { registerSettingsPage } from "@/features/settingsPages";
import { insertLiveTemplate, saveAsLiveTemplate, surroundWith, unwrapRemove } from "./actions";
import { liveTemplatesExtension } from "./editor";
import { LiveTemplatesPage } from "./LiveTemplatesPage";
import { PAGE_ID } from "./model";
import { LiveTemplatePopupHost } from "./Popup";
import "./liveTemplates.css";
import { t } from "@/i18n";

registerEditorExtension(({ path }) => liveTemplatesExtension(path));

registerCommand({ id: "InsertLiveTemplate", title: "Insert Live Template…", category: "Code", run: insertLiveTemplate });
registerCommand({ id: "SurroundWith", title: "Surround With…", category: "Code", run: () => surroundWith(false) });
registerCommand({ id: "SurroundWithLiveTemplate", title: "Surround with Live Template…", category: "Code", run: () => surroundWith(true) });
registerCommand({ id: "Unwrap", title: "Unwrap/Remove…", category: "Code", run: unwrapRemove });
registerCommand({ id: "SaveAsTemplate", title: "Save as Live Template…", category: "Tools", run: saveAsLiveTemplate });

registerSettingsPage({
  id: PAGE_ID,
  title: t("settings.page.live-templates"),
  group: "Editor",
  order: 35,
  keywords: ["abbreviation", "snippet", "template", "surround", "expand", "tab"],
  component: LiveTemplatesPage,
});

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "live-templates-overlay";
  document.body.appendChild(host);
  render(LiveTemplatePopupHost, host);
}
