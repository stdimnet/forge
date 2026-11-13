import { describe, expect, it } from "vitest";
import "./workspaceMock";
import "./agentMock";
import { agentApi, type AgentEvent } from "@/ipc/agent";
import { invoke } from "@/ipc";

describe("agent mock", () => {
  it("lists both agents with their profiles from Rust", async () => {
    const list = await agentApi.list();
    expect(list.map((a) => [a.id, a.installed, a.supported, a.permissionPrompts])).toEqual([
      ["claude-code", true, true, true],
      ["gemini-cli", true, true, false],
    ]);
    const gemini = list[1];
    expect(gemini.installHint).toBe("npm install -g @google/gemini-cli");
    expect(gemini.permissionModes.map((m) => m.value)).toEqual(["default", "acceptEdits", "bypassPermissions"]);
    await expect(agentApi.start("gemini-cli", { executable: "/opt/nope/gem" })).rejects.toThrow(/Gemini CLI was not found/);
  });

  /** Run one message and collect the raw (non-`chat`) events until the turn finished. */
  async function geminiTurn(mode: "default" | "acceptEdits" | "bypassPermissions") {
    const id = await agentApi.start("gemini-cli", { permissionMode: mode });
    const events: AgentEvent[] = [];
    const done = new Promise<void>((resolve) => {
      void agentApi.onEvent(id, (e) => {
        if (e.type !== "chat") events.push(e);
        if (e.type === "turnFinished") resolve();
      });
    });
    await agentApi.send(id, "Add a scope for paid orders to Order model", { files: [] });
    await done;
    await agentApi.stop(id);
    return events;
  }

  it("replays recorded Gemini CLI output through the Rust parser", async () => {
    const before = await invoke<string>("fs_read_file", { path: "app/Models/Order.php" });
    const readOnly = await geminiTurn("default");
    expect(readOnly[0]).toMatchObject({ type: "sessionStarted", model: "gemini-2.5-pro", cwd: expect.stringContaining("acme-shop") });
    // Gemini's tool names arrive in Forge's common vocabulary.
    expect(readOnly.flatMap((e) => (e.type === "toolUse" ? [e.name] : []))).toEqual(["Read", "Read", "Grep"]);
    expect(readOnly.some((e) => e.type === "permissionRequest")).toBe(false);
    expect(await invoke<string>("fs_read_file", { path: "app/Models/Order.php" })).toBe(before);

    const yolo = await geminiTurn("bypassPermissions");
    expect(yolo.flatMap((e) => (e.type === "toolUse" ? [e.name] : []))).toEqual(["Read", "Read", "Grep", "Edit", "Bash"]);
    expect(yolo.find((e) => e.type === "turnFinished")).toMatchObject({ type: "turnFinished", isError: false, usage: { cacheReadTokens: 22400 } });
    expect(await invoke<string>("fs_read_file", { path: "app/Models/Order.php" })).toContain("scopePaid");
  }, 30000);

  it("replays the paid-scope session and edits the demo files", async () => {
    const id = await agentApi.start("claude-code", { model: "sonnet", permissionMode: "bypassPermissions" });
    const events: AgentEvent[] = [];
    const done = new Promise<void>((resolve) => {
      void agentApi.onEvent(id, (e) => {
        events.push(e);
        if (e.type === "turnFinished") resolve();
      });
    });
    await agentApi.send(id, "Add a scope for paid orders to Order model", { files: [] });
    await done;
    expect(events.filter((e) => e.type !== "chat")[0].type).toBe("sessionStarted");
    const tools = events.flatMap((e) => (e.type === "toolUse" ? [e.name] : []));
    expect(tools).toEqual(expect.arrayContaining(["Read", "Edit", "MultiEdit", "Bash", "TodoWrite"]));
    expect(await invoke<string>("fs_read_file", { path: "app/Models/Order.php" })).toContain("scopePaid");
    await agentApi.stop(id);
  }, 30000);

  it("asks before edits in the default mode and stops when denied", async () => {
    const id = await agentApi.start("claude-code", { permissionMode: "default" });
    const events: AgentEvent[] = [];
    const done = new Promise<void>((resolve) => {
      void agentApi.onEvent(id, (e) => {
        events.push(e);
        if (e.type === "permissionRequest") void agentApi.respondPermission(id, e.requestId, { behavior: "deny" });
        if (e.type === "turnFinished") resolve();
      });
    });
    await agentApi.send(id, "Add a scope for paid orders", { files: [] });
    await done;
    const asked = events.flatMap((e) => (e.type === "permissionRequest" ? [e.toolName] : []));
    expect(asked).toEqual(["Edit"]);
    expect(events.some((e) => e.type === "toolResult" && e.isError && /rejected/.test(e.content))).toBe(true);
    expect(events.some((e) => e.type === "toolUse" && e.name === "Bash")).toBe(false);
    await expect(agentApi.respondPermission(id, "nope", { behavior: "deny" })).rejects.toThrow(/no pending/);
    await agentApi.stop(id);
  }, 30000);

  it("reports installation for configured executables", async () => {
    const list = await agentApi.list({ "claude-code": "/opt/claude/bin/claude", "gemini-cli": "gemini" });
    expect(list.map((a) => [a.id, a.installed, a.path])).toEqual([
      ["claude-code", true, "/opt/claude/bin/claude"],
      ["gemini-cli", false, null],
    ]);
  });
});
