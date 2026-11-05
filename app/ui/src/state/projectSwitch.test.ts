/**
 * Switching between two mock projects in this window (This Window): nothing of project A
 * remains in project B - branch, file statuses, diagnostics - and a folder without git
 * shows no branch.
 */
import { describe, expect, it, vi } from "vitest";
import "@/mock";
import { closeWorkspace, lifecycleProject, openWorkspace, workspace } from "./workspace";
import { onProject } from "./projectLifecycle";
import { branchLabel, fileStatuses } from "./vcs";
import { startGitStore, status } from "@/features/git/store";
import { diagnostics, publishDiagnostics } from "@/features/lsp/store";

startGitStore();

const settle = () => new Promise((r) => setTimeout(r, 0));

describe("switching projects in this window", () => {
  it("drops project A's state and loads project B's", async () => {
    const phases: string[] = [];
    const stops = (["closing", "opened", "ready"] as const).map((phase) => onProject(phase, (p) => void phases.push(`${phase} ${p.name}`)));

    await openWorkspace("demo", { askTrust: false });
    const demo = workspace()!;
    await vi.waitFor(() => expect(branchLabel()).not.toBe(""));
    expect(branchLabel()).not.toBe("feature/api-v2");
    await vi.waitFor(() => expect(Object.keys(fileStatuses()).length).toBeGreaterThan(0));
    publishDiagnostics("app/Models/User.php", [{ range: { start: { line: 1, character: 0 }, end: { line: 1, character: 3 } }, severity: "error", message: "Undefined" }]);

    await openWorkspace("~/Projects/symfony-api", { askTrust: false });
    expect(workspace()?.root).toBe("~/Projects/symfony-api");
    // At once, before the new repository answered: nothing of the old one.
    expect(diagnostics["app/Models/User.php"]).toBeUndefined();
    await vi.waitFor(() => expect(branchLabel()).toBe("feature/api-v2"));
    expect(fileStatuses()).toEqual({ "routes/api.php": "modified", "app/Models/Order.php": "untracked" });
    // The Rust event added what the lifecycle knows about the project.
    expect(lifecycleProject()?.vcsRoot).toBe("~/Projects/symfony-api");

    // A folder without git shows no branch, not symfony-api's.
    await openWorkspace("~/.config/nvim", { askTrust: false });
    await vi.waitFor(() => expect(status()).toBeNull());
    expect(branchLabel()).toBe("");
    expect(fileStatuses()).toEqual({});

    // Every project got `opened` and `closing` once, in that order, whether the IPC result
    // or the event came first.
    await new Promise((r) => setTimeout(r, 350));
    expect(phases.filter((p) => !p.startsWith("ready"))).toEqual([
      `opened ${demo.name}`,
      `closing ${demo.name}`,
      "opened symfony-api",
      "closing symfony-api",
      "opened dotfiles.nvim",
    ]);
    // `ready` only while that project is open.
    expect(phases).toContain("ready dotfiles.nvim");
    for (const name of [demo.name, "symfony-api"]) {
      const ready = phases.indexOf(`ready ${name}`);
      if (ready >= 0) expect(ready).toBeLessThan(phases.indexOf(`closing ${name}`));
    }

    closeWorkspace();
    await settle();
    expect(phases.at(-1)).toBe("closing dotfiles.nvim");
    stops.forEach((s) => s());
  });
});
