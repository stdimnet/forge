/**
 * The guest's Code With Me status bar item: one item with the host, the connection path and
 * the round trip time (the host's connection item is registered for hosts only).
 */
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { render } from "solid-js/web";
import { PRESETS, type SessionView } from "@/ipc/collab";
import { applySession } from "./store";
import { GuestStatusItem, guestStatusDetail } from "./GuestStatusItem";

globalThis.ResizeObserver ??= class {
  observe() {}
  disconnect() {}
  unobserve() {}
} as unknown as typeof ResizeObserver;

const tick = () => new Promise((r) => setTimeout(r, 0));
const $ = <T extends Element = HTMLElement>(sel: string) => document.querySelector<T>(sel);

const guestSession = (over: Partial<SessionView> = {}): SessionView => ({
  role: "guest",
  canHost: false,
  me: 1,
  hostLabel: "Robert (workstation)",
  participants: [
    { id: 0, name: "Robert", role: "host", color: 1, permissions: PRESETS.fullAccess },
    { id: 1, name: "Alex", role: "guest", color: 2, permissions: PRESETS.readOnly },
  ],
  files: [],
  permissions: PRESETS.readOnly,
  connections: [{ participant: 0, kind: "direct", remote: "192.168.1.20:41641", rttMs: 7 }],
  connection: { state: "connected" },
  ...over,
});

let dispose: (() => void) | undefined;
beforeEach(() => {
  dispose = render(() => <GuestStatusItem />, document.body);
});
afterEach(() => {
  dispose?.();
  document.body.innerHTML = "";
  applySession({ role: "none", canHost: false, participants: [], files: [] });
});

describe("guestStatusDetail", () => {
  it("names the path and the round trip time", () => {
    const direct = { kind: "direct", remote: "192.168.1.20:41641" } as const;
    expect(guestStatusDetail(direct, 23)).toBe("Direct · 23 ms");
    expect(guestStatusDetail({ kind: "relay", remote: "https://relay.example.com/" }, null)).toBe("Relay (relay.example.com)");
    expect(guestStatusDetail(undefined, 23)).toBe("23 ms");
    expect(guestStatusDetail(undefined, null)).toBeNull();
  });
});

describe("Guest status bar item", () => {
  it("shows the path to the host with the round trip time", async () => {
    applySession(guestSession());
    await tick();
    const item = $("[data-testid=cwm-status]")!;
    expect(item.dataset.path).toBe("direct");
    expect(item.querySelector(".cwm-status-detail")?.textContent).toMatch(/^Direct · \d+ ms$/);
    expect(item.getAttribute("title")).toContain("Direct connection to 192.168.1.20:41641.");
    item.click();
    await tick();
    expect($("[data-testid=cwm-guest-path]")?.textContent).toBe("Direct");
  });

  it("shows the relay and its state in the popup", async () => {
    applySession(
      guestSession({
        connections: [{ participant: 0, kind: "relay", remote: "https://relay.example.com/", rttMs: 48 }],
        relay: { url: "https://relay.example.com/", connected: true },
      }),
    );
    await tick();
    const item = $("[data-testid=cwm-status]")!;
    expect(item.classList.contains("relay")).toBe(true);
    expect(item.querySelector(".cwm-status-detail")?.textContent).toContain("Relay (relay.example.com)");
    item.click();
    await tick();
    expect($(".cwm-status-popup [data-testid=cwm-relay-status]")).not.toBeNull();
  });

  it("drops the path while reconnecting", async () => {
    applySession(guestSession({ connection: { state: "reconnecting", attempt: 1, nextRetrySecs: 2 } }));
    await tick();
    const item = $("[data-testid=cwm-status]")!;
    expect(item.classList.contains("reconnecting")).toBe(true);
    expect(item.dataset.path).toBeUndefined();
    expect(item.querySelector(".cwm-status-detail")).toBeNull();
  });
});
