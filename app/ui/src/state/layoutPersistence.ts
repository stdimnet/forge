/**
 * Per-project workbench layout, like PHPStorm's workspace state: which tool windows are
 * open, where each tool window sits (moved, reordered, removed from the sidebar), the sizes
 * and split ratios of the areas, the selected tabs inside tool windows and the open editor
 * tabs. Saved
 * by Rust in Forge's state dir (`layout_save`, never in the project or localStorage) a moment
 * after every change and restored when the project opens.
 */
import { createEffect, createRoot, on } from "solid-js";
import { reconcile } from "solid-js/store";
import { projectStateApi, type LayoutAnchor, type ProjectLayout } from "@/ipc/projectState";
import { findToolWindow } from "@/features/registry";
import { activateOpenTab, editor, fileSystemFor, openFile } from "./editor";
import {
  DEFAULT_ACTIVE,
  DEFAULT_SIZES,
  DEFAULT_SPLITS,
  layout,
  resizePanel,
  resizeSplit,
  setLayout,
  slotOfToolWindow,
} from "./layout";
import { SLOTS, isSlot, type Placements } from "./toolWindowLayout";
import { workspace } from "./workspace";

const ANCHORS: LayoutAnchor[] = ["left", "right", "bottom"];
const SAVE_DELAY_MS = 400;

/** The tool window part of the layout (also what Store Current Layout as Default keeps). */
export function windowLayout(): ProjectLayout {
  // Plain objects: the store's proxies cannot be cloned.
  const toolWindows: Placements = Object.fromEntries(
    Object.entries(layout.placements).map(([id, p]) => [
      id,
      {
        slot: p.slot,
        order: p.order,
        removed: p.removed,
        ...(p.mode && p.mode !== "dockPinned" ? { mode: p.mode } : {}),
        ...(p.floating ? { floating: { ...p.floating } } : {}),
      },
    ]),
  );
  return {
    active: { ...layout.active },
    sizes: { ...layout.size },
    splits: { ...layout.splits },
    toolWindows,
    tabs: {},
    openFiles: [],
    activeFile: null,
  };
}

/** The current layout as it is saved. */
export function currentLayout(): ProjectLayout {
  // Project files only: no SQL consoles, table editors or scratches.
  const files = editor.tabs.filter((t) => !t.view && !fileSystemFor(t.path)).map((t) => t.path);
  return {
    ...windowLayout(),
    tabs: { ...layout.tabs },
    openFiles: files,
    activeFile: editor.active && files.includes(editor.active) ? editor.active : null,
  };
}

/**
 * Apply the tool window part of a saved layout: placements first, then the open windows
 * (skipped when unknown or no longer at that place), sizes and split ratios. Layouts saved
 * before placements existed have none, so every window starts at its registered place.
 */
export function applyWindowLayout(saved: ProjectLayout): void {
  const placements = Object.fromEntries(Object.entries(saved.toolWindows ?? {}).filter(([, p]) => isSlot(p.slot)));
  setLayout("placements", reconcile(placements));
  for (const slot of SLOTS) {
    if (!(slot in saved.active)) continue;
    const id = saved.active[slot] ?? null;
    const known = id === null || (!!findToolWindow(id) && slotOfToolWindow(id) === slot);
    if (known) setLayout("active", slot, id);
  }
  for (const anchor of ANCHORS) {
    const size = saved.sizes[anchor];
    if (typeof size === "number") resizePanel(anchor, size);
    const split = saved.splits?.[anchor];
    if (typeof split === "number") resizeSplit(anchor, split);
  }
}

/** Apply a saved layout; unknown tool windows and files that are gone are skipped. */
export async function applyLayout(saved: ProjectLayout): Promise<void> {
  applyWindowLayout(saved);
  setLayout("tabs", { ...saved.tabs });
  for (const path of saved.openFiles) {
    await openFile(path, { activate: false }).catch(() => undefined);
  }
  if (saved.activeFile) activateOpenTab(saved.activeFile);
}

/**
 * Window | Restore Default Layout: the layout stored with Store Current Layout as Default,
 * else the factory layout (every window at its registered place, Project open).
 */
export async function restoreDefaultLayout(): Promise<void> {
  const stored = await projectStateApi.loadDefaultLayout().catch(() => null);
  setLayout("active", { ...DEFAULT_ACTIVE, ...(stored ? Object.fromEntries(SLOTS.map((s) => [s, null])) : {}) });
  setLayout("size", { ...DEFAULT_SIZES });
  setLayout("splits", { ...DEFAULT_SPLITS });
  applyWindowLayout(stored ?? { active: {}, sizes: {}, tabs: {}, openFiles: [], activeFile: null });
}

/** Window | Store Current Layout as Default (for every project). */
export async function storeLayoutAsDefault(): Promise<void> {
  await projectStateApi.saveDefaultLayout(windowLayout());
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
