import { describe, expect, it } from "vitest";
import type { AgentEvent, ChatUpdate } from "@/ipc/agent";
import { forgeWasm } from "@/wasm";
import { addTurn, applyChatUpdate, awaitingPermission, emptyConversation } from "./model";
import { highlight } from "./highlight";

const CWD = "/home/dev/shop";

/** Fold events with the Rust conversation model (WebAssembly) and apply its updates. */
async function conversation(events: AgentEvent[]) {
  const wasm = await forgeWasm();
  const handle = Number(wasm.chat_new());
  const conv = emptyConversation();
  const turn = addTurn(conv, "Add a scope", { files: [] });
  const apply = (json: string) => {
    const u = JSON.parse(json) as ChatUpdate;
    applyChatUpdate(conv, u);
    return u;
  };
  apply(wasm.chat_begin_turn(JSON.stringify({ handle, turnId: turn.id, prompt: "Add a scope" })));
  let now = 0;
  const updates = events.map((event) => apply(wasm.chat_apply(JSON.stringify({ handle, event, now: (now += 1000) }))));
  wasm.chat_drop(JSON.stringify({ handle }));
  return { conv, turn: conv.turns[0], updates };
}

describe("conversation (Rust forge-agent-cli via WebAssembly)", () => {
  it("renders a streamed turn: markdown, tool cards with diffs, summary", async () => {
    const { conv, turn, updates } = await conversation([
      { type: "sessionStarted", sessionId: "s1", model: "sonnet", tools: [], cwd: CWD, permissionMode: null },
      { type: "assistantText", text: "I'll **edit** it.", delta: true },
      { type: "toolUse", id: "t1", name: "Edit", input: { file_path: `${CWD}/app/Order.php`, old_string: "a", new_string: "b\nc" } },
      { type: "toolResult", toolUseId: "t1", content: "updated", isError: false },
      { type: "turnFinished", result: null, isError: false, durationMs: 4200, numTurns: 1, costUsd: 0.2, usage: null },
    ]);
    expect(conv.conversationId).toBe("s1");
    expect(turn.blocks.map((b) => b.kind)).toEqual(["text", "tool"]);
    const text = turn.blocks[0];
    expect(text.kind === "text" && text.markdown[0]).toMatchObject({ t: "paragraph" });
    const tool = turn.blocks[1];
    expect(tool.kind === "tool" && [tool.path, tool.diff?.added, tool.diff?.removed]).toEqual(["app/Order.php", 2, 1]);
    expect(updates[3].changedFiles).toEqual(["app/Order.php"]);
    expect(turn).toMatchObject({ status: "done", changedFiles: ["app/Order.php"], working: false });
    expect(turn.summary).toMatchObject({ durationText: "4.2s", costText: "$0.20" });
  });

  it("tracks pending permission requests", async () => {
    const { conv, turn } = await conversation([
      { type: "sessionStarted", sessionId: "s1", model: null, tools: [], cwd: CWD, permissionMode: null },
      { type: "permissionRequest", requestId: "r1", toolName: "Write", input: { file_path: `${CWD}/a/New.php`, content: "x" } },
    ]);
    expect(awaitingPermission(conv)).toBe(true);
    expect(turn.blocks[0]).toMatchObject({ kind: "permission", status: "pending", action: "create New.php", path: "a/New.php" });
  });
});

describe("highlight", () => {
  it("highlights PHP with the Lezer parser", () => {
    const lines = highlight("$a = 'x';\nreturn $a;", "php");
    expect(lines).toHaveLength(2);
    expect(lines[0].some((s) => s.cls.includes("tok-string"))).toBe(true);
    expect(lines[1].some((s) => s.cls.includes("tok-keyword"))).toBe(true);
  });

  it("falls back to a regex tokenizer", () => {
    const lines = highlight('echo "hi" # note', "bash");
    expect(lines[0]).toContainEqual({ text: '"hi"', cls: "tok-string" });
    expect(lines[0]).toContainEqual({ text: "# note", cls: "tok-comment" });
  });
});
