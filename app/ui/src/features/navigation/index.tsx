/**
 * Navigation feature: installs the PHPStorm keymap and registers the actions Forge can
 * already perform (tool windows, tabs, Search Everywhere, Recent Files, editor actions).
 * Every other PHPStorm action stays bound and shows "not available yet" until a feature
 * registers a command with its id.
 */
import { render } from "solid-js/web";
import { registerCommand } from "@/state/commands";
import {
  hideToolWindow,
  isToolWindowVisible,
  layout,
  resizePanel,
  setLayout,
  showToolWindow,
  slotOfToolWindow,
  type ToolWindowSlot,
} from "@/state/layout";
import { SLOTS, anchorOf } from "@/state/toolWindowLayout";
import { findToolWindow, type ToolWindowAnchor } from "@/features/registry";
import {
  activeGroupId,
  closeAllTabsIn,
  closeOtherTabs,
  closeTabIn,
  closeUnmodifiedTabs,
  cycleTabs,
  editor,
  groupActive,
  saveAll,
} from "@/state/editor";
import { workspace } from "@/state/workspace";
import { toggleTheme } from "@/state/theme";
import { actionById, installKeymap, migrateLegacyKeymap, runAction, showBalloon } from "@/keymap";
import { openSearchEverywhere } from "@/features/search/state";
import "@/features/search/builtinProviders";
import { EDITOR_COMMANDS, activeView, runInEditor } from "./editorCommands";
import { navigateBack, navigateForward, reopenClosedTab } from "./history";
import { openRecentFiles, recentOpen } from "./RecentFiles";
import { openSettings } from "@/features/settings/dialogState";
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
/** Tool window activation actions by PHPStorm id → tool window id. */
export const TOOL_WINDOW_ACTIONS: Record<string, string> = {
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
  ActivateDocumentationToolWindow: "documentation",
};

let lastToolWindow: string | null = null;
let hiddenLayout: Partial<Record<ToolWindowSlot, string>> | null = null;

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
    if (isToolWindowVisible(tw.id)) {
      hideToolWindow(tw.id);
      activeView()?.focus();
    } else {
      showToolWindow(tw.id);
      lastToolWindow = tw.id;
      hiddenLayout = null;
      focusToolWindow(tw.id);
    }
  });
}

action("HideAllWindows", () => {
  const visible = SLOTS.filter((s) => layout.active[s]);
  if (visible.length) {
    hiddenLayout = Object.fromEntries(visible.map((s) => [s, layout.active[s]!]));
    for (const s of visible) setLayout("active", s, null);
  } else if (hiddenLayout) {
    for (const id of Object.values(hiddenLayout)) if (findToolWindow(id)) showToolWindow(id);
    hiddenLayout = null;
  }
});
action("HideSideWindows", () => {
  for (const s of SLOTS) if (anchorOf(s) !== "bottom") setLayout("active", s, null);
});

/** The tool window that has focus. */
function focusedToolWindow(): string | null {
  const el = document.activeElement?.closest<HTMLElement>("[data-tool-window-panel]");
  return el?.dataset.toolWindowPanel ?? null;
}

/** Anchor of the tool window that has focus. */
function focusedAnchor(): ToolWindowAnchor | null {
  const id = focusedToolWindow();
  return id && isToolWindowVisible(id) ? anchorOf(slotOfToolWindow(id)) : null;
}

action("HideActiveWindow", () => {
  const id = focusedToolWindow();
  if (id && isToolWindowVisible(id)) {
    hideToolWindow(id);
    activeView()?.focus();
  }
});
action("JumpToLastWindow", () => {
  const tw = findToolWindow(lastToolWindow) ?? findToolWindow(layout.active.left);
  if (!tw) return;
  showToolWindow(tw.id);
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
action("SaveAll", () => saveAll("explicit"));
// Close actions work on the active editor group; pinned tabs stay on Close Others/All.
const inActiveGroup = (fn: (group: number, id: string | null) => void) => () => fn(activeGroupId(), groupActive(activeGroupId()));
action("CloseContent", inActiveGroup((g, id) => id && closeTabIn(g, id)));
action("CloseAllEditors", inActiveGroup((g) => closeAllTabsIn(g)));
action("CloseAllEditorsButActive", inActiveGroup((g, id) => id && closeOtherTabs(g, id)));
action("CloseAllUnmodifiedEditors", inActiveGroup((g) => closeUnmodifiedTabs(g)));
action("NextTab", () => cycleTabs(1));
action("PreviousTab", () => cycleTabs(-1));

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
registerCommand({ id: "settings.keymap", title: "Keymap Settings", category: "Settings", run: () => openSettings("keymap") });

// ---- Editor actions backed by CodeMirror
for (const [id, cmd] of Object.entries(EDITOR_COMMANDS)) action(id, () => runInEditor(cmd));

// ---- Mount popups and start listening for shortcuts
if (typeof document !== "undefined") {
  installKeymap();
  migrateLegacyKeymap();
  const host = document.createElement("div");
  host.id = "forge-popups";
  document.body.appendChild(host);
  render(() => <PopupHost />, host);
}
