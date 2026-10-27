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

export { openSettings, openSettingsForNewProjects, closeSettings };

type SchemaPageDef = Omit<SettingsPage, "component">;

/** Pages rendered from the settings schema (setting `group` = page id). */
const schemaPages: SchemaPageDef[] = [
  { id: "appearance", title: "Appearance", group: "Appearance & Behavior", order: 0, keywords: ["theme", "dark", "light", "font"] },
  { id: "system", title: "System Settings", group: "Appearance & Behavior", order: 10, keywords: ["autosave", "startup", "exit"] },
  { id: "editor.general", title: "General", group: "Editor", order: 0 },
  { id: "editor.tabs", title: "Editor Tabs", group: "Editor", parent: "editor.general", order: 0, keywords: ["tab limit", "closing policy", "pinned", "split"] },
  { id: "editor.general.auto-import", title: "Auto Import", group: "Editor", parent: "editor.general", order: 10, keywords: ["optimize imports", "use statement", "import popup", "fqn", "on the fly"] },
  { id: "editor.font", title: "Font", group: "Editor", order: 10 },
  { id: "editor.inlay-hints", title: "Inlay Hints", group: "Editor", order: 30, keywords: ["code vision"] },
  { id: "editor.todo", title: "TODO", group: "Editor", order: 40, keywords: ["fixme", "patterns", "comments"] },
  { id: "vcs.git", title: "Git", group: "Version Control", order: 0, keywords: ["commit message"] },
  { id: "vcs.commit", title: "Commit", group: "Version Control", order: 1, keywords: ["commit checks", "reformat", "analyze code", "todo", "run tests", "hooks"] },
  { id: "vcs.changelists", title: "Changelists", group: "Version Control", order: 2, keywords: ["changelist", "create changelists automatically"] },
  { id: "vcs.local-history", title: "Local History", group: "Version Control", order: 3, keywords: ["working days", "labels", "revisions", "recent changes"] },
  { id: "lang.php", title: "PHP", group: "Languages & Frameworks", order: 0, keywords: ["interpreter", "language level"] },
  { id: "lang.templates", title: "Template Data Languages", group: "Languages & Frameworks", order: 50, keywords: ["twig", "smarty", "latte", "jinja", "go template", "latex", "delimiter"] },
  { id: "tools.terminal", title: "Terminal", group: "Tools", order: 0, keywords: ["shell"] },
  { id: "tools.search", title: "Search", group: "Tools", order: 10, keywords: ["find in files", "matches", "limit"] },
];

for (const page of schemaPages) {
  registerSettingsPage({ ...page, component: () => <SchemaPage pageId={page.id} /> });
}

registerObjectSettingEditor(DELIMITERS_KEY, TemplateDelimitersEditor);

registerSettingsPage({
  id: "plugins",
  title: "Plugins",
  group: "Plugins",
  order: 0,
  keywords: ["marketplace", "installed", "extensions", "packs"],
  component: PluginsPage,
});

registerCommand({ id: "settings.open", title: "Settings…", category: "File", run: () => openSettings() });
registerCommand({
  id: "TemplateProjectProperties",
  title: "Settings for New Projects…",
  category: "File",
  run: () => openSettingsForNewProjects(),
});

registerDialog<SettingsDialogProps, SettingsDialogResult>({
  id: SETTINGS_DIALOG,
  title: (props) => (props?.forNewProjects ? "Settings for New Projects" : "Settings"),
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

