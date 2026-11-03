import { describe, expect, it } from "vitest";
import type { SessionView } from "@/ipc/collab";
import { pathLabel, pathTitle, relayHost, relayProblem, relaySummary, statusBarText, statusBarTitle } from "./connection";

const perms = { files: "edit", terminal: "none", run: "none", vcs: "readOnly" } as const;
const person = (id: number, name: string, role: "host" | "guest") => ({ id, name, role, color: id + 1, permissions: perms });

const host = (over: Partial<SessionView> = {}): SessionView => ({
  role: "host",
  canHost: true,
  participants: [person(0, "Robert", "host"), person(1, "Alex", "guest"), person(2, "Sam", "guest")],
  files: [],
  ...over,
});

describe("connection paths", () => {
  it("names the relay host", () => {
    expect(relayHost("https://relay.example.com:8443/")).toBe("relay.example.com:8443");
    expect(relayHost("relay.example.com")).toBe("relay.example.com");
  });

  it("labels direct and relayed paths", () => {
    expect(pathLabel({ kind: "direct", remote: "192.168.1.20:41641" })).toBe("Direct");
    expect(pathLabel({ kind: "relay", remote: "https://relay.example.com/" })).toBe("Relay (relay.example.com)");
    expect(pathTitle({ kind: "direct", remote: "192.168.1.20:41641", rttMs: 3 })).toContain("192.168.1.20:41641");
    expect(pathTitle({ kind: "relay", remote: "https://r.example.com" })).toContain("end-to-end encrypted");
  });

  it("summarizes the host's guests", () => {
    expect(statusBarText(host({ participants: [person(0, "Robert", "host")] }))).toBe("Waiting for guests");
    const direct = host({
      connections: [
        { participant: 1, kind: "direct", remote: "10.0.0.2:1" },
        { participant: 2, kind: "direct", remote: "10.0.0.3:1" },
      ],
    });
    expect(statusBarText(direct)).toBe("2 guests, direct");
    const mixed = host({
      connections: [
        { participant: 1, kind: "direct", remote: "10.0.0.2:1" },
        { participant: 2, kind: "relay", remote: "https://r.example.com" },
      ],
    });
    expect(statusBarText(mixed)).toBe("2 guests, 1 via relay");
    const relayed = host({
      participants: [person(0, "Robert", "host"), person(1, "Alex", "guest")],
      connections: [{ participant: 1, kind: "relay", remote: "https://r.example.com" }],
    });
    expect(statusBarText(relayed)).toBe("1 guest, via relay");
    expect(statusBarTitle(mixed)).toContain("Sam: Relay (r.example.com)");
  });

  it("shows the guest's path to the host", () => {
    const guest: SessionView = {
      role: "guest",
      canHost: false,
      participants: [person(0, "Robert", "host"), person(1, "Alex", "guest")],
      files: [],
    };
    expect(statusBarText(guest)).toBe("Connecting…");
    expect(
      statusBarText({ ...guest, connections: [{ participant: 0, kind: "direct", remote: "10.0.0.2:1", rttMs: 12 }] }),
    ).toBe("Direct · 12 ms");
  });

  it("reports the state of the relay", () => {
    expect(relaySummary(host())).toBeUndefined();
    expect(relaySummary(host({ relay: { url: "https://r.example.com/", connected: true } }))).toBe("Relay server r.example.com connected");
    const bad = host({ relay: { url: "https://r.example.com/", connected: false, error: "a relay token is required" } });
    expect(relaySummary(bad)).toBe("Relay server r.example.com: a relay token is required");
    expect(relayProblem(bad)).toBe(true);
    expect(relayProblem(host())).toBe(false);
    expect(statusBarText({ ...bad, participants: [person(0, "Robert", "host")] })).toBe("Relay not connected");
  });
});
