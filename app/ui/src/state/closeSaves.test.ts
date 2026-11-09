/** Reload from disk and opening files never overwrite what the user has. */
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { registerMock } from "@/ipc";
import { callWasm, forgeWasm } from "@/wasm";
import {
  closeAllTabs,
  editor,
  openFile,
} from "./editor";

const disk: Record<string, number[]> = {};
const bytes = (s: string) => Array.from(new TextEncoder().encode(s));
const text = (path: string) => new TextDecoder().decode(new Uint8Array(disk[path]));
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
    disk[String(path)] = (await callWasm<{ bytes: number[] }>(wasm.text_write, rest)).bytes;
  });
  registerMock("text_prepare_save", ({ request }) => callWasm(wasm.text_prepare_save, request));
});

beforeEach(() => {
  closeAllTabs();
  gate = null;
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
