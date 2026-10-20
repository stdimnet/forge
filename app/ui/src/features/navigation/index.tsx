/**
 * Navigation feature: installs the PHPStorm keymap and registers the actions Forge can
 * already perform (tool windows, tabs, Search Everywhere, Recent Files, editor actions).
 * Every other PHPStorm action stays bound and shows "not available yet" until a feature
 * registers a command with its id.
 */
import { render } from "solid-js/web";
import { commands, registerCommand } from "@/state/commands";
import { layout, resizePanel, showToolWindow, toggleToolWindow } from "@/state/layout";
import { findToolWindow, type ToolWindowAnchor } from "@/features/registry";
import { closeTab, editor, isDirty, openFile, saveAll } from "@/state/editor";
import { workspace } from "@/state/workspace";
import { toggleTheme } from "@/state/theme";
import { actionById, installKeymap, runAction, showBalloon } from "@/keymap";
import { openSearchEverywhere } from "@/features/search/state";
import "@/features/search/builtinProviders";
import { EDITOR_COMMANDS, activeView, runInEditor } from "./editorCommands";
import { navigateBack, navigateForward, reopenClosedTab } from "./history";
import { openRecentFiles, recentOpen } from "./RecentFiles";
import { openFallbackSettings } from "./SettingsFallback";
import { PopupHost } from "./PopupHost";
import "./navigation.css";

function action(id: string, run: () => void | Promise<void>): void {
  const def = actionById(id);
  registerCommand({ id, title: def?.title ?? id, category: def?.group.split("/").pop(), run });
}

// ---- Search Everywhere & friends
action("SearchEverywhere", () => openSearchEverywhere("all"));
action("GotoAction", () => openSearchEverywhere("actions"));
action("GotoClass", () => openSearchEverywhere("classes"));
action("GotoFile", () => openSearchEverywhere("files"));
action("GotoSymbol", () => openSearchEverywhere("symbols"));
// The toolbar's search button uses this id.
registerCommand({ id: "search.everywhere", title: "Search Everywhere", run: () => openSearchEverywhere("all") });

// ---- Recent files, switcher, history
action("RecentFiles", () => openRecentFiles("recent"));
action("RecentChangedFiles", () => openRecentFiles("recent", true));
action("Switcher", () => (recentOpen() === "switcher" ? undefined : openRecentFiles("switcher")));
action("Back", navigateBack);
action("Forward", navigateForward);
action("ReopenClosedTab", reopenClosedTab);

// ---- Tool windows
const TOOL_WINDOW_ACTIONS: Record<string, string> = {
  ActivateProjectToolWindow: "project",
  ActivateBookmarksToolWindow: "bookmarks",
  ActivateFindToolWindow: "find",
  ActivateRunToolWindow: "run",
  ActivateDebugToolWindow: "debug",
  ActivateProblemsViewToolWindow: "problems",
  ActivateStructureToolWindow: "structure",
  ActivateServicesToolWindow: "services",
  ActivateVersionControlToolWindow: "git",
  ActivateCommitToolWindow: "commit",
  ActivateTerminalToolWindow: "terminal",
  ActivateDatabaseToolWindow: "database",
  ActivateNotificationsToolWindow: "notifications",
  ActivateAIChatToolWindow: "ai-chat",
  ActivatePullRequestsToolWindow: "pull-requests",
  ActivateTODOToolWindow: "todo",
};

let lastToolWindow: string | null = null;
let hiddenLayout: Partial<Record<ToolWindowAnchor, string>> | null = null;

function focusToolWindow(id: string): void {
  queueMicrotask(() =>
    document.querySelector<HTMLElement>(`[data-tool-window-panel="${id}"] .tool-window-body`)?.focus?.(),
  );
}

for (const [actionId, twId] of Object.entries(TOOL_WINDOW_ACTIONS)) {
  action(actionId, () => {
    const tw = findToolWindow(twId);
    if (!tw) {
      showBalloon(`${actionById(actionId)?.title ?? twId} tool window is not available yet`);
      return;
    }
    // Like PHPStorm: a visible tool window is hidden, otherwise it is shown and focused.
    if (layout.active[tw.anchor] === tw.id) {
      toggleToolWindow(tw.anchor, tw.id);
      activeView()?.focus();
    } else {
      showToolWindow(tw.anchor, tw.id);
      lastToolWindow = tw.id;
      hiddenLayout = null;
      focusToolWindow(tw.id);
    }
  });
}

const ANCHORS: ToolWindowAnchor[] = ["left", "right", "bottom"];

action("HideAllWindows", () => {
  const visible = ANCHORS.filter((a) => layout.active[a]);
  if (visible.length) {
    hiddenLayout = Object.fromEntries(visible.map((a) => [a, layout.active[a]!]));
    for (const a of visible) toggleToolWindow(a, layout.active[a]!);
  } else if (hiddenLayout) {
    for (const [a, id] of Object.entries(hiddenLayout)) showToolWindow(a as ToolWindowAnchor, id);
    hiddenLayout = null;
  }
});
action("HideSideWindows", () => {
  for (const a of ["left", "right"] as const) if (layout.active[a]) toggleToolWindow(a, layout.active[a]!);
});

/** Anchor of the tool window that has focus. */
function focusedAnchor(): ToolWindowAnchor | null {
  const el = document.activeElement?.closest<HTMLElement>("[data-tool-window-panel]");
  const tw = el ? findToolWindow(el.dataset.toolWindowPanel ?? null) : undefined;
  return tw?.anchor ?? null;
}

action("HideActiveWindow", () => {
  const a = focusedAnchor();
  if (a && layout.active[a]) {
    toggleToolWindow(a, layout.active[a]!);
    activeView()?.focus();
  }
});
action("JumpToLastWindow", () => {
  const tw = findToolWindow(lastToolWindow) ?? findToolWindow(layout.active.left);
  if (!tw) return;
  showToolWindow(tw.anchor, tw.id);
  focusToolWindow(tw.id);
});

const stretch = (dir: "left" | "right" | "up" | "down") => () => {
  const a = focusedAnchor();
  if (!a) return;
  const step = 24;
  const grow = (a === "left" && dir === "right") || (a === "right" && dir === "left") || (a === "bottom" && dir === "up");
  const shrink = (a === "left" && dir === "left") || (a === "right" && dir === "right") || (a === "bottom" && dir === "down");
  if (grow || shrink) resizePanel(a, layout.size[a] + (grow ? step : -step));
};
action("ResizeToolWindowLeft", stretch("left"));
action("ResizeToolWindowRight", stretch("right"));
action("ResizeToolWindowUp", stretch("up"));
action("ResizeToolWindowDown", stretch("down"));

// ---- Editor tabs & files
action("SaveAll", saveAll);
action("CloseContent", () => {
  if (editor.active) closeTab(editor.active);
});
action("CloseAllEditors", () => editor.tabs.map((t) => t.path).forEach(closeTab));
action("CloseAllEditorsButActive", () =>
  editor.tabs.filter((t) => t.path !== editor.active).map((t) => t.path).forEach(closeTab),
);
action("CloseAllUnmodifiedEditors", () =>
  editor.tabs.filter((t) => !isDirty(t)).map((t) => t.path).forEach(closeTab),
);

function cycleTab(delta: number): void {
  const tabs = editor.tabs;
  if (!tabs.length) return;
  const i = tabs.findIndex((t) => t.path === editor.active);
  void openFile(tabs[(i + delta + tabs.length) % tabs.length].path);
}
action("NextTab", () => cycleTab(1));
action("PreviousTab", () => cycleTab(-1));

async function copy(text: string, what: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    showBalloon(`${what} copied`, text);
  } catch {
    showBalloon(`Could not copy ${what.toLowerCase()}`);
  }
}
action("CopyPaths", () => {
  const ws = workspace();
  if (editor.active) void copy(ws ? `${ws.root.replace(/\/$/, "")}/${editor.active}` : editor.active, "Path");
});
action("CopyReference", () => {
  const tab = editor.tabs.find((t) => t.path === editor.active);
  if (tab) void copy(`${tab.path}:${tab.cursor.line}`, "Reference");
});

// ---- Appearance & settings
action("ChangeLaf", toggleTheme);
action("QuickChangeScheme", toggleTheme);
action("ShowSettings", () => runAction("settings.open"));
// Until the real Settings dialog lands, settings.open shows the registered pages in a
// minimal dialog. A later registerCommand("settings.open") replaces this one.
if (!commands().has("settings.open")) {
  registerCommand({ id: "settings.open", title: "Settings", run: () => openFallbackSettings() });
}
registerCommand({ id: "settings.keymap", title: "Keymap Settings", category: "Settings", run: () => openFallbackSettings("keymap") });

// ---- Editor actions backed by CodeMirror
for (const [id, cmd] of Object.entries(EDITOR_COMMANDS)) action(id, () => runInEditor(cmd));

// ---- Mount popups and start listening for shortcuts
if (typeof document !== "undefined") {
  installKeymap();
  const host = document.createElement("div");
  host.id = "forge-popups";
  document.body.appendChild(host);
  render(() => <PopupHost />, host);
}
