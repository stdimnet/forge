import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { emitMock, registerMock } from "@/ipc";
import { PRESETS, sharedRunId, type Permissions, type SessionView, type SharedRun, type SharedTerminal } from "@/ipc/collab";
import type { RunEvent } from "@/ipc/run";
import { closeTab, newTabBlocked, tabs, terminalTabActions } from "@/features/terminal/store";
import { launch, sessions, stopSession } from "@/features/run/runStore";
import { applySession } from "./store";
import { sharedTerminalTab, terminalBlockedReason, trackTerminalsAndRuns } from "./terminalsRuns";

const calls: { cmd: string; args: Record<string, unknown> }[] = [];
const record = (cmd: string, answer: (args: Record<string, unknown>) => unknown = () => undefined) =>
  registerMock(cmd, (args) => {
    calls.push({ cmd, args });
    return answer(args);
  });

const RUN_EVENTS: RunEvent[] = [
  { type: "started", commandLine: "phpunit" } as RunEvent,
  { type: "console", lines: [{ segments: [{ text: "OK (3 tests)" }] }] } as RunEvent,
  { type: "exited", code: 0, signal: null },
];

beforeAll(() => {
  record("collab_run_attach", () => RUN_EVENTS);
  record("collab_run_start");
  record("collab_run_stop");
  record("collab_run_detach");
  record("collab_run_answer");
  trackTerminalsAndRuns();
});

afterEach(() => {
  applySession({ role: "none", canHost: false, participants: [], files: [] });
  for (const t of tabs()) closeTab(t.id);
  calls.length = 0;
});

const host = { id: 0, name: "Robert", role: "host" as const, color: 1, permissions: PRESETS.fullAccess };

function guest(permissions: Permissions, terminals: SharedTerminal[] = [], runs: SharedRun[] = []): SessionView {
  return {
    role: "guest",
    canHost: false,
    me: 2,
    participants: [host, { id: 2, name: "Alex", role: "guest", color: 2, permissions }],
    files: [],
    permissions,
    terminals,
    runs,
  };
}

const local: SharedTerminal = { id: 7, title: "Local", owner: 0, cols: 120, rows: 30, running: true };
const mine: SharedTerminal = { id: 8, title: "Alex: Local", owner: 2, cols: 80, rows: 24, running: true };

describe("a guest's terminals", () => {
  it("show the host's shared terminals, read-only without Full access", () => {
    applySession(guest(PRESETS.readOnly, [local, mine]));
    const tab = tabs().find((t) => t.id === sharedTerminalTab(7));
    expect(tab?.source).toEqual({ kind: "collab", terminal: 7, readOnly: true, cols: 120, rows: 30 });
    // Its own terminal is its own tab already.
    expect(tabs().some((t) => t.id === sharedTerminalTab(8))).toBe(false);
    expect(newTabBlocked()).toContain("watch");

    applySession(guest(PRESETS.fullAccess, [{ ...local, cols: 100 }]));
    const again = tabs().find((t) => t.id === sharedTerminalTab(7));
    expect(again?.source).toMatchObject({ readOnly: false, cols: 100 });
    expect(newTabBlocked()).toBeNull();

    applySession(guest(PRESETS.fullAccess, []));
    expect(tabs().some((t) => t.id === sharedTerminalTab(7))).toBe(false);
  });

  it("explain why no terminal can be opened", () => {
    expect(terminalBlockedReason(undefined)).toBeNull();
    expect(terminalBlockedReason(PRESETS.fullAccess)).toBeNull();
    expect(terminalBlockedReason(PRESETS.editFiles)).toContain("does not share");
  });
});

describe("a guest's runs", () => {
  it("show the host's runs with what happened so far", async () => {
    applySession(guest(PRESETS.readOnly, [], [{ id: 3, title: "UserTest", kind: "phpunit", startedBy: 0, running: true }]));
    await vi.waitFor(() => expect(sessions().find((s) => s.shared === 3)?.status).toBe("finished"));
    const s = sessions().find((x) => x.shared === 3)!;
    expect(s.runId).toBe(sharedRunId(3));
    expect(s.name).toBe("UserTest");
    expect(calls.find((c) => c.cmd === "collab_run_attach")?.args).toEqual({ run: 3 });
  });

  it("run, stop and rerun on the host", async () => {
    applySession(guest(PRESETS.fullAccess, [], [{ id: 4, title: "Shop tests", startedBy: 2, running: true }]));
    await vi.waitFor(() => expect(sessions().some((s) => s.shared === 4)).toBe(true));
    await launch({ name: "UserTest", type: "phpunit" });
    expect(calls.find((c) => c.cmd === "collab_run_start")?.args).toEqual({ configuration: "UserTest" });
    // A running shared run is stopped on the host.
    applySession(guest(PRESETS.fullAccess, [], [{ id: 5, title: "Long", startedBy: 2, running: true }]));
    registerMock("collab_run_attach", () => [RUN_EVENTS[0]]);
    await vi.waitFor(() => expect(sessions().find((s) => s.shared === 5)?.status).toBe("running"));
    await stopSession(sessions().find((s) => s.shared === 5)!.id);
    expect(calls.find((c) => c.cmd === "collab_run_stop")?.args).toEqual({ run: 5 });
  });

  it("are not started without Run Full access", async () => {
    applySession(guest(PRESETS.readOnly));
    await launch({ name: "UserTest", type: "phpunit" });
    expect(calls.some((c) => c.cmd === "collab_run_start")).toBe(false);
  });
});

describe("the host", () => {
  const hosting = (terminals: SharedTerminal[] = []): SessionView => ({
    role: "host",
    canHost: true,
    me: 0,
    participants: [host],
    files: [],
    permissions: PRESETS.fullAccess,
    terminals,
  });

  it("marks shared terminal tabs and offers Share with Guests", async () => {
    const { newTab } = await import("@/features/terminal/store");
    const tab = newTab();
    applySession(hosting());
    expect(terminalTabActions(tabs().find((t) => t.id === tab.id)!)).toEqual([expect.objectContaining({ label: "Share with Guests", checked: false })]);
    applySession(hosting([{ ...local, key: tab.id }]));
    const shared = tabs().find((t) => t.id === tab.id)!;
    expect(shared.shared).toBe(true);
    expect(terminalTabActions(shared)[0].checked).toBe(true);
    applySession({ role: "none", canHost: true, participants: [], files: [] });
    expect(tabs().find((t) => t.id === tab.id)?.shared).toBe(false);
    expect(terminalTabActions(tabs().find((t) => t.id === tab.id)!)).toEqual([]);
  });

  it("answers a guest's request for an unknown configuration", async () => {
    applySession(hosting());
    emitMock("collab://run-request", { request: 9, configuration: "Nope", guest: "Alex" });
    await vi.waitFor(() => expect(calls.find((c) => c.cmd === "collab_run_answer")?.args).toEqual({ request: 9, error: "There is no run configuration named 'Nope'." }));
  });

  it("shows a terminal a guest opened on this machine", async () => {
    applySession(hosting());
    emitMock("collab://terminal-opened", { id: "cwm-x-terminal-1", title: "Alex: Local" });
    await vi.waitFor(() => expect(tabs().find((t) => t.id === "cwm-x-terminal-1")?.source).toEqual({ kind: "existing" }));
  });
});
