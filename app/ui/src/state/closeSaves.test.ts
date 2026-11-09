/**
 * Closing editor tabs never loses edits (PHPStorm saves a modified file when its last tab
 * closes): close one tab, Close Others, Close All, the tab limit; a failed save reopens the
 * tab. Reload from disk and layout restore never overwrite what the user has.
 */
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { registerMock } from "@/ipc";
import { callWasm, forgeWasm } from "@/wasm";
import {
  activeGroupId,
  closeAllTabs,
  closeAllTabsIn,
  closeOtherTabs,
  closeTabIn,
  editor,
  isDirty,
  onCloseSaveFailed,
  onTabClosed,
  openFile,
  reloadFile,
  updateContent,
} from "./editor";

const disk: Record<string, number[]> = {};
const bytes = (s: string) => Array.from(new TextEncoder().encode(s));
const text = (path: string) => new TextDecoder().decode(new Uint8Array(disk[path]));
/** Paths whose writes fail (read-only files). */
const readOnly = new Set<string>();
/** Reads that wait until released (a slow disk). */
let gate: Promise<void> | null = null;

beforeAll(async () => {
  const wasm = await forgeWasm();
  registerMock("fs_read_text", async ({ path, encoding }) => {
    const bytesNow = disk[String(path)];
    if (gate) await gate;
    return callWasm(wasm.text_read, { bytes: bytesNow, encoding: encoding ?? null });
  });
  registerMock("fs_write_text", async ({ path, ...rest }) => {
    if (readOnly.has(String(path))) throw new Error("permission denied");
    disk[String(path)] = (await callWasm<{ bytes: number[] }>(wasm.text_write, rest)).bytes;
  });
  registerMock("text_prepare_save", ({ request }) => callWasm(wasm.text_prepare_save, request));
});

beforeEach(() => {
  closeAllTabs();
  readOnly.clear();
  gate = null;
});

async function openModified(path: string, edit = `edited ${path}\n`): Promise<void> {
  disk[path] = bytes(`original ${path}\n`);
  await openFile(path);
  updateContent(path, edit);
}

describe("closing a modified tab", () => {
  it("saves it first (close button, Ctrl+F4, middle click)", async () => {
    await openModified("a.php");
    const closed: string[] = [];
    const stop = onTabClosed((t) => void closed.push(t.path));
    closeTabIn(activeGroupId(), "a.php");
    await vi.waitFor(() => expect(editor.tabs).toHaveLength(0));
    stop();
    expect(text("a.php")).toBe("edited a.php\n");
    expect(closed).toEqual(["a.php"]);
  });

  it("saves every tab that Close Others and Close All close", async () => {
    await openModified("a.php");
    await openModified("b.php");
    await openModified("c.php");
    closeOtherTabs(activeGroupId(), "c.php");
    await vi.waitFor(() => expect(editor.tabs.map((t) => t.path)).toEqual(["c.php"]));
    expect(text("a.php")).toBe("edited a.php\n");
    expect(text("b.php")).toBe("edited b.php\n");
    closeAllTabsIn(activeGroupId());
    await vi.waitFor(() => expect(editor.tabs).toHaveLength(0));
    expect(text("c.php")).toBe("edited c.php\n");
  });

  it("saves a modified tab the tab limit closes", async () => {
    for (let i = 0; i < 10; i++) await openModified(`f${i}.php`);
    await openFile("f0.php");
    disk["new.php"] = bytes("new\n");
    await openFile("new.php");
    // The least recently used one (f1) made room, its edits are on disk.
    await vi.waitFor(() => expect(editor.tabs.some((t) => t.path === "f1.php")).toBe(false));
    expect(text("f1.php")).toBe("edited f1.php\n");
  });

  it("reopens the tab when the file cannot be written", async () => {
    await openModified("locked.php");
    readOnly.add("locked.php");
    const failures: string[] = [];
    const stop = onCloseSaveFailed((path) => void failures.push(path));
    closeTabIn(activeGroupId(), "locked.php");
    await vi.waitFor(() => expect(failures).toEqual(["locked.php"]));
    stop();
    const tab = editor.tabs.find((t) => t.path === "locked.php")!;
    expect(tab.content).toBe("edited locked.php\n");
    expect(isDirty(tab)).toBe(true);
    expect(editor.layout.groups.some((g) => g.tabs.includes("locked.php"))).toBe(true);
  });
});

describe("reload from disk", () => {
  it("keeps text typed while the file was read", async () => {
    disk["r.php"] = bytes("one\n");
    await openFile("r.php");
    let release!: () => void;
    gate = new Promise((r) => (release = r));
    disk["r.php"] = bytes("changed on disk\n");
    const reload = reloadFile("r.php");
    updateContent("r.php", "one\ntyped\n");
    release();
    await reload;
    expect(editor.tabs[0].content).toBe("one\ntyped\n");
  });
});

describe("opening files while the project changes", () => {
  it("drops a file read for the previous project", async () => {
    disk["app/A.php"] = bytes("project A\n");
    let release!: () => void;
    gate = new Promise((r) => (release = r));
    const opening = openFile("app/A.php");
    // Another project takes the window while the read is on its way.
    closeAllTabs();
    release();
    await opening;
    expect(editor.tabs).toEqual([]);
  });
});
