import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { registerMock } from "@/ipc";
import { callWasm, forgeWasm } from "@/wasm";
import { closeAllTabs, editor, notifyBeforeNavigate, openFile, openViewTab, updateCursor } from "@/state/editor";
import { closedTabs, navigateBack, navigateForward, recentFiles } from "./history";

// The real `openFileAt` waits for a mounted CodeMirror view; here it moves the caret in the
// tab state, as the editor's update listener does.
vi.mock("@/editor/views", () => ({
  viewFor: () => undefined,
  scrollStateOf: () => undefined,
  openFileAt: async (path: string, line: number, col = 1) => {
    notifyBeforeNavigate({ path, line });
    await openFile(path);
    updateCursor(path, line, col);
    return true;
  },
}));
const { openFileAt } = await import("@/editor/views");

const caret = () => {
  const tab = editor.tabs.find((t) => t.path === editor.active);
  return [editor.active, tab?.cursor.line, tab?.cursor.col];
};

beforeAll(async () => {
  const wasm = await forgeWasm();
  const bytes = Array.from(new TextEncoder().encode("line\n".repeat(100)));
  registerMock("fs_read_text", () => callWasm(wasm.text_read, { bytes, encoding: null }));
});

describe("navigation history", () => {
  beforeEach(async () => {
    closeAllTabs();
    await Promise.resolve();
  });

  it("forgets the files of the previous project when another project takes the window", async () => {
    openViewTab({ path: "db-console://a/1", name: "a", view: "x" });
    openViewTab({ path: ".env", name: ".env", view: "x" });
    await Promise.resolve();
    expect(recentFiles()).toContain(".env");
    closeAllTabs();
    await Promise.resolve();
    expect(recentFiles()).toEqual([]);
    expect(closedTabs()).toEqual([]);
  });

  it("goes back and forward over jumps inside the same file", async () => {
    await openFile("src/User.php");
    updateCursor("src/User.php", 40, 9); // Ctrl+Click on a method call
    await openFileAt("src/User.php", 12, 21); // its declaration
    expect(caret()).toEqual(["src/User.php", 12, 21]);

    await navigateBack();
    expect(caret()).toEqual(["src/User.php", 40, 9]);
    await navigateForward();
    expect(caret()).toEqual(["src/User.php", 12, 21]);
  });

  it("returns to the caret position in the previous file", async () => {
    await openFile("src/User.php");
    updateCursor("src/User.php", 30, 5);
    await openFileAt("src/Order.php", 7, 3);

    await navigateBack();
    expect(caret()).toEqual(["src/User.php", 30, 5]);
    await navigateForward();
    expect(caret()).toEqual(["src/Order.php", 7, 3]);
  });

  it("drops the forward places after a new jump", async () => {
    await openFile("src/User.php");
    updateCursor("src/User.php", 40, 1);
    await openFileAt("src/User.php", 12, 1);
    await navigateBack();
    await openFileAt("src/User.php", 80, 1);

    await navigateForward();
    expect(caret()).toEqual(["src/User.php", 80, 1]);
    await navigateBack();
    expect(caret()).toEqual(["src/User.php", 40, 1]);
  });
});
