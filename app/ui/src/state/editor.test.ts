import { beforeEach, describe, expect, it } from "vitest";
import { registerMock } from "@/ipc";
import {
  activeGroupId,
  closeAllTabs,
  closeAllTabsIn,
  closeTab,
  closeTabIn,
  editor,
  editorGroups,
  isDirty,
  isTabPinned,
  normalizeEol,
  openFile,
  saveFile,
  splitEditor,
  togglePinTab,
  updateContent,
} from "./editor";

describe("normalizeEol", () => {
  it("keeps LF files as they are", () => {
    expect(normalizeEol("a\nb\n")).toEqual({ text: "a\nb\n", eol: "\n" });
  });
  it("normalizes CRLF files and remembers the ending", () => {
    expect(normalizeEol("a\r\nb\r\n")).toEqual({ text: "a\nb\n", eol: "\r\n" });
  });
  it("uses the dominant ending of mixed files", () => {
    expect(normalizeEol("a\r\nb\r\nc\n").eol).toBe("\r\n");
    expect(normalizeEol("a\nb\nc\r\n").eol).toBe("\n");
  });
});

describe("tabs of CRLF files", () => {
  const disk: Record<string, string> = { "win.txt": "one\r\ntwo\r\n" };
  beforeEach(() => {
    closeAllTabs();
    registerMock("fs_read_file", ({ path }) => disk[String(path)]);
    registerMock("fs_write_file", ({ path, contents }) => {
      disk[String(path)] = String(contents);
    });
  });
  it("opens clean and saves with the original line ending", async () => {
    await openFile("win.txt");
    const tab = editor.tabs[0];
    expect(tab.content).toBe("one\ntwo\n");
    expect(isDirty(tab)).toBe(false);
    updateContent("win.txt", "one\ntwo\nthree\n");
    expect(isDirty(editor.tabs[0])).toBe(true);
    await saveFile("win.txt");
    expect(disk["win.txt"]).toBe("one\r\ntwo\r\nthree\r\n");
    expect(isDirty(editor.tabs[0])).toBe(false);
  });
});

describe("split editors", () => {
  const disk: Record<string, string> = { "a.php": "A", "b.php": "B" };
  beforeEach(() => {
    closeAllTabs();
    registerMock("fs_read_file", ({ path }) => disk[String(path)]);
  });

  it("shares one document between groups and closes it with the last group", async () => {
    await openFile("a.php");
    await openFile("b.php");
    splitEditor("vertical");
    expect(editorGroups()).toHaveLength(2);
    expect(editor.tabs.map((t) => t.path)).toEqual(["a.php", "b.php"]);
    expect(editor.active).toBe("b.php");
    const [first, second] = editorGroups().map((g) => g.id);
    closeTabIn(first, "b.php");
    // Still shown by the second group: the document stays open.
    expect(editor.tabs.map((t) => t.path)).toEqual(["a.php", "b.php"]);
    closeTabIn(second, "b.php");
    expect(editorGroups()).toHaveLength(1);
    expect(editor.tabs.map((t) => t.path)).toEqual(["a.php"]);
    expect(editor.active).toBe("a.php");
  });

  it("keeps pinned tabs on Close All and closes files everywhere with closeTab", async () => {
    await openFile("a.php");
    await openFile("b.php");
    togglePinTab("a.php");
    expect(isTabPinned("a.php")).toBe(true);
    closeAllTabsIn(activeGroupId());
    expect(editor.tabs.map((t) => t.path)).toEqual(["a.php"]);
    splitEditor("horizontal");
    closeTab("a.php");
    expect(editor.tabs).toHaveLength(0);
    expect(editorGroups()).toHaveLength(1);
  });
});
