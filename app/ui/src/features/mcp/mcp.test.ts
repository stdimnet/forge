import { describe, expect, it } from "vitest";
import type { McpStatus } from "@/ipc/mcp";
import { clientsText, durationText, statusText } from "./store";

const base: McpStatus = { enabled: true, running: true, port: 64342, url: "http://127.0.0.1:64342/mcp", error: null, sessions: [], tokenPersistent: true, safeMode: false, aiDisabled: false };
const session = (name: string) => ({ id: name, client: { name, version: "1" }, protocolVersion: "2025-06-18", transport: "http", connectedAt: 0, lastSeenAt: 0, calls: 0 });

describe("mcp status texts", () => {
  it("names the connected clients", () => {
    expect(statusText(null)).toBe("MCP off");
    expect(statusText({ ...base, running: false })).toBe("MCP: not running");
    expect(statusText(base)).toBe("MCP");
    expect(statusText({ ...base, sessions: [session("Claude Code")] })).toBe("MCP: Claude Code");
    expect(clientsText({ ...base, sessions: [session("A"), session("B")] })).toBe("2 clients");
  });

  it("formats durations", () => {
    expect(durationText(340)).toBe("340 ms");
    expect(durationText(2400)).toBe("2.4 s");
  });
});
