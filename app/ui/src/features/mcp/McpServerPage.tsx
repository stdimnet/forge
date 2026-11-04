/**
 * Settings | Tools | MCP Server: the server switch, port, approval and tool groups (schema
 * settings), the server status, "Copy client configuration", the token and the call log.
 */
import { createSignal, Show } from "solid-js";
import { CircleAlert, RefreshCw } from "lucide-solid";
import { mcpApi } from "@/ipc/mcp";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { CallLog } from "./CallLog";
import { ClientConfigs } from "./ClientConfigs";
import { MCP_PAGE_ID } from "./McpStatusItem";
import { clearLog, clientsText, mcpLog, mcpStatus, refreshStatus } from "./store";
import { t } from "@/i18n";

function Section(props: { title: string; children: unknown }) {
  return (
    <section class="set-section">
      <div class="set-section-title">
        <span>{props.title}</span>
      </div>
      <div class="mcp-section-body">{props.children as never}</div>
    </section>
  );
}

function ServerStatusLine() {
  const s = () => mcpStatus();
  return (
    <div class="mcp-server-status" data-testid="mcp-server-status">
      <Show when={s()?.running} fallback={<Show when={s()?.error} fallback={<span class="mcp-dot off" />}>{<CircleAlert size={14} class="mcp-error-icon" />}</Show>}>
        <span class="mcp-dot" />
      </Show>
      <span>
        <Show when={s()?.running} fallback={s()?.error ?? "Not running. Enable it above and click Apply."}>
          {t("tools.mcp.listeningOn")} <span class="mono">{s()!.url}</span> {t("tools.mcp.thisComputerOnly")}
          <Show when={clientsText(s())}>{(c) => <> {t("tools.mcp.connected", { c: c() })}</>}</Show>
        </Show>
      </span>
    </div>
  );
}

export function McpServerPage() {
  const [busy, setBusy] = createSignal(false);
  const regenerate = async () => {
    setBusy(true);
    try {
      await mcpApi.regenerateToken();
      await refreshStatus();
    } finally {
      setBusy(false);
    }
  };
  return (
    <div class="mcp-page" data-testid="mcp-page">
      <div class="muted mcp-intro">
        {t("tools.mcp.externalAiAgentsSuchAs")}
      </div>
      <SchemaPage pageId={MCP_PAGE_ID} />
      <Section title={t("common.status")}>
        <ServerStatusLine />
        <Show when={mcpStatus()?.running}>
          <div class="mcp-row">
            <span class="muted">
              {mcpStatus()?.tokenPersistent
                ? t("tools.mcp.theBearerTokenIsStored")
                : t("tools.mcp.theSystemKeychainIsUnavailable")}
            </span>
            <button class="btn-secondary" disabled={busy()} onClick={() => void regenerate()}>
              <RefreshCw size={13} /> {t("tools.mcp.regenerateToken")}
            </button>
          </div>
        </Show>
      </Section>
      <Section title={t("tools.mcp.clientConfiguration")}>
        <ClientConfigs />
      </Section>
      <Section title={t("tools.mcp.callLog")}>
        <div class="mcp-row">
          <span class="muted">{t("tools.mcp.everyToolCallOfEvery")}</span>
          <button class="btn-secondary" disabled={!mcpLog().length} onClick={() => void clearLog()}>
            {t("common.clear")}
          </button>
        </div>
        <CallLog entries={mcpLog()} />
      </Section>
    </div>
  );
}
