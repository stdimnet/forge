import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { registerMock } from "@/ipc";
import { callWasm, forgeWasm } from "@/wasm";
import {
  changeLineSeparator,
  closeAllTabs,
  convertEncoding,
  editor,
  isDirty,
  normalizeEol,
  openFile,
  reloadWithEncoding,
  saveFile,
  setTabFormat,
  updateContent,
} from "./editor";
import type { FileFormat } from "@/ipc/text";

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

/** A disk of raw bytes behind the same Rust text handling as the desktop app. */
const disk: Record<string, number[]> = {};
const bytes = (s: string) => Array.from(new TextEncoder().encode(s));
const text = (path: string) => new TextDecoder().decode(new Uint8Array(disk[path]));

beforeAll(async () => {
  const wasm = await forgeWasm();
  registerMock("fs_read_text", ({ path, encoding }) => callWasm(wasm.text_read, { bytes: disk[String(path)], encoding: encoding ?? null }));
  registerMock("fs_write_text", async ({ path, ...rest }) => {
    disk[String(path)] = (await callWasm<{ bytes: number[] }>(wasm.text_write, rest)).bytes;
  });
  registerMock("text_prepare_save", ({ request }) => callWasm(wasm.text_prepare_save, request));
});

const format = (over: Partial<FileFormat> = {}): FileFormat => ({
  indent: { useTabs: false, size: 4, tabSize: 4, continuation: 4 },
  indentSource: "codeStyle",
  indentLabel: "4 spaces",
  configuredIndent: { useTabs: false, size: 4, tabSize: 4, continuation: 4 },
  codeStylePage: null,
  languageName: null,
  lineSeparator: "\n",
  requiredLineSeparator: null,
  charset: null,
  save: { stripTrailingSpaces: "none", ensureNewlineAtEof: false },
  hardWrapAt: 120,
  visualGuides: [],
  editorConfig: false,
  ...over,
});

describe("tabs of CRLF files", () => {
  beforeEach(() => {
    closeAllTabs();
    disk["win.txt"] = bytes("one\r\ntwo\r\n");
  });
  it("opens clean and saves with the original line ending", async () => {
    await openFile("win.txt");
    setTabFormat("win.txt", format());
    const tab = editor.tabs[0];
    expect(tab.content).toBe("one\ntwo\n");
    expect(tab.eol).toBe("\r\n");
    expect(isDirty(tab)).toBe(false);
    updateContent("win.txt", "one\ntwo\nthree\n");
    expect(isDirty(editor.tabs[0])).toBe(true);
    await saveFile("win.txt");
    expect(text("win.txt")).toBe("one\r\ntwo\r\nthree\r\n");
    expect(isDirty(editor.tabs[0])).toBe(false);
  });
  it("converts the line separator from the status bar", async () => {
    await openFile("win.txt");
    setTabFormat("win.txt", format());
    await changeLineSeparator("win.txt", "\n");
    expect(text("win.txt")).toBe("one\ntwo\n");
    expect(editor.tabs[0].eol).toBe("\n");
  });
});

describe("on save", () => {
  beforeEach(() => closeAllTabs());
  it("strips trailing spaces of modified lines and adds the final line break", async () => {
    disk["a.php"] = bytes("<?php  \n$a = 1;");
    await openFile("a.php");
    setTabFormat("a.php", format({ save: { stripTrailingSpaces: "modified", ensureNewlineAtEof: true } }));
    updateContent("a.php", "<?php  \n$a = 2;   ");
    await saveFile("a.php");
    expect(text("a.php")).toBe("<?php  \n$a = 2;\n");
    expect(editor.tabs[0].content).toBe("<?php  \n$a = 2;\n");
    expect(isDirty(editor.tabs[0])).toBe(false);
  });
  it("gives new files the code style's separator", async () => {
    disk["new.txt"] = [];
    await openFile("new.txt");
    setTabFormat("new.txt", format({ lineSeparator: "\r\n" }));
    updateContent("new.txt", "a\nb");
    await saveFile("new.txt");
    expect(text("new.txt")).toBe("a\r\nb");
    expect(editor.tabs[0].eol).toBe("\r\n");
  });
});

describe("encodings", () => {
  beforeEach(() => closeAllTabs());
  it("detects Windows-1252, reloads and converts", async () => {
    disk["legacy.txt"] = [99, 97, 102, 233, 10];
    await openFile("legacy.txt");
    setTabFormat("legacy.txt", format());
    expect(editor.tabs[0].encoding).toBe("windows-1252");
    expect(editor.tabs[0].content).toBe("café\n");
    await convertEncoding("legacy.txt", "UTF-8", false);
    expect(disk["legacy.txt"]).toEqual(bytes("café\n"));
    await reloadWithEncoding("legacy.txt", "ISO-8859-1");
    expect(editor.tabs[0].content).toBe("cafÃ©\n");
  });
  it("keeps the old encoding when a character cannot be converted", async () => {
    disk["u.txt"] = bytes("中\n");
    await openFile("u.txt");
    setTabFormat("u.txt", format());
    await expect(convertEncoding("u.txt", "ISO-8859-1", false)).rejects.toThrow(/ISO-8859-1/);
    expect(editor.tabs[0].encoding).toBe("UTF-8");
  });
});
