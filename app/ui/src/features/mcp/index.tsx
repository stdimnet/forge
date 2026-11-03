/**
 * MCP Server: Forge as a Model Context Protocol server for external agents (Claude Code,
 * Claude Desktop, Cursor, Gemini CLI, Codex). The server runs in Rust (`crates/forge-mcp`,
 * `commands/mcp.rs`); the UI has its settings page, the status bar widget with the call
 * log, the approval cards and the editor tools Rust forwards.
 */
import { registerSettingsPage } from "@/features/settingsPages";
import { registerStatusItem } from "@/features/statusItems";
import { openSettings } from "@/features/settings/dialogState";
import { registerCommand } from "@/state/commands";
import { McpServerPage } from "./McpServerPage";
import { MCP_PAGE_ID, McpStatusItem } from "./McpStatusItem";
import { listenToMcp, mcpStatus, setPopupOpen } from "./store";
import "./mcp.css";

registerSettingsPage({
  id: MCP_PAGE_ID,
  title: "MCP Server",
  group: "Tools",
  order: 12,
  keywords: ["MCP", "Model Context Protocol", "Claude Code", "Claude Desktop", "Cursor", "Gemini CLI", "Codex", "agent", "token", "approval"],
  component: McpServerPage,
});

registerStatusItem({ id: "mcp.server", align: "right", order: -30, component: McpStatusItem, when: () => !!mcpStatus()?.enabled });

registerCommand({
  id: "Mcp.Settings",
  title: "MCP Server Settings",
  category: "Tools",
  run: () => openSettings(MCP_PAGE_ID),
});

registerCommand({
  id: "Mcp.ShowLog",
  title: "Show MCP Server Calls",
  category: "Tools",
  run: () => void setPopupOpen(true),
});

void listenToMcp().catch((e) => console.warn("The MCP server is unavailable:", e));
