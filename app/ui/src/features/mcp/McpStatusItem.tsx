/**
 * Status bar widget of the MCP server: "MCP" with the connected clients. The popup lists
 * the sessions (with Disconnect), the latest tool calls and the client configurations.
 */
import { For, Show } from "solid-js";
import { PlugZap, Settings, Unplug } from "lucide-solid";
import { mcpApi } from "@/ipc/mcp";
import { openSettings } from "@/features/settings/dialogState";
import { StatusPopup } from "@/shell/StatusPopup";
import { ApprovalCards } from "./ApprovalCards";
import { CallLog } from "./CallLog";
import { clearLog, mcpLog, mcpStatus, popupOpen, setPopupOpen, statusText, timeOf } from "./store";

export const MCP_PAGE_ID = "tools.mcp-server";

export function McpStatusItem() {
  let el: HTMLDivElement | undefined;
  const s = () => mcpStatus();
  const title = () =>
    s()?.running
      ? `MCP server on ${s()!.url}${s()!.sessions.length ? ` · ${s()!.sessions.length} connected` : " · no clients connected"}`
      : (s()?.error ?? "MCP server is not running");
  return (
    <div class="mcp-status-wrap" ref={el}>
      <button
        class="status-item mcp-status"
        classList={{ connected: !!s()?.sessions.length, problem: !!s()?.error }}
        data-testid="mcp-status"
        title={title()}
        onClick={() => setPopupOpen(!popupOpen())}
      >
        <PlugZap size={12} />
        <span>{statusText(s())}</span>
      </button>
      <ApprovalCards />
      <Show when={popupOpen()}>
        <StatusPopup anchor={el!} align="end" class="mcp-popup" label="MCP Server" testId="mcp-popup" onClose={() => setPopupOpen(false)}>
          <div class="mcp-popup-title">
            <PlugZap size={14} /> MCP Server
            <span class="muted mono mcp-popup-url">{s()?.url ?? "not running"}</span>
            <button class="icon-btn" title="MCP Server Settings" onClick={() => (setPopupOpen(false), openSettings(MCP_PAGE_ID))}>
              <Settings size={14} />
            </button>
          </div>
          <Show when={s()?.safeMode}>
            <div class="mcp-popup-note">Safe Mode: run configurations and terminal commands are refused until you trust the project.</div>
          </Show>
          <Show when={s()?.aiDisabled}>
            <div class="mcp-popup-note">AI features are off for this project: every tool call is refused.</div>
          </Show>
          <div class="mcp-popup-label muted">Connected clients</div>
          <For each={s()?.sessions ?? []} fallback={<div class="muted mcp-popup-empty">None. Copy a client configuration in Settings | Tools | MCP Server.</div>}>
            {(session) => (
              <div class="mcp-session" data-session={session.id}>
                <span class="mcp-dot" />
                <span class="mcp-session-name">{session.client.name}</span>
                <span class="muted">
                  {session.transport === "sse" ? "SSE" : "HTTP"} · since {timeOf(session.connectedAt)} · {session.calls} calls
                </span>
                <button class="icon-btn" title="Disconnect" onClick={() => void mcpApi.disconnect(session.id)}>
                  <Unplug size={13} />
                </button>
              </div>
            )}
          </For>
          <div class="mcp-popup-label muted mcp-popup-row">
            <span>Recent tool calls</span>
            <Show when={mcpLog().length}>
              <button class="mcp-link" onClick={() => void clearLog()}>
                Clear
              </button>
            </Show>
          </div>
          <CallLog entries={mcpLog()} limit={12} />
        </StatusPopup>
      </Show>
    </div>
  );
}
