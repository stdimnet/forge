import { describe, expect, it } from "vitest";
import { invoke } from "@/ipc";
import type { LhContent, LhRecentChange, LhRevertResult, LhRevision, LhSelectionHistory } from "@/ipc/localHistory";
import { mockFiles } from "./workspaceMock";
import "./textMock";
import "./localHistoryMock";

const USER = "app/Models/User.php";

describe("local history mock (forge-localhistory through WebAssembly)", () => {
  it("has demo history: saves, a refactoring, a label", async () => {
    const revs = await invoke<LhRevision[]>("local_history_list", { path: USER, dir: false });
    expect(revs.map((r) => r.name)).toEqual([
      "Saved",
      "Refactoring: Rename method spent() to totalSpent()",
      "Before VIP discount",
      "Saved",
      "Saved",
    ]);
    const before = await invoke<LhContent>("local_history_content", { path: USER, rev: revs[1].id });
    expect(before.text).toContain("function spent()");
    const recent = await invoke<LhRecentChange[]>("local_history_recent", { limit: 10 });
    expect(recent.map((r) => r.kind)).toContain("external");
  });

  it("records saves and reverts a file", async () => {
    const original = mockFiles[USER];
    await invoke("fs_write_text", { path: USER, text: `${original}// changed\n` });
    // Recording runs after the write returns.
    await new Promise((r) => setTimeout(r, 50));
    const revs = await invoke<LhRevision[]>("local_history_list", { path: USER, dir: false });
    expect(revs[0].kind).toBe("save");
    expect((await invoke<LhContent>("local_history_content", { path: USER, rev: revs[0].id })).text).toBe(original);
    const done = await invoke<LhRevertResult>("local_history_revert", {
      target: { kind: "file", path: USER, revPath: USER, rev: revs[0].id },
      name: "Revert",
    });
    expect(done.changed).toEqual([USER]);
    expect(mockFiles[USER]).toBe(original);
  });

  it("filters revisions and follows selected lines", async () => {
    const hits = await invoke<LhRevision[]>("local_history_list", { path: USER, dir: false, filter: "function spent" });
    expect(hits.length).toBeGreaterThan(0);
    const text = mockFiles[USER];
    const line = text.split("\n").findIndex((l) => l.includes("function totalSpent")) + 1;
    const sel = await invoke<LhSelectionHistory>("local_history_selection", { path: USER, text, start: line, end: line });
    expect(sel.current).toContain("totalSpent");
    expect(sel.revisions[0].text).toContain("function spent()");
  });

  it("puts labels and creates patches", async () => {
    await invoke("local_history_put_label", { name: "Checkpoint" });
    await invoke("fs_write_file", { path: "notes.txt", contents: "one\n" });
    await new Promise((r) => setTimeout(r, 50));
    const revs = await invoke<LhRevision[]>("local_history_list", { path: "", dir: true });
    const label = revs.find((r) => r.name === "Checkpoint")!;
    expect(label.kind).toBe("label");
    const patch = await invoke<string>("local_history_patch", { path: "", dir: true, revPath: "", rev: label.id });
    expect(patch).toContain("+++ b/notes.txt");
    await expect(invoke("local_history_put_label", { name: " " })).rejects.toThrow("empty");
  });
});
