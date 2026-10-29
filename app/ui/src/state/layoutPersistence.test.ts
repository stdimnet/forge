import { beforeAll, describe, expect, it, vi } from "vitest";
import { registerMock } from "@/ipc";
import { registerToolWindow } from "@/features/registry";
import type { ProjectLayout } from "@/ipc/projectState";
import { closeAllTabs, editor, openFile } from "./editor";
import {
  isToolWindowVisible,
  layout,
  moveFloatingToolWindow,
  moveToolWindowTo,
  dockedIn,
  removeFromSidebar,
  setToolWindowViewMode,
  viewModeOf,
  setToolWindowTab,
  showToolWindow,
  slotOfToolWindow,
  stripeToolWindows,
} from "./layout";
import { applyLayout, currentLayout, keepsLayout, restoreDefaultLayout, storeLayoutAsDefault } from "./layoutPersistence";
import { adoptWorkspace, closeWorkspace } from "./workspace";

const Empty = () => null;
const Icon = () => null;

beforeAll(() => {
  registerMock("fs_read_text", ({ path }) => ({ text: `// ${path}\n`, eol: "\n", encoding: "UTF-8", bom: false, malformed: false }));
  registerToolWindow({ id: "project", title: "Project", icon: Icon, anchor: "left", order: 0, component: Empty });
  registerToolWindow({ id: "problems", title: "Problems", icon: Icon, anchor: "bottom", order: 50, component: Empty });
  registerToolWindow({ id: "terminal", title: "Terminal", icon: Icon, anchor: "bottom", order: 30, component: Empty });
  registerToolWindow({ id: "ai-chat", title: "AI Chat", icon: Icon, anchor: "right", order: 0, component: Empty });
  let stored: ProjectLayout | null = null;
  registerMock("layout_default_load", () => (stored ? structuredClone(stored) : null));
  registerMock("layout_default_save", ({ layout: next }) => {
    stored = structuredClone(next as ProjectLayout);
  });
});

const empty = (over: Partial<ProjectLayout>): ProjectLayout => ({ active: {}, sizes: {}, tabs: {}, openFiles: [], activeFile: null, ...over });

describe("project layout", () => {
  it("captures tool windows, sizes, tool window tabs and open files", async () => {
    closeAllTabs();
    showToolWindow("problems");
    setToolWindowTab("problems", "project");
    await openFile("app/A.php");
    await openFile("app/B.php");
    const saved = currentLayout();
    expect(saved.active.bottom).toBe("problems");
    expect(saved.tabs.problems).toBe("project");
    expect(saved.openFiles).toEqual(["app/A.php", "app/B.php"]);
    expect(saved.activeFile).toBe("app/B.php");
  });

  it("restores a saved layout and skips unknown tool windows", async () => {
    closeAllTabs();
    const saved: ProjectLayout = {
      active: { left: "project", right: "no-such-window", bottom: null },
      sizes: { left: 420, bottom: 5000 },
      tabs: { problems: "file" },
      openFiles: ["routes/web.php", "app/User.php"],
      activeFile: "routes/web.php",
    };
    const rightBefore = layout.active.right;
    await applyLayout(saved);
    expect(layout.active.left).toBe("project");
    expect(layout.active.right).toBe(rightBefore);
    expect(layout.active.bottom).toBeNull();
    expect(layout.size.left).toBe(420);
    expect(layout.size.bottom).toBe(700);
    expect(layout.tabs.problems).toBe("file");
    expect(editor.tabs.map((t) => t.path)).toEqual(["routes/web.php", "app/User.php"]);
    expect(editor.active).toBe("routes/web.php");
  });

  it("saves and restores where tool windows sit", async () => {
    await applyLayout(empty({ active: { bottom: null, right: null, bottomSplit: null } }));
    showToolWindow("terminal");
    moveToolWindowTo("terminal", "right");
    // An open window stays open at its new place.
    expect(layout.active.bottom).toBeNull();
    expect(layout.active.right).toBe("terminal");
    removeFromSidebar("problems");
    const saved = currentLayout();
    expect(saved.toolWindows?.terminal).toEqual({ slot: "right", order: 20, removed: false });
    expect(saved.toolWindows?.problems.removed).toBe(true);
    // The saved layout holds plain objects (the browser mock clones it).
    expect(() => structuredClone(saved)).not.toThrow();

    await applyLayout(empty({}));
    expect(slotOfToolWindow("terminal")).toBe("bottom");
    await applyLayout(saved);
    expect(slotOfToolWindow("terminal")).toBe("right");
    expect(stripeToolWindows("right").map((t) => t.id)).toEqual(["ai-chat", "terminal"]);
    expect(stripeToolWindows("bottom").map((t) => t.id)).toEqual([]);
    expect(isToolWindowVisible("terminal")).toBe(true);
    // Opening a removed window puts its button back.
    showToolWindow("problems");
    expect(stripeToolWindows("bottom").map((t) => t.id)).toEqual(["problems"]);
  });

  it("restores layouts saved before tool windows could move", async () => {
    await applyLayout(empty({ toolWindows: { terminal: { slot: "right", order: 10, removed: false } } }));
    // Old layouts have no placements: the window is back at its registered place and the
    // open window per anchor still applies.
    await applyLayout(empty({ active: { bottom: "terminal", right: null }, sizes: { bottom: 300 } }));
    expect(slotOfToolWindow("terminal")).toBe("bottom");
    expect(layout.active.bottom).toBe("terminal");
    expect(layout.size.bottom).toBe(300);
    // An open window saved for a place it no longer has is skipped.
    await applyLayout(empty({ active: { right: "terminal" }, toolWindows: {} }));
    expect(layout.active.right).toBeNull();
  });

  it("stores the current layout as default and restores it", async () => {
    await restoreDefaultLayout();
    expect(layout.active.left).toBe("project");
    expect(slotOfToolWindow("terminal")).toBe("bottom");
    moveToolWindowTo("terminal", "leftSplit");
    showToolWindow("terminal");
    await storeLayoutAsDefault();
    moveToolWindowTo("terminal", "bottomSplit");
    await restoreDefaultLayout();
    expect(slotOfToolWindow("terminal")).toBe("leftSplit");
    expect(layout.active.leftSplit).toBe("terminal");
  });

  it("saves and restores view modes and floating bounds", async () => {
    await restoreDefaultLayout();
    setToolWindowViewMode("terminal", "float");
    moveFloatingToolWindow("terminal", { x: 40, y: 60, width: 500, height: 260 });
    showToolWindow("terminal");
    const saved = currentLayout();
    expect(saved.toolWindows?.terminal).toMatchObject({ mode: "float", floating: { x: 40, y: 60, width: 500, height: 260 } });
    // A floating window takes no room in its area.
    const slot = slotOfToolWindow("terminal");
    expect(dockedIn(slot)).toBeNull();
    setToolWindowViewMode("terminal", "dockPinned");
    expect(currentLayout().toolWindows?.terminal.mode).toBeUndefined();
    expect(dockedIn(slot)).toBe("terminal");
    await applyLayout(saved);
    expect(viewModeOf("terminal")).toBe("float");
  });

  it("is neither restored nor saved in a Code With Me guest's window", async () => {
    closeAllTabs();
    let loads = 0;
    let saves = 0;
    registerMock("layout_load", () => {
      loads++;
      return empty({ openFiles: ["app/Saved.php"], activeFile: "app/Saved.php" });
    });
    registerMock("layout_save", () => void saves++);
    // The guest follows the host into the host's file; a restored layout must not switch away.
    adoptWorkspace({ name: "shop", root: "/shop", collab: { host: "Robert", label: "Robert (workstation)" } });
    expect(keepsLayout()).toBe(false);
    await openFile("cwm://app/Http/Controllers/OrderController.php").catch(() => undefined);
    await new Promise((r) => setTimeout(r, 600));
    expect(loads).toBe(0);
    expect(saves).toBe(0);
    expect(editor.tabs.map((t) => t.path)).not.toContain("app/Saved.php");
    closeWorkspace();
    // A normal project restores its layout.
    adoptWorkspace({ name: "shop", root: "/home/robert/shop" });
    expect(keepsLayout()).toBe(true);
    await vi.waitFor(() => expect(editor.tabs.map((t) => t.path)).toContain("app/Saved.php"));
    expect(loads).toBe(1);
    closeWorkspace();
  });
});
