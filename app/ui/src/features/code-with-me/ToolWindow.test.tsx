/**
 * The Code With Me tool window's Participants tab shows each connection's path like the session
 * popup: hosts see one for every guest, guests only for the host, plus the relay state.
 */
import { afterEach, describe, expect, it } from "vitest";
import { render } from "solid-js/web";
import { PRESETS, type SessionView } from "@/ipc/collab";
import { applySession } from "./store";
import { CodeWithMeToolWindow } from "./ToolWindow";

const participants: SessionView["participants"] = [
  { id: 0, name: "Robert", role: "host", color: 1, permissions: PRESETS.fullAccess },
  { id: 1, name: "Alex", role: "guest", color: 2, permissions: PRESETS.readOnly },
  { id: 2, name: "Sam", role: "guest", color: 3, permissions: PRESETS.editFiles },
];

const hostSession: SessionView = {
  role: "host",
  canHost: true,
  me: 0,
  participants,
  files: [],
  connections: [{ participant: 1, kind: "direct", remote: "192.168.1.42:41641", rttMs: 4 }],
};

const guestSession: SessionView = {
  role: "guest",
  canHost: true,
  me: 1,
  participants,
  files: [],
  connections: [{ participant: 0, kind: "relay", remote: "https://relay.example.com/", rttMs: 59 }],
  relay: { url: "https://relay.example.com/", connected: true },
};

const row = (id: number) => document.querySelector(`.cwm-tool-window .cwm-participant[data-participant="${id}"]`)!;

let dispose: (() => void) | undefined;

function show(s: SessionView): void {
  applySession(s);
  dispose = render(() => <CodeWithMeToolWindow />, document.body);
}

afterEach(() => {
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
});

describe("Code With Me tool window", () => {
  it("shows the host the path of every guest", () => {
    show(hostSession);
    expect(row(1).querySelector('.cwm-path[data-path="direct"]')?.textContent).toBe("Direct4 ms");
    expect(row(2).querySelector(".cwm-path.pending")?.textContent).toBe("Connecting…");
    expect(row(0).querySelector(".cwm-path")).toBeNull();
    expect(document.querySelector("[data-testid=cwm-relay-status]")).toBeNull();
  });

  it("shows a reconnecting guest as Reconnecting… once", () => {
    show({ ...hostSession, participants: participants.map((p) => (p.id === 1 ? { ...p, reconnecting: true } : p)) });
    expect(row(1).querySelectorAll(".cwm-participant-reconnecting").length).toBe(1);
    expect(row(1).querySelector(".cwm-participant-reconnecting")?.textContent).toBe("Reconnecting…");
    expect(row(1).querySelector('.cwm-path[data-path="direct"]')).toBeNull();
  });

  it("shows a guest only the path to the host, and the relay state", () => {
    show(guestSession);
    expect(row(0).querySelector('.cwm-path[data-path="relay"]')?.textContent).toBe("Relay (relay.example.com)59 ms");
    expect(row(1).querySelector(".cwm-path")).toBeNull();
    expect(row(2).querySelector(".cwm-path")).toBeNull();
    expect(document.querySelector("[data-testid=cwm-relay-status]")?.textContent).toBe("Relay server relay.example.com connected");
  });

  it("keeps the participant rows keyboard reachable", () => {
    show(hostSession);
    expect(row(1).getAttribute("data-nav")).toBe("");
    expect(row(1).querySelector("button.cwm-participant-menu")?.getAttribute("aria-label")).toBe("Actions for Alex");
  });
});
