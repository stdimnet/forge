/**
 * Agent CLI sessions (Claude Code, Gemini CLI, ...) run headless by the backend
 * (`commands/agent.rs`). Events arrive on `agent://<sessionId>`.
 */
import { invoke, listen } from "./index";

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

/** Mirrors `forge_agent_cli::AgentEvent`. */
export type AgentEvent =
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
  | { type: "permissionRequest"; requestId: string; toolName: string; input: Record<string, unknown> }
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

export const agentApi = {
  list: (executables?: Record<string, string>) => invoke<AgentInfo[]>("agent_list", { executables }),
  start: (agentId: string, options: StartOptions) => invoke<string>("agent_start", { agentId, options }),
  send: (sessionId: string, text: string, context?: MessageContext) =>
    invoke<void>("agent_send", { sessionId, text, context }),
  interrupt: (sessionId: string) => invoke<void>("agent_interrupt", { sessionId }),
  stop: (sessionId: string) => invoke<void>("agent_stop", { sessionId }),
  onEvent: (sessionId: string, cb: (e: AgentEvent) => void) => listen<AgentEvent>(`agent://${sessionId}`, cb),
};
