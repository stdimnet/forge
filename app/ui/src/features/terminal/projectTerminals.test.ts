/**
 * Terminals belong to a project: a running `sleep 100` asks before the window switches
 * (Cancel keeps the project), and each project's tabs come back when it opens again.
 */
import { afterAll, beforeEach, describe, expect, it, vi } from "vitest";

const choose = vi.fn<(opts: { title: string; message: string }) => Promise<string | null>>();
vi.mock("@/features/contextMenu/dialogs", async (orig) => ({
  ...(await orig<object>()),
  askChoice: (opts: { title: string; message: string }) => choose(opts),
}));

import "@/mock";
import { invoke } from "@/ipc";
import { terminalApi } from "@/ipc/terminal";
import { closeWorkspace, openWorkspace, workspace } from "@/state/workspace";
import { activeTab, newTab, renameTab, restoringTabs, setActiveTab, tabs } from "./store";
import "./projectTerminals";

const A = "demo";
const B = "~/Projects/symfony-api";

async function open(path: string): Promise<boolean> {
  const ok = await openWorkspace(path, { askTrust: false });
  await vi.waitFor(() => expect(restoringTabs()).toBe(false));
  return ok;
}

/** Start the tab's (fake) shell as the Terminal view would. */
async function startShell(id: string): Promise<void> {
  await terminalApi.open({ id, cols: 80, rows: 24 });
  await new Promise((r) => setTimeout(r, 40)); // the prompt
}

beforeEach(() => choose.mockReset());
afterAll(() => closeWorkspace());

describe("terminals of a project", () => {
  it("restore project A's tabs, names and directories after B", async () => {
    await open(A);
    const rootA = workspace()!.root;
    const local = newTab();
    const app = newTab("app");
    renameTab(app.id, "Artisan");
    setActiveTab(app.id);
    await startShell(local.id);

    await open(B);
    expect(workspace()?.root).toBe(B);
    expect(tabs()).toEqual([]); // B has no saved layout: the Terminal opens a fresh tab
    const other = newTab();
    expect(other.title).toBe("Local");

    await open(A);
    expect(workspace()?.root).toBe(rootA);
    expect(tabs().map((t) => ({ id: t.id, title: t.title, cwd: t.cwd, renamed: !!t.renamed }))).toEqual([
      { id: local.id, title: "Local", cwd: undefined, renamed: false },
      { id: app.id, title: "Artisan", cwd: "app", renamed: true },
    ]);
    expect(activeTab()).toBe(app.id);

    // The output of the started shell is kept for its tab (user cache dir in Rust).
    const saved = await terminalApi.loadLayout(rootA);
    expect(saved?.tabs.find((t) => t.id === local.id)?.scrollback).toContain("dev@forge");

    // B's own tab comes back in B.
    await open(B);
    expect(tabs().map((t) => t.id)).toEqual([other.id]);
  });

  it("keeps no output when Restore terminal history is off", async () => {
    await invoke("settings_set", { key: "terminal.restoreScrollback", value: false, scope: "user" });
    try {
      await open(A);
      const rootA = workspace()!.root;
      const tab = tabs()[0] ?? newTab();
      await startShell(tab.id);
      await open(B);
      const saved = await terminalApi.loadLayout(rootA);
      expect(saved?.tabs.length).toBeGreaterThan(0);
      expect(saved?.tabs.every((t) => !t.scrollback)).toBe(true);
    } finally {
      await invoke("settings_set", { key: "terminal.restoreScrollback", value: true, scope: "user" });
    }
  });

  it("asks before ending a running sleep 100; Cancel keeps the project", async () => {
    await open(A);
    const rootA = workspace()!.root;
    const tab = tabs()[0] ?? newTab();
    await startShell(tab.id);
    await terminalApi.write(tab.id, "sleep 100\r");
    expect(await terminalApi.running()).toEqual([{ id: tab.id, process: "sleep" }]);

    choose.mockResolvedValueOnce("cancel");
    expect(await openWorkspace(B, { askTrust: false })).toBe(false);
    expect(choose).toHaveBeenCalledTimes(1);
    expect(choose.mock.calls[0][0].message).toContain("sleep");
    expect(workspace()?.root).toBe(rootA);
    expect(tabs().map((t) => t.id)).toContain(tab.id);
    expect(await terminalApi.running()).toHaveLength(1); // still running

    choose.mockResolvedValueOnce("terminate");
    expect(await open(B)).toBe(true);
    expect(workspace()?.root).toBe(B);
    expect(await terminalApi.running()).toEqual([]);
  });
});
