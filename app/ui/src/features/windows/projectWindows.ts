/**
 * Several project windows, like PHPStorm: opening a project while another one is open asks
 * "New Window / This Window" (Settings | System Settings | Project: "Open project in"), every
 * project window has its own project state in Rust (one Forge instance per window, see
 * `app/src-tauri/src/commands/project_windows.rs`), and Window | Next / Previous Project
 * Window (Ctrl+Alt+] / Ctrl+Alt+[) and the window list at the end of the Window menu switch
 * between them.
 */
import { projectWindowsApi } from "@/ipc/projectWindows";
import { registerCommand } from "@/state/commands";
import { getSetting, saveSetting } from "@/state/settings";
import { openWorkspace, setProjectOpener, trustBeforeOpen, workspace } from "@/state/workspace";
import { showBalloon } from "@/keymap";
import { askChoiceWithCheckbox } from "@/features/contextMenu/dialogs";
import { action, registerMainMenuDynamic, registerMainMenuEntries } from "@/features/mainMenu/model";
import { registerKeymapActions } from "@/keymap/store";
import { t } from "@/i18n";

export const OPEN_PROJECT_IN = "system.openProjectIn";
export type OpenProjectIn = "ask" | "newWindow" | "thisWindow";

const nameOf = (path: string) => path.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || path;

/** Where to open a project: the setting, or the answer of the "Open Project" dialog. */
async function chooseWindow(path: string, what: string): Promise<"newWindow" | "thisWindow" | null> {
  const setting = getSetting<string>(OPEN_PROJECT_IN, "ask") as OpenProjectIn;
  if (setting === "newWindow" || setting === "thisWindow") return setting;
  const { id, checked } = await askChoiceWithCheckbox({
    title: "Open Project",
    message: `Where would you like to open ${what} ${nameOf(path)}?`,
    choices: [
      { id: "thisWindow", label: "This Window" },
      { id: "newWindow", label: "New Window" },
    ],
    checkbox: "Remember, don't ask again",
  });
  if (id !== "newWindow" && id !== "thisWindow") return null;
  if (checked) await saveSetting(OPEN_PROJECT_IN, id).catch(() => undefined);
  return id;
}

/**
 * Open a local project folder. Without an open project it opens in this window; otherwise
 * in a new window or in this one, as the setting or the user decides.
 */
export async function openProject(path: string, what = "the project"): Promise<void> {
  const current = workspace();
  if (!current) return void (await openWorkspace(path));
  if (current.root === path) return;
  const where = await chooseWindow(path, what);
  if (where === "thisWindow") return void (await openWorkspace(path));
  if (where === "newWindow") {
    // The new window opens the folder trusted or in Safe Mode, as decided here.
    if ((await trustBeforeOpen(path)) === "cancel") return;
    try {
      await projectWindowsApi.open(path);
    } catch (e) {
      showBalloon(t("tools.windows.cannotOpenANewWindow"), String((e as Error)?.message ?? e));
    }
  }
}

setProjectOpener(openProject);

async function cycle(delta: 1 | -1): Promise<void> {
  try {
    await projectWindowsApi.cycle(delta);
  } catch (e) {
    showBalloon(t("tools.windows.cannotSwitchTheProjectWindow"), String((e as Error)?.message ?? e));
  }
}

/** File | New Window: another window on the Welcome screen, e.g. to join a Code With Me session. */
async function newWindow(): Promise<void> {
  try {
    await projectWindowsApi.openEmpty();
  } catch (e) {
    showBalloon(t("tools.windows.cannotOpenANewWindow"), String((e as Error)?.message ?? e));
  }
}

registerKeymapActions([{ id: "NewWindow", title: "New Window", group: "File" }]);
registerCommand({ id: "NewWindow", title: "New Window", category: "File", run: newWindow });
registerMainMenuEntries("File", [action("NewWindow", "New _Window")], { after: "OpenFile" });

registerCommand({ id: "NextProjectWindow", title: "Next Project Window", category: "Window", run: () => cycle(1) });
registerCommand({ id: "PreviousProjectWindow", title: "Previous Project Window", category: "Window", run: () => cycle(-1) });

/** The end of the Window menu: one entry per project window, the current one checked. */
registerMainMenuDynamic("projectWindows", async () => {
  const windows = await projectWindowsApi.list();
  return windows.map((w) => ({
    key: w.id,
    label: w.name ?? "Welcome to Forge",
    detail: w.root ?? undefined,
    checked: w.current,
    run: () => {
      if (!w.current) void projectWindowsApi.focus(w.id).catch((e) => showBalloon(t("tools.windows.cannotSwitchTheProjectWindow"), String(e)));
    },
  }));
});
