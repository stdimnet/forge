/**
 * Forge as an MCP server (Settings | Tools | MCP Server). Rust side:
 * `app/src-tauri/src/commands/mcp.rs` and `crates/forge-mcp`.
 */
import { invoke, listen } from "./index";

export const MCP_STATUS_EVENT = "mcp://status";
export const MCP_CALL_EVENT = "mcp://call";
export const MCP_APPROVAL_EVENT = "mcp://approval";
export const MCP_APPROVAL_CLOSED_EVENT = "mcp://approval-closed";
export const MCP_TOOL_REQUEST_EVENT = "mcp://tool-request";

export interface McpSession {
  id: string;
  client: { name: string; version: string };
  protocolVersion: string;
  /** `http` or `sse`. */
  transport: string;
  /** Unix milliseconds. */
  connectedAt: number;
  lastSeenAt: number;
  calls: number;
}

export interface McpStatus {
  enabled: boolean;
  running: boolean;
  port?: number | null;
  url?: string | null;
  error?: string | null;
  sessions: McpSession[];
  tokenPersistent: boolean;
  safeMode: boolean;
  aiDisabled: boolean;
}

export type McpToolGroup = "read" | "vcs" | "edit" | "run" | "terminal" | "database";
export type McpToolKind = "read" | "write" | "exec";
export type McpCallStatus = "ok" | "error" | "refused" | "denied";

export interface McpCallLogEntry {
  id: number;
  session: string;
  client: string;
  tool: string;
  /** Arguments as JSON (cut when long). */
  arguments: string;
  status: McpCallStatus;
  message?: string | null;
  /** Unix milliseconds. */
  startedAt: number;
  durationMs: number;
}

export interface McpApprovalRequest {
  id: number;
  session: string;
  client: string;
  tool: string;
  title: string;
  group: McpToolGroup;
  kind: McpToolKind;
  /** "edit app/Models/User.php", "run `npm test`". */
  summary: string;
  arguments: Record<string, unknown>;
  /** Whether "Allow for this session" is offered (never for terminal commands). */
  sessionAllowed: boolean;
}

export type McpApproval = "once" | "session" | "deny";

export interface McpClientConfig {
  /** `claude-code`, `claude-desktop`, `cursor`, `gemini-cli`, `codex`, `vscode`. */
  id: string;
  name: string;
  transport: "http" | "stdio";
  /** A command to run, or the file to paste into. */
  target: string;
  language: "shell" | "json" | "toml";
  text: string;
}

/** A tool the server forwards to the editor. */
export interface McpToolRequest {
  id: number;
  tool: string;
  arguments: Record<string, unknown>;
}

export const mcpApi = {
  status: () => invoke<McpStatus>("mcp_status"),
  clientConfigs: () => invoke<McpClientConfig[] | null>("mcp_client_configs"),
  regenerateToken: () => invoke<void>("mcp_regenerate_token"),
  callLog: () => invoke<McpCallLogEntry[]>("mcp_call_log"),
  clearLog: () => invoke<void>("mcp_clear_log"),
  answerApproval: (id: number, answer: McpApproval) => invoke<boolean>("mcp_answer_approval", { id, answer }),
  toolResult: (id: number, answer: { result?: unknown; error?: string }) => invoke<boolean>("mcp_tool_result", { id, answer }),
  disconnect: (session: string) => invoke<boolean>("mcp_disconnect", { session }),
  onStatus: (cb: (s: McpStatus) => void) => listen<McpStatus>(MCP_STATUS_EVENT, cb),
  onCall: (cb: (e: McpCallLogEntry) => void) => listen<McpCallLogEntry>(MCP_CALL_EVENT, cb),
  onApproval: (cb: (r: McpApprovalRequest) => void) => listen<McpApprovalRequest>(MCP_APPROVAL_EVENT, cb),
  onApprovalClosed: (cb: (e: { id: number }) => void) => listen<{ id: number }>(MCP_APPROVAL_CLOSED_EVENT, cb),
  onToolRequest: (cb: (r: McpToolRequest) => void) => listen<McpToolRequest>(MCP_TOOL_REQUEST_EVENT, cb),
};
