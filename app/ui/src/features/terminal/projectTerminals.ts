/**
 * Terminals belong to a project (PHPStorm's Terminal does the same):
 *
 * - `beforeClose`: commands still running (Rust asks each PTY for its foreground process)
 *   ask "Terminate …?" with Terminate / Cancel; Cancel keeps the project open.
 * - `closing`: the project's layout (tabs, names, directories, shell profiles) is saved as
 *   workspace state in Rust (`terminal_layout_save`, never localStorage), the tabs go; Rust
 *   ends the shells and keeps their output when `terminal.restoreScrollback` is on.
 * - `opened`: the new project's tabs come back, with their output; without a saved layout the
 *   Terminal opens a fresh tab as usual.
 *
 * The layout is also saved a moment after every change, so a crash keeps it.
 */
import { createEffect, createRoot, createSignal, on } from "solid-js";
import { terminalApi, type TerminalLayout } from "@/ipc/terminal";
import { onProject, type LifecycleProject } from "@/state/projectLifecycle";
import { isCollabGuest, workspace } from "@/state/workspace";
import { askChoice } from "@/features/contextMenu/dialogs";
import { t } from "@/i18n";
import { activeTab, displayTitle, replaceTabs, setRestoringTabs, shellProfiles, tabById, tabs, tabsFromLayout, terminalLayoutOf } from "./store";
import { disposeAllSessions, setRestoredHistory } from "./sessions";

const SAVE_DELAY_MS = 500;
let timer: ReturnType<typeof setTimeout> | undefined;
/** The root whose tabs the Terminal shows; null while none are restored yet. */
const [shownRoot, setShownRoot] = createSignal<string | null>(null);

/** A Code With Me guest's terminals are the host's: nothing to keep. */
const keepsLayout = (project: LifecycleProject) => !project.collab;

const layoutFor = (root: string): TerminalLayout => terminalLayoutOf(tabs(), activeTab(), root, shellProfiles());

function save(root: string, layout: TerminalLayout): void {
  void terminalApi.saveLayout(root, layout).catch((e) => console.warn("the terminal layout could not be saved", e));
}

/** "Terminate 2 running processes?" for the claimed objections; true for Terminate. */
async function confirmTerminate(processes: string[]): Promise<boolean> {
  const id = await askChoice({
    title: t("tools.terminal.lifecycle.title"),
    message: t("tools.terminal.lifecycle.message", { count: processes.length, processes: processes.join(", ") }),
    choices: [
      { id: "terminate", label: t("tools.terminal.lifecycle.terminate") },
    ],
  });
  return id === "terminate";
}

onProject(
  "beforeClose",
  async (_project, ctx) => {
    const running = ctx.claim("terminal.running");
    // `sleep (Local)`, `npm (Local (2))`: the program and the tab it runs in.
    const processes = running.flatMap((o) =>
      o.items.map((process, i) => {
        const tab = tabById(o.targets?.[i] ?? "");
        return tab ? `${process} (${displayTitle(tab)})` : process;
      }),
    );
    if (!processes.length) return true;
    return confirmTerminate(processes);
  },
  { name: "terminals" },
);

onProject(
  "closing",
  (project) => {
    clearTimeout(timer);
    // Nothing shows while the next project's tabs load.
    setRestoringTabs(true);
    if (keepsLayout(project) && shownRoot() === project.root) save(project.root, layoutFor(project.root));
    setShownRoot(null);
    disposeAllSessions();
    replaceTabs([], null);
  },
  { name: "terminals", order: -20 },
);

onProject(
  "opened",
  async (project) => {
    try {
      if (!keepsLayout(project)) return;
      const layout = await terminalApi.loadLayout(project.root).catch(() => null);
      if (workspace()?.root !== project.root) return;
      if (layout?.tabs.length) {
        for (const tab of layout.tabs) if (tab.scrollback) setRestoredHistory(tab.id, tab.scrollback);
        // Tabs opened meanwhile (Open In > Terminal right after opening) stay after the restored ones.
        replaceTabs([...tabsFromLayout(layout, shellProfiles()), ...tabs()], layout.active ?? null);
      }
      setShownRoot(project.root);
    } finally {
      if (workspace()?.root === project.root) setRestoringTabs(false);
    }
  },
  { name: "terminals" },
);

// Save shortly after every change of the tabs (title, directory, order, selection).
createRoot(() => {
  createEffect(
    on(
      () => {
        const root = shownRoot();
        return root ? JSON.stringify(layoutFor(root)) : null;
      },
      (json) => {
        const root = shownRoot();
        if (!root || !json || isCollabGuest()) return;
        clearTimeout(timer);
        const layout = layoutFor(root);
        timer = setTimeout(() => {
          if (shownRoot() === root) save(root, layout);
        }, SAVE_DELAY_MS);
      },
      { defer: true },
    ),
  );
});
