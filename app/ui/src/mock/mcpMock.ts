/**
 * Browser-mode MCP server: follows the `mcp.enabled` / `mcp.port` settings, shows Claude
 * Code and Cursor as connected clients with a few logged calls, and builds the client
 * configurations with the Rust code (WebAssembly). `?mcp` turns the server on at startup;
 * `?mcpApproval` also shows an approval card. Nothing listens on a port and no project file
 * is touched.
 */
import { emitMock, listen, registerMock } from "@/ipc";
import {
  MCP_APPROVAL_CLOSED_EVENT,
  MCP_APPROVAL_EVENT,
  MCP_CALL_EVENT,
  MCP_STATUS_EVENT,
  type McpApprovalRequest,
  type McpCallLogEntry,
  type McpSession,
  type McpStatus,
} from "@/ipc/mcp";
import { SETTINGS_CHANGED_EVENT } from "@/ipc/settings";
import { callWasm, forgeWasm } from "@/wasm";
import { mockSettingValue, setSetting } from "./settingsMock";

const params = () => (typeof location !== "undefined" ? new URLSearchParams(location.search) : new URLSearchParams());
const now = () => Date.now();
const startedAt = now();

let token = "3f9c2a7e5b1d4c8a9e0f6b2d7c4a1e8f3b9d0c6a5e2f7b1c4d8a9e3f0b6c2d7a";
let sessions: McpSession[] = [
  { id: "s-claude", client: { name: "Claude Code", version: "2.0.14" }, protocolVersion: "2025-06-18", transport: "http", connectedAt: startedAt - 6 * 60_000, lastSeenAt: startedAt - 4_000, calls: 7 },
  { id: "s-cursor", client: { name: "Cursor", version: "1.7.2" }, protocolVersion: "2025-03-26", transport: "http", connectedAt: startedAt - 25 * 60_000, lastSeenAt: startedAt - 90_000, calls: 2 },
];

const entry = (id: number, ago: number, client: string, tool: string, args: unknown, status: McpCallLogEntry["status"], durationMs: number, message?: string): McpCallLogEntry => ({
  id,
  session: client === "Cursor" ? "s-cursor" : "s-claude",
  client,
  tool,
  arguments: JSON.stringify(args),
  status,
  message: message ?? null,
  startedAt: startedAt - ago,
  durationMs,
});

let log: McpCallLogEntry[] = [
  entry(1, 24 * 60_000, "Cursor", "get_current_file", {}, "ok", 18),
  entry(2, 23 * 60_000, "Cursor", "search_in_files", { query: "OrderStatus::Shipped" }, "ok", 142),
  entry(3, 5 * 60_000, "Claude Code", "get_open_editors", {}, "ok", 9),
  entry(4, 5 * 60_000 - 3_000, "Claude Code", "read_file", { path: "app/Http/Controllers/OrderController.php" }, "ok", 21),
  entry(5, 4 * 60_000, "Claude Code", "find_usages", { path: "app/Models/Order.php", line: 18, column: 21 }, "ok", 388),
  entry(6, 3 * 60_000, "Claude Code", "read_file", { path: ".env" }, "refused", 2, ".env is excluded from AI by .aiignore"),
  entry(7, 2 * 60_000, "Claude Code", "replace_text_in_file", { path: "app/Models/Order.php", oldText: "'shipped'", newText: "OrderStatus::Shipped" }, "ok", 2_412),
  entry(8, 90_000, "Claude Code", "get_problems", { path: "app/Models/Order.php" }, "ok", 14),
  entry(9, 40_000, "Claude Code", "run_configuration", { name: "OrderTest" }, "denied", 5_830, "The user denied run_configuration in Forge."),
];
let nextId = 100;

const enabled = () => mockSettingValue("mcp.enabled") === true;
const port = () => Number(mockSettingValue("mcp.port") ?? 64342) || 64342;

function status(): McpStatus {
  const on = enabled();
  return {
    enabled: on,
    running: on,
    port: on ? port() : null,
    url: on ? `http://127.0.0.1:${port()}/mcp` : null,
    error: null,
    sessions: on ? sessions : [],
    tokenPersistent: true,
    safeMode: false,
    aiDisabled: mockSettingValue("ai.enabled") === false,
  };
}

const emitStatus = () => emitMock(MCP_STATUS_EVENT, status());

void listen<{ key: string }>(SETTINGS_CHANGED_EVENT, (c) => {
  if (c.key === "mcp.enabled" || c.key === "mcp.port" || c.key === "ai.enabled") emitStatus();
});

registerMock("mcp_status", () => status());

registerMock("mcp_client_configs", async () => {
  const s = status();
  if (!s.url) return null;
  return callWasm((await forgeWasm()).mcp_client_configs, { url: s.url, token, exe: "/opt/Forge/forge" });
});

registerMock("mcp_regenerate_token", () => {
  token = Array.from({ length: 64 }, () => "0123456789abcdef"[Math.floor(Math.random() * 16)]).join("");
  sessions = [];
  emitStatus();
});

let demoApprovalShown = false;
registerMock("mcp_call_log", () => {
  // The UI asks for the log once it listens to the events: show the `?mcpApproval` card then.
  if (params().has("mcpApproval") && !demoApprovalShown) {
    demoApprovalShown = true;
    setTimeout(showDemoApproval, 300);
  }
  return [...log];
});
registerMock("mcp_clear_log", () => {
  log = [];
});

const pending = new Map<number, McpApprovalRequest>();

registerMock("mcp_answer_approval", ({ id, answer }) => {
  const req = pending.get(Number(id));
  if (!req) return false;
  pending.delete(req.id);
  const allowed = answer !== "deny";
  const e = entry(nextId++, 0, req.client, req.tool, req.arguments, allowed ? "ok" : "denied", 1_200, allowed ? undefined : `The user denied ${req.tool} in Forge.`);
  e.startedAt = now();
  log = [...log, e];
  emitMock(MCP_CALL_EVENT, e);
  return true;
});

registerMock("mcp_tool_result", () => true);

registerMock("mcp_disconnect", ({ session }) => {
  const before = sessions.length;
  sessions = sessions.filter((s) => s.id !== session);
  emitStatus();
  return sessions.length !== before;
});

/** Show an approval card like a client's write call would (`?mcpApproval`, tests). */
export function showDemoApproval(): McpApprovalRequest {
  const req: McpApprovalRequest = {
    id: nextId++,
    session: "s-claude",
    client: "Claude Code",
    tool: "execute_terminal_command",
    title: "Execute Terminal Command",
    group: "terminal",
    kind: "exec",
    summary: "run a terminal command",
    arguments: { command: "php artisan test --filter=OrderTest", timeoutSeconds: 120 },
    sessionAllowed: false,
  };
  pending.set(req.id, req);
  emitMock(MCP_APPROVAL_EVENT, req);
  return req;
}

/** Close a card nobody answered. */
export function expireDemoApproval(id: number): void {
  if (pending.delete(id)) emitMock(MCP_APPROVAL_CLOSED_EVENT, { id });
}

if (params().has("mcp") || params().has("mcpApproval")) {
  void forgeWasm().then(() => {
    setSetting("mcp.enabled", true, "user");
    emitStatus();
  });
}
