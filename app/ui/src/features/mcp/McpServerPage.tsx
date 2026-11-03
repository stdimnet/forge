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
          Listening on <span class="mono">{s()!.url}</span> (this computer only)
          <Show when={clientsText(s())}>{(c) => <> · connected: {c()}</>}</Show>
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
        External AI agents such as Claude Code, Claude Desktop, Cursor, Gemini CLI and Codex can connect to Forge and use its tools on the open
        project: read and search files, navigate, see problems, edit in the editor (undoable), run configurations and tests, Git, terminal commands
        and read-only database queries. Projects in Safe Mode, files in .aiignore and projects with AI features turned off stay protected.
      </div>
      <SchemaPage pageId={MCP_PAGE_ID} />
      <Section title="Status">
        <ServerStatusLine />
        <Show when={mcpStatus()?.running}>
          <div class="mcp-row">
            <span class="muted">
              {mcpStatus()?.tokenPersistent
                ? "The bearer token is stored in the system keychain."
                : "The system keychain is unavailable: the token changes when Forge restarts."}
            </span>
            <button class="btn-secondary" disabled={busy()} onClick={() => void regenerate()}>
              <RefreshCw size={13} /> Regenerate Token
            </button>
          </div>
        </Show>
      </Section>
      <Section title="Client Configuration">
        <ClientConfigs />
      </Section>
      <Section title="Call Log">
        <div class="mcp-row">
          <span class="muted">Every tool call of every client, newest first.</span>
          <button class="btn-secondary" disabled={!mcpLog().length} onClick={() => void clearLog()}>
            Clear
          </button>
        </div>
        <CallLog entries={mcpLog()} />
      </Section>
    </div>
  );
}
