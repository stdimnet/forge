import { afterEach, describe, expect, it } from "vitest";
import { invoke, registerMock } from "@/ipc";
import { isGuestTabPath, projectPathOf, setGuestRoot, withProjectPaths } from "./projectPath";
import { gitApi } from "@/features/git/api";
import { phpApi } from "@/features/php/api";
import { lspApi } from "./lsp";
import { targetState } from "@/features/git/context";
import { diagnosticsTab } from "@/features/lsp/store";
import { closeAllTabs, openViewTab } from "@/state/editor";

const TAB = "cwm://app/Models/Order.php";
const FILE = "app/Models/Order.php";

afterEach(() => setGuestRoot(null));

describe("projectPathOf", () => {
  it("maps a guest's tab to its project file", () => {
    expect(projectPathOf(TAB)).toBe(FILE);
    expect(isGuestTabPath(TAB)).toBe(true);
  });

  it("leaves other paths alone", () => {
    for (const p of [FILE, "forge:settings", "scratch://notes.txt", "/home/me/x.php", ""]) {
      expect(projectPathOf(p)).toBe(p);
      expect(isGuestTabPath(p)).toBe(false);
    }
  });

  it("does not take the guest's project root for a tab", () => {
    setGuestRoot("cwm://Alice/shop");
    expect(projectPathOf("cwm://Alice/shop")).toBe("cwm://Alice/shop");
    expect(projectPathOf("cwm://Alice/shop/app/x.php")).toBe("cwm://Alice/shop/app/x.php");
    expect(projectPathOf(TAB)).toBe(FILE);
  });
});

describe("withProjectPaths", () => {
  it("maps path arguments, nested ones too, and keeps everything else", () => {
    const args = {
      path: TAB,
      text: "cwm://not a path",
      content: "<?php",
      query: { paths: [TAB, "b.php"], limit: 5 },
      filePath: TAB,
      files: [{ path: TAB, contents: "cwm://text" }],
    };
    expect(withProjectPaths(args)).toEqual({
      path: FILE,
      text: "cwm://not a path",
      content: "<?php",
      query: { paths: [FILE, "b.php"], limit: 5 },
      filePath: FILE,
      files: [{ path: FILE, contents: "cwm://text" }],
    });
    // The caller's arguments are not changed.
    expect(args.path).toBe(TAB);
  });

  it("returns the same object when there is nothing to map", () => {
    const args = { path: FILE, offset: 3, bytes: new Uint8Array([1]) };
    expect(withProjectPaths(args)).toBe(args);
  });
});

describe("IPC calls of a guest's shared tab", () => {
  const seen = (cmd: string) => {
    const calls: Record<string, unknown>[] = [];
    registerMock(cmd, (args) => {
      calls.push(args);
      return null;
    });
    return calls;
  };

  it("send the project path to git, PHP and the language servers", async () => {
    const blame = seen("git_blame");
    const show = seen("git_show");
    const log = seen("git_log");
    const structure = seen("php_file_structure");
    const hover = seen("lsp_hover");
    await gitApi.blame(TAB, "<?php");
    await gitApi.show("HEAD", TAB);
    await gitApi.log({ paths: [TAB] } as Parameters<typeof gitApi.log>[0]);
    await phpApi.fileStructure(TAB, "<?php");
    await lspApi.hover(TAB, { line: 1, character: 2 });
    expect(blame[0]).toEqual({ path: FILE, contents: "<?php" });
    expect(show[0]).toEqual({ rev: "HEAD", path: FILE });
    expect(log[0]).toEqual({ query: { paths: [FILE] } });
    expect(structure[0]).toMatchObject({ path: FILE, content: "<?php" });
    expect(hover[0]).toEqual({ path: FILE, line: 1, character: 2 });
  });

  it("keep the tab path for commands that are not about a file", async () => {
    const echo = seen("test_echo");
    await invoke("test_echo", { name: TAB });
    expect(echo[0]).toEqual({ name: TAB });
  });
});

describe("guest tabs in git and language server state", () => {
  it("finds a shared tab's file in the git status", () => {
    const entries = [{ path: FILE, origPath: null, index: "unmodified", worktree: "untracked", conflict: false }] as Parameters<typeof targetState>[1];
    const s = targetState([{ path: TAB, dir: false }], entries);
    expect(s.file).toBe(TAB);
    expect(s.unversioned).toEqual([FILE]);
    expect(s.fileTracked).toBe(false);
    expect(s.allUnversioned).toBe(true);
  });

  it("shows the host's diagnostics in the open shared tab", () => {
    closeAllTabs();
    expect(diagnosticsTab(FILE)).toBe(FILE);
    openViewTab({ path: TAB, name: "Order.php", view: "text" });
    expect(diagnosticsTab(FILE)).toBe(TAB);
    closeAllTabs();
  });
});
