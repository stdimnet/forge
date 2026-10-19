import { describe, expect, it } from "vitest";
import "./workspaceMock";
import "./agentMock";
import { agentApi, type AgentEvent } from "@/ipc/agent";
import { invoke } from "@/ipc";

describe("agent mock", () => {
  it("lists Claude Code as installed and Gemini as unsupported", async () => {
    const list = await agentApi.list();
    expect(list.map((a) => [a.id, a.installed, a.supported])).toEqual([
      ["claude-code", true, true],
      ["gemini-cli", false, false],
    ]);
    await expect(agentApi.start("gemini-cli", {})).rejects.toThrow(/not yet supported/);
  });

  it("replays the paid-scope session and edits the demo files", async () => {
    const id = await agentApi.start("claude-code", { model: "sonnet" });
    const events: AgentEvent[] = [];
    const done = new Promise<void>((resolve) => {
      void agentApi.onEvent(id, (e) => {
        events.push(e);
        if (e.type === "turnFinished") resolve();
      });
    });
    await agentApi.send(id, "Add a scope for paid orders to Order model", { files: [] });
    await done;
    expect(events[0].type).toBe("sessionStarted");
    const tools = events.flatMap((e) => (e.type === "toolUse" ? [e.name] : []));
    expect(tools).toEqual(expect.arrayContaining(["Read", "Edit", "MultiEdit", "Bash", "TodoWrite"]));
    expect(await invoke<string>("fs_read_file", { path: "app/Models/Order.php" })).toContain("scopePaid");
    await agentApi.stop(id);
  }, 30000);
});
