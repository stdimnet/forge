import { beforeEach, describe, expect, it } from "vitest";
import { registerMock } from "@/ipc";
import { closeAllTabs, editor, isDirty, normalizeEol, openFile, saveFile, updateContent } from "./editor";

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
