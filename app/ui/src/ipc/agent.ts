/**
 * Agent CLI sessions (Claude Code, Gemini CLI, ...) run headless by the backend
 * (`commands/agent.rs`). Events arrive on `agent://<sessionId>`: the raw agent events and,
 * after each, a `chat` update with the rendered blocks of the current turn
 * (`forge_agent_cli::conversation`).
 */
import { invoke, listen } from "./index";
import type { MdBlock } from "./markdown";

export type PermissionMode = "default" | "acceptEdits" | "plan" | "bypassPermissions";

export interface AgentInfo {
  id: string;
  name: string;
  supported: boolean;
  installed: boolean;
  path: string | null;
  models: string[];
}

export interface StartOptions {
  model?: string;
  permissionMode?: PermissionMode;
  resume?: string;
  allowedTools?: string[];
  executable?: string;
}

export interface Selection {
  path: string;
  startLine: number;
  endLine: number;
  text: string;
}

export interface MessageContext {
  files: string[];
  selection?: Selection;
}

export interface Usage {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens?: number;
  cacheWriteTokens?: number;
}

export type ToolStatus = "running" | "done" | "error";

/** One `+`/`-`/context line of an edit diff; `near`: distance to the nearest change. */
export interface DiffLine {
  type: "ctx" | "add" | "del";
  text: string;
  oldNo?: number;
  newNo?: number;
  near: number;
}

/** The change an edit tool call makes (one line list per old/new pair). */
export interface EditDiff {
  hunks: DiffLine[][];
  added: number;
  removed: number;
}

export interface ToolBlock {
  kind: "tool";
  id: string;
  name: string;
  input: Record<string, unknown>;
  status: ToolStatus;
  result?: string;
  /** Tool calls of a sub-agent started by this tool (e.g. `Task`); `subject`: path, pattern or command. */
  children: { id: string; name: string; input: Record<string, unknown>; status: ToolStatus; subject: string }[];
  /** Workspace-relative path the tool works on. */
  path?: string;
  diff?: EditDiff;
  /** Non-empty lines of the result. */
  resultLines: number;
  /** An older todo list. */
  superseded?: boolean;
}

/** Pending: waiting for the user. Expired: the turn ended before an answer. */
export type PermissionStatus = "pending" | "allowed" | "allowedForSession" | "denied" | "expired";

export interface PermissionBlock {
  kind: "permission";
  requestId: string;
  toolName: string;
  input: Record<string, unknown>;
  toolUseId?: string;
  status: PermissionStatus;
  /** "run this command", "edit Order.php". */
  action: string;
  subject: string;
  path?: string;
  diff?: EditDiff;
}

/** A rendered block of a chat turn (`forge_agent_cli::conversation::Block`). */
export type ChatBlock =
  | { kind: "text"; text: string; streaming: boolean; markdown: MdBlock[] }
  | { kind: "thinking"; text: string; streaming: boolean; startedAt: number; durationMs?: number; label: string }
  | ToolBlock
  | { kind: "error"; message: string }
  | PermissionBlock;

export type TurnStatus = "running" | "done" | "error" | "interrupted";

export interface TurnSummary {
  durationMs: number | null;
  numTurns: number | null;
  costUsd: number | null;
  usage: Usage | null;
  durationText?: string;
  tokensText?: string;
  costText?: string;
}

/** Changes of the current turn: blocks with their index (appended at the current length). */
export interface TurnUpdate {
  id: number;
  status: TurnStatus;
  summary?: TurnSummary;
  blocks: [number, ChatBlock][];
  changedFiles: string[];
  awaitingPermission: boolean;
  working: boolean;
}

export interface ChatUpdate {
  conversationId: string | null;
  model: string | null;
  cwd: string | null;
  turn?: TurnUpdate;
  /** Files changed by this event (reload them). */
  changedFiles: string[];
  turnEnded: boolean;
}

/** Mirrors `forge_agent_cli::AgentEvent`, plus the `chat` updates. */
export type AgentEvent =
  | ({ type: "chat" } & ChatUpdate)
  | {
      type: "sessionStarted";
      sessionId: string;
      model: string | null;
      tools: string[];
      cwd: string | null;
      permissionMode: string | null;
    }
  | { type: "assistantText"; text: string; delta: boolean; parentToolUseId?: string }
  | { type: "thinking"; text: string; delta: boolean }
  | { type: "toolUse"; id: string; name: string; input: Record<string, unknown>; parentToolUseId?: string }
  | { type: "toolResult"; toolUseId: string; content: string; isError: boolean }
  | { type: "usage"; usage: Usage; costUsd: number | null }
  | { type: "permissionRequest"; requestId: string; toolName: string; input: Record<string, unknown>; toolUseId?: string }
  | {
      type: "turnFinished";
      result: string | null;
      isError: boolean;
      durationMs: number | null;
      numTurns: number | null;
      costUsd: number | null;
      usage: Usage | null;
    }
  | { type: "interrupted" }
  | { type: "error"; message: string }
  | { type: "exited"; code: number | null };

/** Answer to a permission request (mirrors `forge_agent_cli::PermissionDecision`). */
export type PermissionDecision =
  | { behavior: "allow"; scope?: "once" | "session"; updatedInput?: Record<string, unknown> }
  | { behavior: "deny"; message?: string };

export const agentApi = {
  list: (executables?: Record<string, string>) => invoke<AgentInfo[]>("agent_list", { executables }),
  start: (agentId: string, options: StartOptions) => invoke<string>("agent_start", { agentId, options }),
  send: (sessionId: string, text: string, context?: MessageContext, turnId?: number) =>
    invoke<void>("agent_send", { sessionId, text, context, turnId }),
  respondPermission: (sessionId: string, requestId: string, decision: PermissionDecision) =>
    invoke<void>("agent_respond_permission", { sessionId, requestId, decision }),
  interrupt: (sessionId: string) => invoke<void>("agent_interrupt", { sessionId }),
  stop: (sessionId: string) => invoke<void>("agent_stop", { sessionId }),
  onEvent: (sessionId: string, cb: (e: AgentEvent) => void) => listen<AgentEvent>(`agent://${sessionId}`, cb),
};
