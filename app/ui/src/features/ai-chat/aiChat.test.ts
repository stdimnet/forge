import { describe, expect, it } from "vitest";
import type { AgentEvent } from "@/ipc/agent";
import {
  addTurn,
  applyEvent,
  awaitingPermission,
  changedFilesOf,
  emptyConversation,
  findPermission,
  formatDuration,
  formatTokens,
  relativePath,
} from "./model";
import { permissionSummary } from "./PermissionCard";
import { parseInline, parseMarkdown } from "./markdown";
import { diffLines, diffStats, editHunks } from "./diff";
import { highlight } from "./highlight";

const CWD = "/home/dev/shop";

function conversation(events: AgentEvent[]) {
  const conv = emptyConversation();
  addTurn(conv, "Add a scope", { files: [] });
  let t = 0;
  const results = events.map((e) => applyEvent(conv, e, (t += 1000)));
  return { conv, turn: conv.turns[0], results };
}

describe("permission requests", () => {
  it("show as pending blocks that expire with the turn", () => {
    const { conv, turn } = conversation([
      { type: "toolUse", id: "t1", name: "Bash", input: { command: "php artisan test" } },
      { type: "permissionRequest", requestId: "r1", toolName: "Bash", input: { command: "php artisan test" }, toolUseId: "t1" },
    ]);
    expect(awaitingPermission(conv)).toBe(true);
    expect(findPermission(conv, "r1")).toMatchObject({ kind: "permission", status: "pending", toolUseId: "t1" });
    applyEvent(conv, { type: "turnFinished", result: null, isError: false, durationMs: 1, numTurns: 1, costUsd: null, usage: null });
    expect(findPermission(conv, "r1")?.status).toBe("expired");
    expect(awaitingPermission(conv)).toBe(false);
    expect(turn.status).toBe("done");
  });

  it("summarizes what is asked", () => {
    expect(permissionSummary({ toolName: "Bash", input: { command: "ls" } }, CWD)).toEqual({ action: "run this command", subject: "ls" });
    expect(permissionSummary({ toolName: "Edit", input: { file_path: `${CWD}/app/Order.php` } }, CWD).action).toBe("edit Order.php");
    expect(permissionSummary({ toolName: "Write", input: { file_path: `${CWD}/a/New.php` } }, CWD).action).toBe("create New.php");
    expect(permissionSummary({ toolName: "mcp__db__query", input: {} }, CWD).action).toBe("use mcp__db__query");
  });
});

describe("conversation model", () => {
  it("folds a streamed turn into blocks", () => {
    const { conv, turn, results } = conversation([
      { type: "sessionStarted", sessionId: "s1", model: "sonnet", tools: [], cwd: CWD, permissionMode: null },
      { type: "thinking", text: "Let me ", delta: true },
      { type: "thinking", text: "look.", delta: true },
      { type: "assistantText", text: "I'll ", delta: true },
      { type: "assistantText", text: "read it.", delta: true },
      { type: "toolUse", id: "t1", name: "Read", input: { file_path: `${CWD}/app/Order.php` } },
      { type: "toolResult", toolUseId: "t1", content: "<?php", isError: false },
      { type: "toolUse", id: "t2", name: "Edit", input: { file_path: `${CWD}/app/Order.php`, old_string: "a", new_string: "b" } },
      { type: "toolResult", toolUseId: "t2", content: "updated", isError: false },
      { type: "assistantText", text: "Done.", delta: false },
      { type: "turnFinished", result: "Done.", isError: false, durationMs: 5000, numTurns: 3, costUsd: 0.01, usage: null },
    ]);
    expect(conv.conversationId).toBe("s1");
    expect(turn.blocks.map((b) => b.kind)).toEqual(["thinking", "text", "tool", "tool", "text"]);
    const thinking = turn.blocks[0];
    expect(thinking.kind === "thinking" && thinking.text).toBe("Let me look.");
    expect(thinking.kind === "thinking" && thinking.durationMs).toBe(2000);
    expect(turn.blocks[1]).toMatchObject({ kind: "text", text: "I'll read it.", streaming: false });
    expect(turn.blocks[2]).toMatchObject({ kind: "tool", status: "done", result: "<?php" });
    expect(results[8].changedFiles).toEqual(["app/Order.php"]);
    expect(results[10].turnEnded).toBe(true);
    expect(turn.status).toBe("done");
    expect(turn.summary?.costUsd).toBe(0.01);
    expect(changedFilesOf(turn, CWD)).toEqual(["app/Order.php"]);
  });

  it("marks running tools as failed when interrupted", () => {
    const { turn } = conversation([
      { type: "toolUse", id: "t1", name: "Bash", input: { command: "sleep 10" } },
      { type: "interrupted" },
      { type: "exited", code: null },
    ]);
    expect(turn.status).toBe("interrupted");
    expect(turn.blocks[0]).toMatchObject({ status: "error" });
    expect(turn.blocks.some((b) => b.kind === "error")).toBe(false);
  });

  it("reports a crash when the process exits mid-turn", () => {
    const { turn } = conversation([{ type: "error", message: "not logged in" }, { type: "exited", code: 1 }]);
    expect(turn.status).toBe("error");
    expect(turn.blocks).toEqual([{ kind: "error", message: "not logged in" }]);
  });

  it("nests sub-agent tool calls in their Task card", () => {
    const { turn } = conversation([
      { type: "toolUse", id: "task", name: "Task", input: { description: "Explore" } },
      { type: "toolUse", id: "c1", name: "Grep", input: { pattern: "x" }, parentToolUseId: "task" },
      { type: "assistantText", text: "sub-agent prose", delta: false, parentToolUseId: "task" },
      { type: "toolResult", toolUseId: "c1", content: "", isError: false },
    ]);
    expect(turn.blocks).toHaveLength(1);
    const task = turn.blocks[0];
    expect(task.kind === "tool" && task.children).toEqual([{ id: "c1", name: "Grep", input: { pattern: "x" }, status: "done" }]);
  });

  it("relativizes paths and formats numbers", () => {
    expect(relativePath(`${CWD}/a/b.php`, CWD)).toBe("a/b.php");
    expect(relativePath("C:\\p\\a\\b.php", "C:\\p")).toBe("a/b.php");
    expect(relativePath("/elsewhere/x", CWD)).toBe("/elsewhere/x");
    expect(formatTokens(48210)).toBe("48k");
    expect(formatTokens(1240)).toBe("1.2k");
    expect(formatDuration(24610)).toBe("25s");
    expect(formatDuration(4200)).toBe("4.2s");
  });
});

describe("markdown", () => {
  it("parses blocks", () => {
    const md = parseMarkdown("# Title\n\nSome **bold** and `code`.\n\n- [x] done\n- [ ] todo\n\n```php\n$a = 1;\n```\n1. one\n2. two");
    expect(md.map((b) => b.t)).toEqual(["heading", "paragraph", "list", "code", "list"]);
    expect(md[2]).toMatchObject({ t: "list", items: [{ checked: true }, { checked: false }] });
    expect(md[3]).toEqual({ t: "code", lang: "php", text: "$a = 1;", open: false });
    expect(md[4]).toMatchObject({ t: "list", ordered: true, start: 1 });
  });

  it("keeps an unterminated fence open while streaming", () => {
    expect(parseMarkdown("```js\nlet a")[0]).toEqual({ t: "code", lang: "js", text: "let a", open: true });
  });

  it("parses inline markup without HTML", () => {
    expect(parseInline("a <b>x</b> `c` [l](https://x.dev) *e*")).toEqual([
      { t: "text", text: "a <b>x</b> " },
      { t: "code", text: "c" },
      { t: "text", text: " " },
      { t: "link", text: "l", href: "https://x.dev" },
      { t: "text", text: " " },
      { t: "em", text: "e" },
    ]);
  });
});

describe("diff", () => {
  it("diffs lines", () => {
    const lines = diffLines("a\nb\nc", "a\nB\nc\nd");
    expect(lines.map((l) => `${l.type}:${l.text}`)).toEqual(["ctx:a", "add:B", "del:b", "ctx:c", "add:d"]);
    expect(diffStats(lines)).toEqual({ added: 2, removed: 1 });
  });

  it("extracts hunks from edit tools", () => {
    expect(editHunks("Write", { content: "x" })).toEqual([{ oldText: "", newText: "x" }]);
    expect(editHunks("MultiEdit", { edits: [{ old_string: "a", new_string: "b" }] })).toEqual([{ oldText: "a", newText: "b" }]);
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
