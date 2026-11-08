/**
 * MCP server state in the UI: the server status, the call log and the open approval cards,
 * kept up to date from the `mcp://` events.
 */
import { createSignal } from "solid-js";
import {
  mcpApi,
  type McpApproval,
  type McpApprovalRequest,
  type McpCallLogEntry,
  type McpStatus,
} from "@/ipc/mcp";
import { handleToolRequest } from "./editorTools";
import { t } from "@/i18n";

/** Entries the log view keeps. */
export const LOG_LIMIT = 500;

const [status, setStatus] = createSignal<McpStatus | null>(null);
const [log, setLog] = createSignal<McpCallLogEntry[]>([]);
const [approvals, setApprovals] = createSignal<McpApprovalRequest[]>([]);
const [popupOpen, setPopupOpen] = createSignal(false);

export { status as mcpStatus, log as mcpLog, approvals as mcpApprovals, popupOpen, setPopupOpen };

export async function refreshStatus(): Promise<void> {
  setStatus(await mcpApi.status());
}

export async function refreshLog(): Promise<void> {
  setLog(await mcpApi.callLog());
}

export async function clearLog(): Promise<void> {
  await mcpApi.clearLog();
  setLog([]);
}

export async function answerApproval(id: number, answer: McpApproval): Promise<void> {
  setApprovals((list) => list.filter((a) => a.id !== id));
  await mcpApi.answerApproval(id, answer).catch(() => undefined);
}

/** Subscribe to the server's events (once, at startup). */
export async function listenToMcp(): Promise<void> {
  await Promise.all([
    mcpApi.onStatus(setStatus),
    mcpApi.onCall((e) => setLog((list) => [...list, e].slice(-LOG_LIMIT))),
    mcpApi.onApproval((r) => setApprovals((list) => [...list.filter((a) => a.id !== r.id), r])),
    mcpApi.onApprovalClosed(({ id }) => setApprovals((list) => list.filter((a) => a.id !== id))),
    mcpApi.onToolRequest((r) => void handleToolRequest(r)),
  ]);
  await Promise.all([refreshStatus(), refreshLog()]);
}

/** "Claude Code", or "2 clients", or "" with none. */
export function clientsText(s: McpStatus | null): string {
  const n = s?.sessions.length ?? 0;
  if (n === 0) return "";
  if (n === 1) return s!.sessions[0].client.name;
  return t("tools.mcp.clientCount", { count: n });
}

/** The status bar text: "MCP", "MCP: Claude Code", "MCP: port in use". */
export function statusText(s: McpStatus | null): string {
  if (!s?.enabled) return t("tools.mcp.statusOff");
  if (!s.running) return t("tools.mcp.statusNotRunning");
  const clients = clientsText(s);
  return clients ? t("tools.mcp.statusClients", { clients }) : "MCP";
}

/** "12:04:31". */
export function timeOf(ms: number): string {
  const d = new Date(ms);
  return [d.getHours(), d.getMinutes(), d.getSeconds()].map((n) => String(n).padStart(2, "0")).join(":");
}

/** "340 ms", "2.4 s". */
export function durationText(ms: number): string {
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
}
