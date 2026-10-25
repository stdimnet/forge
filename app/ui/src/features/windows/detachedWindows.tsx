/**
 * Editor tabs and tool windows in their own windows, like PHPStorm:
 *
 * - Open in New Window (Shift+F4, the tab menu, or dragging a tab out of the IDE window)
 *   moves an editor tab into a window of its own. Closing that window docks its tabs back,
 *   unsaved changes included.
 * - View Mode › Window shows a tool window in a separate window; closing it hides the tool
 *   window, another View Mode puts it back into the IDE window.
 *
 * Both are non-modal dialog windows (`shell/window/dialogs.ts`, `modal: false`) of the same
 * project process, so they share its Rust state. In browser mode an editor window opens as a
 * browser popup and a tool window as a panel inside the page.
 */
import { createEffect, createRoot, on, onCleanup, onMount, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { findToolWindow } from "@/features/registry";
import { registerCommand } from "@/state/commands";
import {
  activateOpenTab,
  activeGroupId,
  closeTabIn,
  editor,
  fileSystemFor,
  groupActive,
  isCustomTab,
  isDirty,
  openFile,
  setOpenFileForwarder,
  updateContent,
} from "@/state/editor";
import { hideToolWindow, undockedWindows, viewModeOf } from "@/state/layout";
import { workspace } from "@/state/workspace";
import { showBalloon } from "@/keymap";
import { EditorArea } from "@/shell/EditorArea";
import { setTabDragOutHandler } from "@/shell/tabMenu";
import {
  closeDialog,
  closeOwnedDialog,
  instanceId,
  notifyDialogOpener,
  onDialogCancel,
  openDialogWindow,
  registerDialog,
} from "@/shell/window/dialogs";
import { dialogBoot, isDialogWindow } from "@/shell/window/mode";

// ---- Editor tabs in their own window ---------------------------------------------------

/** A file moved into an editor window; `content` is set when it has unsaved changes. */
export interface DetachedFile {
  path: string;
  content?: string;
}

interface EditorWindowProps {
  window: string;
  files: DetachedFile[];
  active?: string | null;
}

interface EditorWindowResult {
  files: DetachedFile[];
  active?: string | null;
}

const fileName = (path: string) => path.split("/").pop() ?? path;

/** Open files with their unsaved text (into this window's editor). */
async function openDetached(files: DetachedFile[], active?: string | null): Promise<void> {
  for (const f of files) {
    await openFile(f.path, { activate: false }).catch(() => undefined);
    if (f.content !== undefined) updateContent(f.path, f.content);
  }
  const target = active && files.some((f) => f.path === active) ? active : files[files.length - 1]?.path;
  if (target) activateOpenTab(target);
}

/** The project files open in this window's editor, with their unsaved text. */
function openFiles(): EditorWindowResult {
  const files = editor.tabs
    .filter((t) => !t.view && !fileSystemFor(t.path))
    .map((t) => (isDirty(t) ? { path: t.path, content: t.content } : { path: t.path }));
  return { files, active: editor.active };
}

function EditorWindow(props: { props: EditorWindowProps }) {
  onMount(() => void openDetached(props.props.files, props.props.active));
  // Closing the window (its close button, Alt+F4) hands the tabs back to the project window.
  const off = onDialogCancel(props.props.window, () => closeDialog(props.props.window, openFiles()));
  onCleanup(off);
  return (
    <div class="detached-editor" data-detached-editor>
      <EditorArea />
    </div>
  );
}

registerDialog<EditorWindowProps, EditorWindowResult>({
  id: "editor",
  title: (p) => `${fileName(p.files.find((f) => f.path === p.active)?.path ?? p.files[0]?.path ?? "Editor")} – ${workspace()?.name ?? "Forge"}`,
  width: 960,
  height: 680,
  minWidth: 420,
  minHeight: 300,
  modal: false,
  browserPopup: true,
  component: EditorWindow,
  // The window closed: its tabs come back as tabs of the project window.
  onClosed: (result, props) => {
    const back = result ?? { files: props.files, active: props.active };
    void openDetached(back.files, back.active);
  },
});

let counter = 0;

/** Can this tab move into a window of its own? Project files only (no views or consoles). */
export function canDetach(id: string): boolean {
  if (isCustomTab(id) || fileSystemFor(id)) return false;
  const tab = editor.tabs.find((t) => t.path === id);
  return !!tab && !tab.view;
}

/** Window | Editor Tabs | Open in New Window (Shift+F4): move a tab into a new window. */
export function openTabInNewWindow(group: number, id: string): void {
  if (isDialogWindow()) return;
  if (!canDetach(id)) return void showBalloon("This tab cannot open in a new window", "Only project files can move into their own window.");
  const tab = editor.tabs.find((t) => t.path === id)!;
  const window = instanceId("editor", `${Date.now().toString(36)}${counter++}`);
  const files: DetachedFile[] = [isDirty(tab) ? { path: id, content: tab.content } : { path: id }];
  closeTabIn(group, id);
  void openDialogWindow<EditorWindowResult>(window, { window, files, active: id });
}

// ---- Tool windows in their own window (View Mode › Window) -----------------------------

interface ToolWindowWindowProps {
  window: string;
  id: string;
}

function ToolWindowWindow(props: { props: ToolWindowWindowProps }) {
  const tw = () => findToolWindow(props.props.id);
  // A separate window has no editor: files open in the project window.
  if (dialogBoot()?.id === props.props.window) {
    setOpenFileForwarder((path) => {
      notifyDialogOpener(props.props.window, { type: "openFile", path });
      return true;
    });
    onCleanup(() => setOpenFileForwarder(null));
  }
  return (
    <Show when={tw()} fallback={<div class="wf-missing">This tool window is not available.</div>}>
      {(w) => (
        <section class="tool-window detached-tool-window" data-tool-window-panel={w().id}>
          <Show when={w().headerContent || w().actions}>
            <header class="tool-window-header">
              <div class="tool-window-header-content">{w().headerContent?.()}</div>
              <div class="tool-window-actions">{w().actions?.()}</div>
            </header>
          </Show>
          <div class="tool-window-body">
            <Dynamic component={w().component} />
          </div>
        </section>
      )}
    </Show>
  );
}

/** Windows this window closes itself (mode changed, hidden): no need to hide the tool window. */
const closingOnPurpose = new Set<string>();

registerDialog<ToolWindowWindowProps>({
  id: "toolwindow",
  title: (p) => findToolWindow(p.id)?.title ?? p.id,
  width: 560,
  height: 420,
  minWidth: 260,
  minHeight: 160,
  modal: false,
  component: ToolWindowWindow,
  onMessage: (message) => {
    const m = message as { type?: string; path?: string } | null;
    if (m?.type === "openFile" && m.path) void openFile(m.path);
  },
  // The user closed the window: like PHPStorm, the tool window is hidden.
  onClosed: (_result, props) => {
    if (closingOnPurpose.delete(props.window)) return;
    if (viewModeOf(props.id) === "window") hideToolWindow(props.id);
  },
});

/** Keep a window open for every open tool window in Window mode (project window only). */
function syncToolWindowWindows(): void {
  const open = new Map<string, string>();
  createEffect(
    on(
      () =>
        undockedWindows()
          .filter((w) => w.mode === "window")
          .map((w) => w.id)
          .join("\n"),
      (joined) => {
        const wanted = new Set(joined ? joined.split("\n") : []);
        for (const [id, window] of open) {
          if (wanted.has(id)) continue;
          open.delete(id);
          closingOnPurpose.add(window);
          void closeOwnedDialog(window).finally(() => closingOnPurpose.delete(window));
        }
        for (const id of wanted) {
          if (open.has(id)) continue;
          const window = instanceId("toolwindow", id);
          open.set(id, window);
          void openDialogWindow(window, { window, id }).then(() => {
            if (open.get(id) === window) open.delete(id);
          });
        }
      },
    ),
  );
}

if (typeof document !== "undefined" && !isDialogWindow()) {
  createRoot(syncToolWindowWindows);
}

// ---- Actions -------------------------------------------------------------------------------

registerCommand({
  id: "EditSourceInNewWindow",
  title: "Open in New Window",
  category: "Editor Tabs",
  run: () => {
    const group = activeGroupId();
    const id = groupActive(group);
    if (id) openTabInNewWindow(group, id);
  },
});

// Dragging a tab out of the IDE window opens it in a new window.
setTabDragOutHandler((group, id) => {
  if (canDetach(id)) openTabInNewWindow(group, id);
});
