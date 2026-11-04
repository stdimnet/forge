/**
 * Editor tabs and split editors (PHPStorm: Window | Editor Tabs and the tab context menu):
 * Split Right / Down, Split and Move, Move to Opposite Group, Unsplit, Pin Tab, the close
 * actions (Close Others, Close All, to the Left/Right, Unmodified) that keep pinned tabs,
 * Copy Path and Select in Project View. The tab rules live in `state/tabGroups.ts`.
 */
import { registerCommand } from "@/state/commands";
import { actionById, registerKeymapActions, showBalloon } from "@/keymap";
import {
  activeGroupId,
  changeSplitOrientation,
  closeAllTabsIn,
  closeOtherTabs,
  closeTabIn,
  closeTabsToSide,
  closeUnmodifiedTabs,
  editor,
  editorGroups,
  focusNeighbourGroup,
  groupActive,
  isCustomTab,
  isTabPinned,
  moveTabToOppositeGroup,
  splitEditor,
  togglePinTab,
  unsplitEditor,
} from "@/state/editor";
import { workspace } from "@/state/workspace";
import { showToolWindow } from "@/state/layout";
import { groupView } from "@/editor/views";
import { item, providedEntries, separator, showContextMenu, submenu, type MenuEntry } from "@/features/contextMenu/state";
import { registerMainMenuEntries } from "@/features/mainMenu";
import { action as menuAction } from "@/features/mainMenu/model";
import { selectInTree } from "@/features/project/ProjectView";
import { setTabMenuProvider } from "@/shell/tabMenu";
import { windowApi } from "@/ipc/window";
import { fileManagerName } from "@/features/editorNavigation/selectIn";
import { canDetach, openTabInNewWindow } from "@/features/windows/detachedWindows";
import "./recentLocations";
import "./editorTabs.css";
import { t } from "@/i18n";
import { writeClipboardText } from "@/features/clipboard/system";

registerKeymapActions([
  { id: "MoveTabRight", title: "Split and Move Right", group: "Main Menu/Window/Editor Tabs" },
  { id: "MoveTabDown", title: "Split and Move Down", group: "Main Menu/Window/Editor Tabs" },
  { id: "MoveEditorToOppositeTabGroup", title: "Move to Opposite Group", group: "Main Menu/Window/Editor Tabs" },
  { id: "ChangeSplitOrientation", title: "Change Splitter Orientation", group: "Main Menu/Window/Editor Tabs" },
  { id: "Unsplit", title: "Unsplit", group: "Main Menu/Window/Editor Tabs" },
  { id: "UnsplitAll", title: "Unsplit All", group: "Main Menu/Window/Editor Tabs" },
  { id: "CloseAllToTheLeft", title: "Close Tabs to the Left", group: "Main Menu/Window/Editor Tabs" },
  { id: "CloseAllToTheRight", title: "Close Tabs to the Right", group: "Main Menu/Window/Editor Tabs" },
  { id: "SelectInProjectView", title: "Select in Project View", group: "Main Menu/Navigate" },
]);

function action(id: string, run: () => void | Promise<void>): void {
  const def = actionById(id);
  registerCommand({ id, title: def?.title ?? id, category: "Editor Tabs", run });
}

/** The active group and its active tab, if any. */
function current(): { group: number; id: string } | undefined {
  const group = activeGroupId();
  const id = groupActive(group);
  return id ? { group, id } : undefined;
}

const withCurrent = (fn: (group: number, id: string) => void) => () => {
  const c = current();
  if (c) fn(c.group, c.id);
};

/** Give the keyboard focus to the editor of a group after the layout changed. */
function focusGroupEditor(group: number | undefined): void {
  if (group === undefined) return;
  requestAnimationFrame(() => groupView(group)?.focus());
}

const isSplit = () => editorGroups().length > 1;

function splitAndMove(orientation: "vertical" | "horizontal", group: number, id: string): void {
  splitEditor(orientation, group, id);
  const target = activeGroupId();
  closeTabIn(group, id);
  focusGroupEditor(target);
}

action("SplitVertically", withCurrent((g, id) => (splitEditor("vertical", g, id), focusGroupEditor(activeGroupId()))));
action("SplitHorizontally", withCurrent((g, id) => (splitEditor("horizontal", g, id), focusGroupEditor(activeGroupId()))));
action("MoveTabRight", withCurrent((g, id) => splitAndMove("vertical", g, id)));
action("MoveTabDown", withCurrent((g, id) => splitAndMove("horizontal", g, id)));
action(
  "MoveEditorToOppositeTabGroup",
  withCurrent((g, id) => {
    if (!isSplit()) return void showBalloon(t("tools.editorTabs.thereIsNoOppositeGroup"), t("tools.editorTabs.splitTheEditorFirstWindow"));
    moveTabToOppositeGroup(g, id);
    focusGroupEditor(activeGroupId());
  }),
);
action("ChangeSplitOrientation", changeSplitOrientation);
action("Unsplit", () => {
  unsplitEditor(activeGroupId());
  focusGroupEditor(activeGroupId());
});
action("UnsplitAll", () => {
  unsplitEditor();
  focusGroupEditor(activeGroupId());
});
action("NextSplitter", () => focusGroupEditor(focusNeighbourGroup(1)));
action("PrevSplitter", () => focusGroupEditor(focusNeighbourGroup(-1)));
action("PinActiveEditorTab", withCurrent((_, id) => togglePinTab(id)));
action("CloseAllToTheLeft", withCurrent((g, id) => closeTabsToSide(g, id, "left")));
action("CloseAllToTheRight", withCurrent((g, id) => closeTabsToSide(g, id, "right")));

/** Select a file in the Project tool window (Alt+F1, 1 in PHPStorm). */
function selectInProject(path: string): void {
  if (isCustomTab(path) || path.includes("://")) return;
  showToolWindow("project");
  selectInTree(path);
}
action("SelectInProjectView", () => {
  if (editor.active) selectInProject(editor.active);
});

registerMainMenuEntries(
  "Window/Editor Tabs",
  [menuAction("MoveTabRight"), menuAction("MoveTabDown"), menuAction("MoveEditorToOppositeTabGroup"), menuAction("ChangeSplitOrientation"), menuAction("Unsplit"), menuAction("UnsplitAll")],
  { after: "SplitHorizontally" },
);
registerMainMenuEntries("Window/Editor Tabs", [menuAction("CloseAllToTheLeft"), menuAction("CloseAllToTheRight")], { after: "CloseAllEditors" });
registerMainMenuEntries("Window/Editor Tabs", [menuAction("EditSourceInNewWindow")], { after: "UnsplitAll" });

async function copy(text: string, what: "path" | "name"): Promise<void> {
  try {
    await writeClipboardText(text);
    showBalloon(t(`editor.tabs.copied.${what}`), text);
  } catch {
    showBalloon(t(`editor.tabs.copyFailed.${what}`), t("project.clipboardUnavailable"));
  }
}

const absolute = (path: string) => {
  const root = workspace()?.root ?? "";
  return root ? `${root.replace(/\/$/, "")}/${path}` : path;
};

function revealInFileManager(path: string): void {
  windowApi.revealInFileManager(path).catch((e) => {
    showBalloon(t("editor.tabs.revealFailed", { fileManager: fileManagerName() }), String(e instanceof Error ? e.message : e));
  });
}

/** The context menu of an editor tab (PHPStorm's editor tab popup). */
export function tabMenuEntries(group: number, id: string): MenuEntry[] {
  const pinned = isTabPinned(id);
  const file = !isCustomTab(id) && !id.includes("://");
  const entries: MenuEntry[] = [
    item(t("editor.tabs.close"), () => closeTabIn(group, id), "CloseContent"),
    item(t("editor.tabs.closeOthers"), () => closeOtherTabs(group, id), "CloseAllEditorsButActive"),
    item(t("editor.tabs.closeAll"), () => closeAllTabsIn(group), "CloseAllEditors"),
    item(t("editor.tabs.closeUnmodified"), () => closeUnmodifiedTabs(group), "CloseAllUnmodifiedEditors"),
    item(t("editor.tabs.closeLeft"), () => closeTabsToSide(group, id, "left"), "CloseAllToTheLeft"),
    item(t("editor.tabs.closeRight"), () => closeTabsToSide(group, id, "right"), "CloseAllToTheRight"),
    separator,
    item(pinned ? t("editor.tabs.unpin") : t("editor.tabs.pin"), () => togglePinTab(id), "PinActiveEditorTab"),
    separator,
    item(t("editor.tabs.splitRight"), () => (splitEditor("vertical", group, id), focusGroupEditor(activeGroupId())), "SplitVertically"),
    item(t("editor.tabs.splitDown"), () => (splitEditor("horizontal", group, id), focusGroupEditor(activeGroupId())), "SplitHorizontally"),
    item(t("editor.tabs.splitMoveRight"), () => splitAndMove("vertical", group, id), "MoveTabRight"),
    item(t("editor.tabs.splitMoveDown"), () => splitAndMove("horizontal", group, id), "MoveTabDown"),
  ];
  if (isSplit()) {
    entries.push(
      item(t("editor.tabs.moveOpposite"), () => (moveTabToOppositeGroup(group, id), focusGroupEditor(activeGroupId())), "MoveEditorToOppositeTabGroup"),
      item(t("editor.tabs.changeOrientation"), changeSplitOrientation, "ChangeSplitOrientation"),
      item(t("editor.tabs.unsplit"), () => unsplitEditor(group), "Unsplit"),
      item(t("editor.tabs.unsplitAll"), () => unsplitEditor(), "UnsplitAll"),
    );
  }
  if (file) {
    const name = id.split("/").pop() ?? id;
    entries.push(
      separator,
      submenu(t("editor.tabs.copyPathReference"), [
        item(t("editor.tabs.absolutePath"), () => void copy(absolute(id), "path"), "CopyPaths"),
        item(t("editor.tabs.pathFromRoot"), () => void copy(id, "path")),
        item(t("editor.tabs.fileName"), () => void copy(name, "name")),
      ]),
      item(t("editor.tabs.selectInProject"), () => selectInProject(id), "SelectInProjectView"),
      item(t("editor.tabs.showIn", { fileManager: fileManagerName() }), () => revealInFileManager(id)),
    );
    // Override File Type and friends (features/fileTypes).
    const more = providedEntries("editor.tab", id);
    if (more.length) entries.push(separator, ...more);
  }
  if (canDetach(id)) {
    entries.push(separator, item(t("editor.tabs.openInNewWindow"), () => openTabInNewWindow(group, id), "EditSourceInNewWindow"));
  }
  return entries;
}

setTabMenuProvider((x, y, group, id) => {
  showContextMenu(tabMenuEntries(group, id), x, y, () => groupView(group)?.focus());
});
