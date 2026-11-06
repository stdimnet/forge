/**
 * A window that had its own project open joins a session: what the Project, Problems, Find
 * and Inspection Results windows and the Safe Mode banner showed of that project is dropped.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { createEffect, createRoot } from "solid-js";
import { adoptWorkspace, closeWorkspace, onProjectSwitch, workspace } from "@/state/workspace";
import { projectPathOf } from "@/ipc/projectPath";
import { projectTree, revealPath } from "@/features/project/ProjectView";
import { diagnostics, publishDiagnostics } from "@/features/lsp/store";
import { findTabs, showSearchTab, searchTabOpen } from "@/features/codeintel/usages";
import { findResults, setFindResults } from "@/features/find/window";
import { canTrustProject, refreshTrust, trustStatus } from "@/features/trust/store";
import { trustApi, type TrustStatus } from "@/ipc/trust";
import { balloons, showBalloon } from "@/keymap";
import "@/features/notifications";

const local = { name: "legacy-crm", root: "/home/me/Downloads/legacy-crm" };
const guest = { name: "shop", root: "cwm://Robert/shop", collab: { host: "Robert", label: "Robert (workstation)" } };

const untrusted: TrustStatus = {
  project: local.root,
  display: local.root,
  trusted: false,
  safeMode: true,
  remote: false,
  guest: false,
};
const guestStatus: TrustStatus = { project: guest.root, display: guest.root, trusted: false, safeMode: true, remote: true, guest: true };

afterEach(() => {
  closeWorkspace();
  vi.restoreAllMocks();
});

describe("joining a session from a window with a project", () => {
  it("tells the features only when the project really changes", () => {
    const reset = vi.fn();
    const stop = onProjectSwitch(reset);
    adoptWorkspace(local); // from the Welcome screen: nothing to drop
    expect(reset).not.toHaveBeenCalled();
    adoptWorkspace({ ...local }); // the same project again
    expect(reset).not.toHaveBeenCalled();
    adoptWorkspace(guest);
    expect(reset).toHaveBeenCalledTimes(1);
    closeWorkspace(); // leaving the session
    expect(reset).toHaveBeenCalledTimes(2);
    stop();
  });

  it("does not set the host's project again with every session status", () => {
    const seen: unknown[] = [];
    const dispose = createRoot((d) => {
      createEffect(() => seen.push(workspace()));
      return d;
    });
    adoptWorkspace(guest);
    adoptWorkspace({ ...guest, collab: { ...guest.collab } }); // the next status update
    expect(seen).toEqual([null, guest]);
    // The guest's root is not one of its `cwm://` tabs.
    expect(projectPathOf(guest.root)).toBe(guest.root);
    expect(projectPathOf("cwm://app/Models/Order.php")).toBe("app/Models/Order.php");
    closeWorkspace();
    expect(projectPathOf(guest.root)).toBe("Robert/shop");
    dispose();
  });

  it("starts the Project tree of the host's project collapsed and unselected", () => {
    adoptWorkspace(local);
    revealPath("app/Models");
    projectTree.setSelected("app/Models/User.php");
    expect(projectTree.expanded().has("app/Models")).toBe(true);
    adoptWorkspace(guest);
    expect(projectTree.expanded().size).toBe(0);
    expect(projectTree.selected()).toBeNull();
  });

  it("drops the local project's problems and search results", () => {
    adoptWorkspace(local);
    publishDiagnostics("app/Models/User.php", [
      { range: { start: { line: 1, character: 0 }, end: { line: 1, character: 3 } }, severity: "error", message: "Undefined" },
    ]);
    showSearchTab();
    setFindResults({ ...findResults(), searched: true, matchCount: 3 });
    expect(diagnostics["app/Models/User.php"]).toHaveLength(1);
    expect(searchTabOpen()).toBe(true);

    adoptWorkspace(guest);
    expect(diagnostics["app/Models/User.php"]).toBeUndefined();
    expect(searchTabOpen()).toBe(false);
    expect(findTabs()).toEqual([]);
    expect(findResults().searched).toBe(false);
  });

  it("closes the local project's balloons", () => {
    adoptWorkspace(local);
    showBalloon("Database found in .env", "MySQL · crm@localhost", 0);
    expect(balloons().map((b) => b.text)).toContain("Database found in .env");
    adoptWorkspace(guest);
    expect(balloons()).toEqual([]);
  });

  it("hides the local project's Safe Mode banner; a guest has nothing to trust", async () => {
    const status = vi.spyOn(trustApi, "status").mockResolvedValue(untrusted);
    adoptWorkspace(local);
    await refreshTrust();
    expect(canTrustProject()).toBe(true);

    // The local project's answer is still on its way when the window joins.
    let answerLocal!: (s: TrustStatus) => void;
    status.mockReturnValueOnce(new Promise((r) => (answerLocal = r)));
    const late = refreshTrust();
    status.mockResolvedValue(guestStatus);
    adoptWorkspace(guest);
    expect(canTrustProject()).toBe(false);
    await refreshTrust();
    answerLocal(untrusted);
    await late;
    expect(trustStatus()?.guest).toBe(true);
    expect(trustStatus()?.safeMode).toBe(true);
    expect(canTrustProject()).toBe(false);
  });
});
