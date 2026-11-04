/**
 * Settings feature: the Settings dialog (command `settings.open`, Ctrl+Alt+S), the built-in
 * settings pages generated from the plugin settings schema, the Plugins page, and applying
 * settings (fonts, theme, editor options) to the running UI.
 */
import { EditorView, highlightWhitespace } from "@codemirror/view";
import { registerCommand } from "@/state/commands";
import { registerSettingsPage, type SettingsPage } from "@/features/settingsPages";
import { registerEditorExtension } from "@/editor/extensions";
import { loadSettings } from "@/state/settings";
import { registerDialog, isDialogOpen } from "@/shell/window/dialogs";
import {
  closeSettings,
  openSettings,
  openSettingsForNewProjects,
  setSelected,
  SETTINGS_DIALOG,
  type SettingsDialogProps,
  type SettingsDialogResult,
} from "./dialogState";
import { SettingsDialog } from "./SettingsDialog";
import { registerObjectSettingEditor, SchemaPage } from "./SchemaPage";
import { TemplateDelimitersEditor } from "./TemplateDelimitersEditor";
import { DELIMITERS_KEY } from "@/editor/templateDelimiters";
import { PluginsPage } from "./PluginsPage";
import { applySettingsToDocument } from "./apply";
import { softWrapFor } from "@/editor/languages";
import "./settings.css";
import { t } from "@/i18n";

export { openSettings, openSettingsForNewProjects, closeSettings };

type SchemaPageDef = Omit<SettingsPage, "component">;

/** Pages rendered from the settings schema (setting `group` = page id). */
const schemaPages: SchemaPageDef[] = [
  { id: "appearance", title: t("settings.page.appearance"), group: "Appearance & Behavior", order: 0, keywords: ["theme", "dark", "light", "font"] },
  { id: "system", title: t("settings.page.system"), group: "Appearance & Behavior", order: 10, keywords: ["autosave", "startup", "exit"] },
  { id: "system.language", title: t("settings.page.system.language"), group: "Appearance & Behavior", parent: "system", order: 5, keywords: ["language", "locale", "translation", "english", "deutsch", "german", "sprache"] },
  { id: "editor.general", title: t("settings.page.editor.general"), group: "Editor", order: 0 },
  { id: "editor.tabs", title: t("settings.page.editor.tabs"), group: "Editor", parent: "editor.general", order: 0, keywords: ["tab limit", "closing policy", "pinned", "split"] },
  { id: "editor.general.auto-import", title: t("settings.page.editor.general.auto-import"), group: "Editor", parent: "editor.general", order: 10, keywords: ["optimize imports", "use statement", "import popup", "fqn", "on the fly"] },
  { id: "editor.font", title: t("settings.page.editor.font"), group: "Editor", order: 10 },
  { id: "editor.inlay-hints", title: t("settings.page.editor.inlay-hints"), group: "Editor", order: 30, keywords: ["code vision"] },
  { id: "editor.todo", title: t("settings.page.editor.todo"), group: "Editor", order: 40, keywords: ["fixme", "patterns", "comments"] },
  { id: "vcs.git", title: t("settings.page.vcs.git"), group: "Version Control", order: 0, keywords: ["commit message"] },
  { id: "vcs.commit", title: t("settings.page.vcs.commit"), group: "Version Control", order: 1, keywords: ["commit checks", "reformat", "analyze code", "todo", "run tests", "hooks"] },
  { id: "vcs.changelists", title: t("settings.page.vcs.changelists"), group: "Version Control", order: 2, keywords: ["changelist", "create changelists automatically"] },
  { id: "vcs.local-history", title: t("settings.page.vcs.local-history"), group: "Version Control", order: 3, keywords: ["working days", "labels", "revisions", "recent changes"] },
  { id: "lang.php", title: t("settings.page.lang.php"), group: "Languages & Frameworks", order: 0, keywords: ["interpreter", "language level"] },
  { id: "lang.templates", title: t("settings.page.lang.templates"), group: "Languages & Frameworks", order: 50, keywords: ["twig", "smarty", "latte", "jinja", "go template", "latex", "delimiter"] },
  { id: "tools.terminal", title: t("settings.page.tools.terminal"), group: "Tools", order: 0, keywords: ["shell"] },
  { id: "tools.search", title: t("settings.page.tools.search"), group: "Tools", order: 10, keywords: ["find in files", "matches", "limit"] },
];

for (const page of schemaPages) {
  registerSettingsPage({ ...page, component: () => <SchemaPage pageId={page.id} /> });
}

registerObjectSettingEditor(DELIMITERS_KEY, TemplateDelimitersEditor);

registerSettingsPage({
  id: "plugins",
  title: t("settings.page.plugins"),
  group: "Plugins",
  order: 0,
  keywords: ["marketplace", "installed", "extensions", "packs"],
  component: PluginsPage,
});

registerCommand({ id: "settings.open", title: t("settings.page.settings.open"), category: "File", run: () => openSettings() });
registerCommand({
  id: "TemplateProjectProperties",
  title: "Settings for New Projects…",
  category: "File",
  run: () => openSettingsForNewProjects(),
});

registerDialog<SettingsDialogProps, SettingsDialogResult>({
  id: SETTINGS_DIALOG,
  title: (props) => (props?.forNewProjects ? t("settings.titleNewProjects") : t("settings.title")),
  width: 1040,
  height: 700,
  minWidth: 640,
  minHeight: 420,
  component: SettingsDialog,
  prepare: () => loadSettings(),
  // Changes reach every window through `settings://changed`; the plugin list and the
  // setting layers are reloaded here.
  onClosed: (result) => {
    if (result?.page) setSelected(result.page);
    void loadSettings().catch(() => undefined);
  },
  onMessage: () => void loadSettings().catch(() => undefined),
});

// Ctrl+Alt+S (Cmd+, on macOS) toggles the dialog; capture phase so the editor cannot eat it.
window.addEventListener(
  "keydown",
  (e) => {
    const ctrlAltS = e.ctrlKey && e.altKey && !e.shiftKey && e.code === "KeyS";
    const cmdComma = e.metaKey && !e.ctrlKey && e.key === ",";
    if (!ctrlAltS && !cmdComma) return;
    e.preventDefault();
    e.stopPropagation();
    if (isDialogOpen(SETTINGS_DIALOG)) closeSettings();
    else openSettings();
  },
  true,
);

// Whitespace markers are always rendered and shown via CSS, so the setting applies live.
registerEditorExtension(() => highlightWhitespace());
// Soft wraps apply to files opened after the change.
registerEditorExtension(({ path }) => (softWrapFor(path) ? EditorView.lineWrapping : null));

applySettingsToDocument();
void loadSettings().catch((e) => console.error("settings could not be loaded", e));

