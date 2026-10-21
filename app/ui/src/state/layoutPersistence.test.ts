import { beforeAll, describe, expect, it } from "vitest";
import { registerMock } from "@/ipc";
import { registerToolWindow } from "@/features/registry";
import type { ProjectLayout } from "@/ipc/projectState";
import { closeAllTabs, editor, openFile } from "./editor";
import { layout, setToolWindowTab, showToolWindow } from "./layout";
import { applyLayout, currentLayout } from "./layoutPersistence";

const Empty = () => null;
const Icon = () => null;

beforeAll(() => {
  registerMock("fs_read_text", ({ path }) => ({ text: `// ${path}\n`, eol: "\n", encoding: "UTF-8", bom: false, malformed: false }));
  registerToolWindow({ id: "project", title: "Project", icon: Icon, anchor: "left", order: 0, component: Empty });
  registerToolWindow({ id: "problems", title: "Problems", icon: Icon, anchor: "bottom", order: 50, component: Empty });
});

describe("project layout", () => {
  it("captures tool windows, sizes, tool window tabs and open files", async () => {
    closeAllTabs();
    showToolWindow("bottom", "problems");
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
});
