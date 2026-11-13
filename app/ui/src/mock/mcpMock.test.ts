import { describe, expect, it } from "vitest";
import { invoke, listen } from "@/ipc";
import { MCP_CALL_EVENT, type McpCallLogEntry, type McpClientConfig, type McpStatus } from "@/ipc/mcp";
import { forgeWasm } from "@/wasm";
import { setSetting } from "./settingsMock";
import { showDemoApproval } from "./mcpMock";
import "./mcpMock";

describe("mcp mock", () => {
  it("follows the enabled setting and builds client configurations in Rust", async () => {
    await forgeWasm();
    setSetting("mcp.enabled", false, "user");
    expect((await invoke<McpStatus>("mcp_status")).running).toBe(false);
    expect(await invoke("mcp_client_configs")).toBeNull();
    setSetting("mcp.enabled", true, "user");
    const status = await invoke<McpStatus>("mcp_status");
    expect(status.url).toBe("http://127.0.0.1:64342/mcp");
    expect(status.sessions.map((s) => s.client.name)).toContain("Claude Code");
    const configs = await invoke<McpClientConfig[]>("mcp_client_configs");
    const claude = configs.find((c) => c.id === "claude-code")!;
    expect(claude.text).toMatch(/^claude mcp add --transport http forge http:\/\/127\.0\.0\.1:64342\/mcp --header "Authorization: Bearer [0-9a-f]{64}"$/);
    expect(configs.map((c) => c.id)).toEqual(expect.arrayContaining(["claude-desktop", "cursor", "gemini-cli"]));
    setSetting("mcp.enabled", null, "user");
  });

  it("logs answered approvals", async () => {
    const calls: McpCallLogEntry[] = [];
    const off = await listen<McpCallLogEntry>(MCP_CALL_EVENT, (e) => calls.push(e));
    const req = showDemoApproval();
    expect(await invoke("mcp_answer_approval", { id: req.id, answer: "deny" })).toBe(true);
    expect(calls[0].status).toBe("denied");
    expect(await invoke("mcp_answer_approval", { id: req.id, answer: "once" })).toBe(false);
    off();
  });
});
