/**
 * Settings feature: the Settings dialog (command `settings.open`, Ctrl+Alt+S), the built-in
 * settings pages generated from the plugin settings schema, the Plugins page, and applying
 * settings (fonts, theme, editor options) to the running UI.
 */
import { render } from "solid-js/web";
import { EditorView, highlightWhitespace } from "@codemirror/view";
import { registerCommand } from "@/state/commands";
import { registerSettingsPage, type SettingsPage } from "@/features/settingsPages";
import { registerEditorExtension } from "@/editor/extensions";
import { getSetting, loadSettings } from "@/state/settings";
import { closeSettings, isOpen, openSettings } from "./dialogState";
import { SettingsDialog } from "./SettingsDialog";
import { SchemaPage } from "./SchemaPage";
import { PluginsPage } from "./PluginsPage";
import { applySettingsToDocument } from "./apply";
import { matchesGlobList } from "./globs";
import "./settings.css";

export { openSettings, closeSettings };

type SchemaPageDef = Omit<SettingsPage, "component">;

/** Pages rendered from the settings schema (setting `group` = page id). */
const schemaPages: SchemaPageDef[] = [
  { id: "appearance", title: "Appearance", group: "Appearance & Behavior", order: 0, keywords: ["theme", "dark", "light", "font"] },
  { id: "system", title: "System Settings", group: "Appearance & Behavior", order: 10, keywords: ["autosave", "startup", "exit"] },
  { id: "editor.general", title: "General", group: "Editor", order: 0 },
  { id: "editor.font", title: "Font", group: "Editor", order: 10 },
  { id: "editor.code-style", title: "Code Style", group: "Editor", order: 20, keywords: ["formatting"] },
  { id: "editor.code-style.php", title: "PHP", group: "Editor", parent: "editor.code-style", order: 0, keywords: ["indent", "braces", "psr"] },
  { id: "editor.inlay-hints", title: "Inlay Hints", group: "Editor", order: 30, keywords: ["code vision"] },
  { id: "vcs.git", title: "Git", group: "Version Control", order: 0, keywords: ["commit message"] },
  { id: "lang.php", title: "PHP", group: "Languages & Frameworks", order: 0, keywords: ["interpreter", "language level"] },
  { id: "lang.php.composer", title: "Composer", group: "Languages & Frameworks", parent: "lang.php", order: 0 },
  { id: "lang.templates", title: "Template Data Languages", group: "Languages & Frameworks", order: 50, keywords: ["twig", "smarty", "latte", "jinja", "latex", "delimiter"] },
  { id: "tools.terminal", title: "Terminal", group: "Tools", order: 0, keywords: ["shell"] },
];

for (const page of schemaPages) {
  registerSettingsPage({ ...page, component: () => <SchemaPage pageId={page.id} /> });
}

registerSettingsPage({
  id: "plugins",
  title: "Plugins",
  group: "Plugins",
  order: 0,
  keywords: ["marketplace", "installed", "extensions", "packs"],
  component: PluginsPage,
});

registerCommand({ id: "settings.open", title: "Settings…", category: "File", run: () => openSettings() });

// Ctrl+Alt+S (Cmd+, on macOS) toggles the dialog; capture phase so the editor cannot eat it.
window.addEventListener(
  "keydown",
  (e) => {
    const ctrlAltS = e.ctrlKey && e.altKey && !e.shiftKey && e.code === "KeyS";
    const cmdComma = e.metaKey && !e.ctrlKey && e.key === ",";
    if (!ctrlAltS && !cmdComma) return;
    e.preventDefault();
    e.stopPropagation();
    if (isOpen()) closeSettings();
    else openSettings();
  },
  true,
);

// Whitespace markers are always rendered and shown via CSS, so the setting applies live.
registerEditorExtension(() => highlightWhitespace());
// Soft wraps apply to files opened after the change.
registerEditorExtension(({ path }) =>
  getSetting<boolean>("editor.softWraps") && matchesGlobList(path, getSetting<string>("editor.softWrapFiles"))
    ? EditorView.lineWrapping
    : null,
);

applySettingsToDocument();
void loadSettings().catch((e) => console.error("settings could not be loaded", e));

const host = document.createElement("div");
host.id = "settings-dialog-root";
document.body.appendChild(host);
render(() => <SettingsDialog />, host);
