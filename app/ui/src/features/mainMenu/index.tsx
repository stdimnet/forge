/**
 * Main menu (☰ / menu bar) like PHPStorm's New UI. The structure lives in Rust
 * (`forge-keymap::menu`); this feature opens it, fills the dynamic lists, reports toggle
 * states and implements the actions only the menu needs (About, Keymap Reference, View |
 * Appearance toggles). Plugins add entries with `[[contributions.mainMenu]]` in their
 * manifest or `registerMainMenuEntries` (see `./model`).
 */
import { render } from "solid-js/web";
import { redo, undo } from "@codemirror/commands";
import { isTauri } from "@/ipc";
import { workspaceApi } from "@/ipc/workspace";
import { runAction } from "@/keymap";
import { registerCommand } from "@/state/commands";
import { toggleToolWindow } from "@/state/layout";
import { getSetting, saveSetting } from "@/state/settings";
import { openProject, workspace } from "@/state/workspace";
import { toolWindows } from "@/features/registry";
import { activeView } from "@/features/navigation/editorCommands";
import { TOOL_WINDOW_ACTIONS } from "@/features/navigation";
import { openRecentRemote } from "@/features/remote/store";
import { openSettings } from "@/features/settings/dialogState";
import { AboutHost, openAbout } from "./AboutDialog";
import { mainMenuMode, showMainMenu } from "./controller";
import { registerMainMenuDynamic, registerMainMenuToggle } from "./model";
import "./mainMenu.css";

export { registerMainMenuDynamic, registerMainMenuEntries, registerMainMenuLabel, registerMainMenuToggle } from "./model";

registerCommand({ id: "MainMenuButton.ShowMenu", title: "Main Menu", run: showMainMenu });
registerCommand({ id: "About", title: "About", run: () => void openAbout() });
registerCommand({ id: "Help.KeymapReference", title: "Keymap Reference", run: () => openSettings("keymap") });

// ---- Edit: Undo / Redo for the menu (the keys themselves stay with CodeMirror)
for (const [id, cmd] of [
  ["$Undo", undo],
  ["$Redo", redo],
] as const) {
  registerCommand({
    id,
    title: id.slice(1),
    run: () => {
      const view = activeView();
      if (!view) return;
      cmd(view);
      view.focus();
    },
  });
}

if (isTauri()) {
  registerCommand({
    id: "Exit",
    title: "Exit",
    run: async () => (await import("@tauri-apps/api/window")).getCurrentWindow().close(),
  });
}

// ---- View | Appearance toggles
const flip = (key: string) => void saveSetting(key, !getSetting<boolean>(key, false));

registerCommand({
  id: "ToggleMainMenuInSeparateToolbar",
  title: "Show Main Menu in Separate Toolbar",
  run: () => void saveSetting("appearance.mainMenu", mainMenuMode() === "toolbar" ? "hamburger" : "toolbar"),
});
registerMainMenuToggle("ToggleMainMenuInSeparateToolbar", () => mainMenuMode() === "toolbar");

registerCommand({ id: "ToggleCompactMode", title: "Compact Mode", run: () => flip("appearance.compactMode") });
registerMainMenuToggle("ToggleCompactMode", () => getSetting<boolean>("appearance.compactMode", false));

registerCommand({ id: "ViewToolButtons", title: "Tool Window Bars", run: () => flip("appearance.showToolWindowBars") });
registerMainMenuToggle("ViewToolButtons", () => getSetting<boolean>("appearance.showToolWindowBars", true));

// ---- Dynamic lists
const actionOfToolWindow = new Map(Object.entries(TOOL_WINDOW_ACTIONS).map(([action, id]) => [id, action]));

/** View | Tool Windows: every tool window the project shows, alphabetically like PHPStorm. */
registerMainMenuDynamic("toolWindows", () =>
  toolWindows()
    .filter((tw) => tw.when?.() ?? true)
    .sort((a, b) => a.title.localeCompare(b.title))
    .map((tw) => {
      const action = actionOfToolWindow.get(tw.id);
      return {
        key: tw.id,
        label: tw.title,
        action,
        run: () => {
          if (action) void runAction(action);
          else toggleToolWindow(tw.id);
        },
      };
    }),
);

/** File | Recent Projects: the recent projects except the open one. */
registerMainMenuDynamic("recentProjects", async () => {
  const current = workspace()?.root;
  const recent = await workspaceApi.recent();
  return recent
    .filter((p) => p.root !== current)
    .slice(0, 15)
    .map((p) => ({
      key: p.root,
      label: p.name,
      detail: p.remote ? `${p.remote.label}:${p.remote.path}` : p.root,
      run: () => void (p.remote ? openRecentRemote(p) : openProject(p.root)),
    }));
});

if (typeof document !== "undefined") {
  const host = document.createElement("div");
  host.id = "main-menu-overlay";
  document.body.appendChild(host);
  render(() => <AboutHost />, host);
}
