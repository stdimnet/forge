/**
 * Switching or closing the project, and closing the window, never lose unsaved edits: they
 * are saved in `beforeClose`; a file that cannot be written keeps the project open unless the
 * user discards it.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import "@/mock";
import { registerMock } from "@/ipc";
import { closeAllTabs, editor, isDirty, openFile, updateContent } from "@/state/editor";
import { openWorkspace, requestCloseWorkspace, workspace } from "@/state/workspace";

const answers: boolean[] = [];
const asked: string[] = [];
vi.mock("@/features/contextMenu/dialogs", () => ({
  askConfirm: async (req: { message: string }) => {
    asked.push(req.message);
    return answers.shift() ?? false;
  },
  askConfirmWithCheckbox: async () => ({ ok: true, checked: false }),
}));

const { saveBeforeLeaving } = await import("./unsaved");

const FILE = "app/Models/User.php";
const writes: string[] = [];
const failing = new Set<string>();
// Record which project each write goes to; fail the read-only files.
registerMock("fs_write_text", async (args) => {
  const path = String((args as { path: string }).path);
  if (failing.has(path)) throw new Error("permission denied");
  writes.push(`${workspace()?.root}:${path}`);
});

async function editInDemo(): Promise<string> {
  await openWorkspace("demo", { askTrust: false });
  await openFile(FILE);
  const edited = `${editor.tabs.find((t) => t.path === FILE)!.content}// unsaved\n`;
  updateContent(FILE, edited);
  return edited;
}

beforeEach(async () => {
  writes.length = 0;
  asked.length = 0;
  answers.length = 0;
  failing.clear();
  closeAllTabs();
});

describe("unsaved edits when the project changes", () => {
  it("are saved into the old project before another project opens (This Window)", async () => {
    const root = (await editInDemo(), workspace()!.root);
    await openWorkspace("~/Projects/symfony-api", { askTrust: false });
    expect(workspace()?.root).toBe("~/Projects/symfony-api");
    expect(writes).toEqual([`${root}:${FILE}`]);
    expect(asked).toEqual([]);
  });

  it("are saved by File | Close Project", async () => {
    const root = (await editInDemo(), workspace()!.root);
    expect(await requestCloseWorkspace()).toBe(true);
    expect(writes).toEqual([`${root}:${FILE}`]);
    expect(workspace()).toBeNull();
  });

  it("keep the project open when they cannot be saved, unless the user discards them", async () => {
    const edited = await editInDemo();
    failing.add(FILE);
    answers.push(false);
    expect(await requestCloseWorkspace()).toBe(false);
    expect(asked[0]).toContain(FILE);
    expect(asked[0]).toContain("permission denied");
    expect(workspace()).not.toBeNull();
    const tab = editor.tabs.find((t) => t.path === FILE)!;
    expect(tab.content).toBe(edited);
    expect(isDirty(tab)).toBe(true);
    // Switching is refused the same way.
    answers.push(false);
    expect(await openWorkspace("~/Projects/symfony-api", { askTrust: false })).toBe(false);
    expect(workspace()?.root).not.toBe("~/Projects/symfony-api");
    // "Discard and Close".
    answers.push(true);
    expect(await requestCloseWorkspace()).toBe(true);
    expect(workspace()).toBeNull();
  });
});

describe("unsaved edits when the window closes", () => {
  it("are saved; a file that cannot be written keeps the window open", async () => {
    const root = (await editInDemo(), workspace()!.root);
    expect(await saveBeforeLeaving("window")).toBe(true);
    expect(writes).toEqual([`${root}:${FILE}`]);
    updateContent(FILE, "<?php // again\n");
    failing.add(FILE);
    expect(await saveBeforeLeaving("window")).toBe(false);
    expect(asked[0]).toContain("Close the window anyway");
  });
});
