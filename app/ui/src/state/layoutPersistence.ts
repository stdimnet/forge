/**
 * Per-project workbench layout, like PHPStorm's workspace state: which tool windows are
 * open, their sizes, the selected tabs inside tool windows and the open editor tabs. Saved
 * by Rust in Forge's state dir (`layout_save`, never in the project or localStorage) a moment
 * after every change and restored when the project opens.
 */
import { createEffect, createRoot, on } from "solid-js";
import { projectStateApi, type LayoutAnchor, type ProjectLayout } from "@/ipc/projectState";
import { toolWindows } from "@/features/registry";
import { activateOpenTab, editor, fileSystemFor, openFile } from "./editor";
import { layout, SIZE_LIMITS, setLayout } from "./layout";
import { workspace } from "./workspace";

const ANCHORS: LayoutAnchor[] = ["left", "right", "bottom"];
const SAVE_DELAY_MS = 400;

/** The current layout as it is saved. */
export function currentLayout(): ProjectLayout {
  // Project files only: no SQL consoles, table editors or scratches.
  const files = editor.tabs.filter((t) => !t.view && !fileSystemFor(t.path)).map((t) => t.path);
  return {
    active: { ...layout.active },
    sizes: { ...layout.size },
    tabs: { ...layout.tabs },
    openFiles: files,
    activeFile: editor.active && files.includes(editor.active) ? editor.active : null,
  };
}

/** Apply a saved layout; unknown tool windows and files that are gone are skipped. */
export async function applyLayout(saved: ProjectLayout): Promise<void> {
  for (const anchor of ANCHORS) {
    if (anchor in saved.active) {
      const id = saved.active[anchor] ?? null;
      const known = id === null || toolWindows().some((tw) => tw.id === id && tw.anchor === anchor);
      if (known) setLayout("active", anchor, id);
    }
    const size = saved.sizes[anchor];
    if (typeof size === "number") {
      const [min, max] = SIZE_LIMITS[anchor];
      setLayout("size", anchor, Math.max(min, Math.min(max, size)));
    }
  }
  setLayout("tabs", { ...saved.tabs });
  for (const path of saved.openFiles) {
    await openFile(path, { activate: false }).catch(() => undefined);
  }
  if (saved.activeFile) activateOpenTab(saved.activeFile);
}

let restoring = false;
let timer: ReturnType<typeof setTimeout> | undefined;

createRoot(() => {
  createEffect(
    on(
      () => workspace()?.root,
      async (root) => {
        clearTimeout(timer);
        if (!root) return;
        restoring = true;
        try {
          const saved = await projectStateApi.loadLayout().catch(() => null);
          if (saved && workspace()?.root === root) await applyLayout(saved);
        } finally {
          if (workspace()?.root === root) restoring = false;
        }
      },
    ),
  );

  // Save shortly after any change of the tracked state.
  createEffect(
    on(
      () => JSON.stringify(currentLayout()),
      () => {
        if (restoring || !workspace()) return;
        clearTimeout(timer);
        const root = workspace()?.root;
        timer = setTimeout(() => {
          if (workspace()?.root !== root || restoring) return;
          void projectStateApi.saveLayout(currentLayout()).catch((e) => console.warn("the layout could not be saved", e));
        }, SAVE_DELAY_MS);
      },
      { defer: true },
    ),
  );
});
