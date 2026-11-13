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
import { t } from "@/i18n";

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
        <StatusPopup anchor={el!} align="end" class="mcp-popup" label={t("tools.mcp.mcpServer")} testId="mcp-popup" onClose={() => setPopupOpen(false)}>
          <div class="mcp-popup-title">
            <PlugZap size={14} /> {t("tools.mcp.mcpServer")}
            <span class="muted mono mcp-popup-url">{s()?.url ?? t("tools.mcp.notRunning")}</span>
            <button class="icon-btn" title={t("tools.mcp.mcpServerSettings")} onClick={() => (setPopupOpen(false), openSettings(MCP_PAGE_ID))}>
              <Settings size={14} />
            </button>
          </div>
          <Show when={s()?.safeMode}>
            <div class="mcp-popup-note">{t("tools.mcp.safeModeRunConfigurationsAnd")}</div>
          </Show>
          <Show when={s()?.aiDisabled}>
            <div class="mcp-popup-note">{t("tools.mcp.aiFeaturesAreOffFor")}</div>
          </Show>
          <div class="mcp-popup-label muted">{t("tools.mcp.connectedClients")}</div>
          <For each={s()?.sessions ?? []} fallback={<div class="muted mcp-popup-empty">{t("tools.mcp.noneCopyAClientConfiguration")}</div>}>
            {(session) => (
              <div class="mcp-session" data-session={session.id}>
                <span class="mcp-dot" />
                <span class="mcp-session-name">{session.client.name}</span>
                <span class="muted">
                  {t("tools.mcp.sinceCalls", { value: session.transport === "sse" ? "SSE" : "HTTP", value2: timeOf(session.connectedAt), calls: session.calls })}
                </span>
                <button class="icon-btn" title={t("tools.mcp.disconnect")} onClick={() => void mcpApi.disconnect(session.id)}>
                  <Unplug size={13} />
                </button>
              </div>
            )}
          </For>
          <div class="mcp-popup-label muted mcp-popup-row">
            <span>{t("tools.mcp.recentToolCalls")}</span>
            <Show when={mcpLog().length}>
              <button class="mcp-link" onClick={() => void clearLog()}>
                {t("common.clear")}
              </button>
            </Show>
          </div>
          <CallLog entries={mcpLog()} limit={12} />
        </StatusPopup>
      </Show>
    </div>
  );
}
