import { afterEach, describe, expect, it } from "vitest";
import { registerMock } from "@/ipc";
import type { FileEntry } from "@/ipc/workspace";
import type { EditPlan } from "@/ipc/codeintel";
import { moveDialog } from "@/features/codeintel/moveRefactorings";
import { canPasteFiles, copyFiles, fileClipboard, pasteFiles, pastePlan, resetFileClipboard, sameClipboardText } from "./fileClipboard";

describe("pastePlan", () => {
  it("duplicates one copied entry pasted into its own folder", () => {
    expect(pastePlan(["app/User.php"], false, "app")).toEqual({ kind: "duplicate", path: "app/User.php" });
    expect(pastePlan(["README.md"], false, "")).toEqual({ kind: "duplicate", path: "README.md" });
  });

  it("copies into another folder", () => {
    expect(pastePlan(["app/User.php", "app/Post.php"], false, "lib")).toEqual({ kind: "copy", paths: ["app/User.php", "app/Post.php"], dir: "lib" });
    expect(pastePlan(["app/User.php"], false, "")).toEqual({ kind: "copy", paths: ["app/User.php"], dir: "" });
  });

  it("moves cut entries that are not in the folder yet", () => {
    expect(pastePlan(["app/User.php", "lib/Post.php"], true, "lib")).toEqual({ kind: "move", paths: ["app/User.php"], dir: "lib" });
    expect(pastePlan(["lib/Post.php"], true, "lib")).toEqual({ kind: "none" });
  });

  it("pastes a folder that is still selected next to itself", () => {
    expect(pastePlan(["app/Models"], false, "app/Models")).toEqual({ kind: "duplicate", path: "app/Models" });
    expect(pastePlan(["app/Models"], true, "app/Models")).toEqual({ kind: "none" });
  });

  it("refuses to paste a folder into a folder inside it", () => {
    expect(pastePlan(["app"], true, "app/Models")).toEqual({ kind: "intoItself", path: "app" });
    expect(pastePlan(["app"], false, "application")).toEqual({ kind: "copy", paths: ["app"], dir: "application" });
  });

  it("does nothing without entries", () => {
    expect(pastePlan([], false, "app")).toEqual({ kind: "none" });
  });
});

describe("sameClipboardText", () => {
  it("ignores the line endings the system clipboard may convert", () => {
    expect(sameClipboardText("/p/a\r\n/p/b", "/p/a\n/p/b")).toBe(true);
    expect(sameClipboardText("/p/a", "/p/b")).toBe(false);
  });
});

describe("Paste in the Project view", () => {
  const tree: Record<string, FileEntry[]> = {
    "": [
      { name: "app", path: "app", kind: "dir" },
      { name: "notes", path: "notes", kind: "dir" },
    ],
    app: [
      { name: "a.txt", path: "app/a.txt", kind: "file" },
      { name: "b.txt", path: "app/b.txt", kind: "file" },
    ],
    notes: [],
  };
  const tick = () => new Promise((r) => setTimeout(r));
  async function dialog() {
    for (let i = 0; i < 50 && !moveDialog(); i++) await tick();
    return moveDialog()!;
  }
  function backend(): string[] {
    const calls: string[] = [];
    registerMock("fs_list_dir", ({ path }) => tree[path as string] ?? []);
    registerMock("fs_copy", ({ from, to }) => void calls.push(`copy ${from} -> ${to}`));
    registerMock("code_plan_edit", (): EditPlan => ({ editors: [], backend: { changes: [] }, saveFirst: [], reopen: [], close: [], fileCount: 0 }));
    return calls;
  }
  afterEach(() => resetFileClipboard());

  it("copies several files into another folder, keeping their names", async () => {
    const calls = backend();
    await copyFiles(["app/a.txt", "app/b.txt"], false);
    expect(canPasteFiles()).toBe(true);
    const running = pasteFiles("notes");
    const d = await dialog();
    if (d.kind !== "copyFiles") throw new Error(d.kind);
    expect(d.req).toMatchObject({ what: "2 items", name: null, directory: "notes" });
    d.req.done({ name: null, directory: "notes" });
    await running;
    expect(calls).toEqual(["copy app/a.txt -> notes/a.txt", "copy app/b.txt -> notes/b.txt"]);
    // Copied files can be pasted again.
    expect(fileClipboard()?.paths).toEqual(["app/a.txt", "app/b.txt"]);
  });

  it("asks for a new name when a file is pasted into its own folder", async () => {
    const calls = backend();
    await copyFiles(["app/a.txt"], false);
    const running = pasteFiles("app");
    const d = await dialog();
    if (d.kind !== "copyFiles") throw new Error(d.kind);
    expect(d.req).toMatchObject({ name: "a.txt", directory: "app" });
    d.req.done({ name: "a copy.txt", directory: "app" });
    await running;
    expect(calls).toEqual(["copy app/a.txt -> app/a copy.txt"]);
  });
});
