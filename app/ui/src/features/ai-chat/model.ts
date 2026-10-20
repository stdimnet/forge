/**
 * Conversation model: folds the normalized agent event stream into turns and blocks.
 * Pure functions that mutate a draft, so they work with Solid's `produce` and in tests.
 */
import type { AgentEvent, MessageContext, Usage } from "@/ipc/agent";

export type ToolStatus = "running" | "done" | "error";

export interface ToolBlock {
  kind: "tool";
  id: string;
  name: string;
  input: Record<string, unknown>;
  status: ToolStatus;
  result?: string;
  /** Tool calls made by a sub-agent started by this tool (e.g. `Task`). */
  children: { id: string; name: string; input: Record<string, unknown>; status: ToolStatus }[];
}

export type Block =
  | { kind: "text"; text: string; streaming: boolean }
  | { kind: "thinking"; text: string; streaming: boolean; startedAt: number; durationMs?: number }
  | ToolBlock
  | { kind: "error"; message: string }
  | PermissionBlock;

/** Pending: waiting for the user. Expired: the turn ended before an answer. */
export type PermissionStatus = "pending" | "allowed" | "allowedForSession" | "denied" | "expired";

export interface PermissionBlock {
  kind: "permission";
  requestId: string;
  toolName: string;
  input: Record<string, unknown>;
  toolUseId?: string;
  status: PermissionStatus;
}

export type TurnStatus = "running" | "done" | "error" | "interrupted";

export interface TurnSummary {
  durationMs: number | null;
  numTurns: number | null;
  costUsd: number | null;
  usage: Usage | null;
}

export interface Turn {
  id: number;
  prompt: string;
  context: MessageContext;
  blocks: Block[];
  status: TurnStatus;
  summary?: TurnSummary;
}

export interface Conversation {
  /** Conversation id reported by the CLI; used to resume. */
  conversationId: string | null;
  /** Model reported by the CLI on start. */
  model: string | null;
  /** Working directory of the CLI, used to show workspace-relative paths. */
  cwd: string | null;
  turns: Turn[];
}

export const emptyConversation = (): Conversation => ({ conversationId: null, model: null, cwd: null, turns: [] });

let nextTurnId = 1;

export function addTurn(conv: Conversation, prompt: string, context: MessageContext): Turn {
  const turn: Turn = { id: nextTurnId++, prompt, context, blocks: [], status: "running" };
  conv.turns.push(turn);
  return turn;
}

/** Tools whose successful result means a file in the workspace changed. */
export const EDIT_TOOLS = new Set(["Edit", "MultiEdit", "Write", "NotebookEdit", "replace", "write_file"]);

/** Path a tool operates on, if any. */
export function toolPath(input: Record<string, unknown>): string | undefined {
  const p = input.file_path ?? input.notebook_path ?? input.absolute_path ?? input.path;
  return typeof p === "string" ? p : undefined;
}

/** Strip the agent's working directory so paths are workspace-relative. */
export function relativePath(path: string, cwd: string | null): string {
  if (cwd) {
    const root = cwd.replace(/[\\/]+$/, "");
    if (path === root) return "";
    if (path.startsWith(`${root}/`) || path.startsWith(`${root}\\`)) return path.slice(root.length + 1).replace(/\\/g, "/");
  }
  return path;
}

function settle(turn: Turn, now: number): void {
  const last = turn.blocks[turn.blocks.length - 1];
  if (!last) return;
  if (last.kind === "text") last.streaming = false;
  if (last.kind === "thinking" && last.streaming) {
    last.streaming = false;
    last.durationMs = now - last.startedAt;
  }
}

function findTool(turn: Turn, id: string): ToolBlock | undefined {
  for (const b of turn.blocks) if (b.kind === "tool" && b.id === id) return b;
  return undefined;
}

export interface ApplyResult {
  /** Workspace-relative paths changed by an edit tool in this event. */
  changedFiles?: string[];
  /** The turn ended (finished, interrupted or the process exited). */
  turnEnded?: boolean;
}

/** Apply one event to the conversation. `now` is injectable for tests. */
export function applyEvent(conv: Conversation, ev: AgentEvent, now = Date.now()): ApplyResult {
  if (ev.type === "sessionStarted") {
    conv.conversationId = ev.sessionId || conv.conversationId;
    conv.model = ev.model;
    conv.cwd = ev.cwd ?? conv.cwd;
    return {};
  }
  const turn = conv.turns[conv.turns.length - 1];
  if (!turn) return {};
  const running = turn.status === "running";

  switch (ev.type) {
    case "assistantText": {
      if (ev.parentToolUseId) return {}; // sub-agent prose stays inside its tool card
      const last = turn.blocks[turn.blocks.length - 1];
      if (ev.delta && last?.kind === "text" && last.streaming) {
        last.text += ev.text;
      } else {
        settle(turn, now);
        turn.blocks.push({ kind: "text", text: ev.text, streaming: ev.delta });
      }
      return {};
    }
    case "thinking": {
      const last = turn.blocks[turn.blocks.length - 1];
      if (ev.delta && last?.kind === "thinking" && last.streaming) {
        last.text += ev.text;
      } else {
        settle(turn, now);
        turn.blocks.push({ kind: "thinking", text: ev.text, streaming: ev.delta, startedAt: now, durationMs: ev.delta ? undefined : 0 });
      }
      return {};
    }
    case "toolUse": {
      const parent = ev.parentToolUseId ? findTool(turn, ev.parentToolUseId) : undefined;
      if (parent) {
        parent.children.push({ id: ev.id, name: ev.name, input: ev.input ?? {}, status: "running" });
        return {};
      }
      settle(turn, now);
      turn.blocks.push({ kind: "tool", id: ev.id, name: ev.name, input: ev.input ?? {}, status: "running", children: [] });
      return {};
    }
    case "toolResult": {
      const status: ToolStatus = ev.isError ? "error" : "done";
      const tool = findTool(turn, ev.toolUseId);
      if (!tool) {
        for (const b of turn.blocks) {
          if (b.kind !== "tool") continue;
          const child = b.children.find((c) => c.id === ev.toolUseId);
          if (child) child.status = status;
        }
        return {};
      }
      tool.status = status;
      tool.result = ev.content;
      const path = toolPath(tool.input);
      if (!ev.isError && path && EDIT_TOOLS.has(tool.name)) return { changedFiles: [relativePath(path, conv.cwd)] };
      return {};
    }
    case "permissionRequest":
      settle(turn, now);
      turn.blocks.push({
        kind: "permission",
        requestId: ev.requestId,
        toolName: ev.toolName,
        input: ev.input ?? {},
        toolUseId: ev.toolUseId,
        status: "pending",
      });
      return {};
    case "usage":
      return {};
    case "error":
      settle(turn, now);
      turn.blocks.push({ kind: "error", message: ev.message });
      return {};
    case "turnFinished":
      settle(turn, now);
      finishTools(turn, ev.isError ? "error" : "done");
      turn.status = ev.isError ? "error" : "done";
      if (ev.isError && ev.result && !turn.blocks.some((b) => b.kind === "error")) {
        turn.blocks.push({ kind: "error", message: ev.result });
      }
      turn.summary = { durationMs: ev.durationMs, numTurns: ev.numTurns, costUsd: ev.costUsd, usage: ev.usage };
      return { turnEnded: true };
    case "interrupted":
      settle(turn, now);
      if (running) {
        finishTools(turn, "error");
        turn.status = "interrupted";
      }
      return { turnEnded: true };
    case "exited":
      if (!running) return { turnEnded: true };
      settle(turn, now);
      finishTools(turn, "error");
      turn.status = "error";
      if (!turn.blocks.some((b) => b.kind === "error")) {
        turn.blocks.push({ kind: "error", message: `The agent exited unexpectedly (code ${ev.code ?? "unknown"}).` });
      }
      return { turnEnded: true };
  }
}

function finishTools(turn: Turn, status: ToolStatus): void {
  for (const b of turn.blocks) {
    if (b.kind === "tool" && b.status === "running") b.status = status;
    // Unanswered requests can no longer be answered once the turn is over.
    if (b.kind === "permission" && b.status === "pending") b.status = "expired";
  }
}

/** The permission request with this id, if any. */
export function findPermission(conv: Conversation, requestId: string): PermissionBlock | undefined {
  for (const t of conv.turns) for (const b of t.blocks) if (b.kind === "permission" && b.requestId === requestId) return b;
  return undefined;
}

/** Whether the agent is waiting for the user to answer a permission request. */
export const awaitingPermission = (conv: Conversation): boolean =>
  !!conv.turns[conv.turns.length - 1]?.blocks.some((b) => b.kind === "permission" && b.status === "pending");

/** Files edited during a turn, in order of first edit. */
export function changedFilesOf(turn: Turn, cwd: string | null): string[] {
  const out: string[] = [];
  for (const b of turn.blocks) {
    if (b.kind !== "tool" || b.status !== "done" || !EDIT_TOOLS.has(b.name)) continue;
    const p = toolPath(b.input);
    if (p) {
      const rel = relativePath(p, cwd);
      if (!out.includes(rel)) out.push(rel);
    }
  }
  return out;
}

export function formatTokens(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}k` : String(n);
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const s = ms / 1000;
  if (s < 60) return `${s < 10 ? s.toFixed(1) : Math.round(s)}s`;
  return `${Math.floor(s / 60)}m ${Math.round(s % 60)}s`;
}
