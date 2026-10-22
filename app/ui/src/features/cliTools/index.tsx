/**
 * PHPForge Command Line Tool Support (`forge.command-line-tools`), like PHPStorm's:
 * Tools | Run Command… (Ctrl+Shift+X, `CommandLineToolsAction`) opens the Command Line
 * Tools Console popup, the "Command Line Tools Console" tool window keeps the output of
 * each command, and Settings | Tools | Command Line Tool Support configures the tools
 * (Composer, Laravel Artisan, Symfony Console, WP-CLI, Drush and custom ones). Parsing,
 * completion and the command lines are computed in Rust (`phpforge-cli-tools`).
 */
import { createComponent } from "solid-js";
import { render } from "solid-js/web";
import { SquareTerminal } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerSettingsPage } from "@/features/settingsPages";
import { registerObjectSettingEditor, SchemaPage } from "@/features/settings/SchemaPage";
import { registerKeymapActions } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { showToolWindow } from "@/state/layout";
import { closeRunCommand, openRunCommand, RunCommandPopup, runCommandOpen, SETTINGS_PAGE } from "./CommandPopup";
import { ConsoleActions, ConsoleTabs, ConsoleToolWindow } from "./ConsoleToolWindow";
import { ToolsEditor } from "./ToolsEditor";
import { cliToolsActive, TOOL_WINDOW_ID, TOOLS_SETTING } from "./store";
import "./cliTools.css";

export { openRunCommand } from "./CommandPopup";

registerToolWindow({
  id: TOOL_WINDOW_ID,
  title: "Command Line Tools Console",
  icon: SquareTerminal,
  anchor: "bottom",
  order: 45,
  component: ConsoleToolWindow,
  headerContent: ConsoleTabs,
  actions: ConsoleActions,
  when: cliToolsActive,
});

registerSettingsPage({
  id: SETTINGS_PAGE,
  title: "Command Line Tool Support",
  group: "Tools",
  order: 20,
  keywords: ["composer", "artisan", "symfony", "console", "wp-cli", "drush", "alias", "run command"],
  component: () => <SchemaPage pageId={SETTINGS_PAGE} />,
});
registerObjectSettingEditor(TOOLS_SETTING, ToolsEditor);

registerKeymapActions([
  {
    id: "CommandLineToolsAction",
    title: "Run Command…",
    group: "Tools",
    shortcuts: { $default: ["Ctrl+Shift+X"], macOS: ["Cmd+Shift+X"] },
  },
  { id: "ActivateCommandLineToolsConsoleToolWindow", title: "Command Line Tools Console", group: "Tool Windows" },
]);

registerCommand({
  id: "CommandLineToolsAction",
  title: "Run Command…",
  category: "Tools",
  run: () => {
    if (!cliToolsActive()) return;
    if (runCommandOpen()) closeRunCommand();
    else openRunCommand();
  },
});
registerCommand({
  id: "ActivateCommandLineToolsConsoleToolWindow",
  title: "Command Line Tools Console",
  category: "View",
  run: () => {
    if (cliToolsActive()) showToolWindow(TOOL_WINDOW_ID);
  },
});

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "cli-tools-overlay";
  document.body.appendChild(host);
  render(() => createComponent(RunCommandPopup, {}), host);
}
