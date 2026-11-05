/**
 * Approval cards for MCP tool calls that edit files, run configurations, run terminal
 * commands or query databases: what the client wants to do, with Allow once / Allow for
 * this session / Deny. Rust waits for the answer (and denies when nobody answers).
 */
import { For, Show } from "solid-js";
import { Layer } from "@/shell/layers";
import { Ban, Check, CheckCheck, PlugZap } from "lucide-solid";
import type { McpApprovalRequest } from "@/ipc/mcp";
import { answerApproval, mcpApprovals } from "./store";
import { t } from "@/i18n";

const str = (v: unknown) => (typeof v === "string" ? v : "");

/** The part of the arguments worth showing: the command, the query, the replaced text. */
function Preview(props: { req: McpApprovalRequest }) {
  const a = () => props.req.arguments;
  return (
    <Show
      when={str(a().command) || str(a().sql)}
      fallback={
        <Show when={str(a().oldText) || str(a().text)}>
          <div class="mcp-approval-diff mono">
            <Show when={str(a().oldText)}>
              <pre class="mcp-diff-old">{str(a().oldText)}</pre>
            </Show>
            <pre class="mcp-diff-new">{str(a().newText) || str(a().text)}</pre>
          </div>
        </Show>
      }
    >
      <pre class="mcp-approval-command mono">
        <Show when={str(a().command)}>
          <span class="muted">$ </span>
        </Show>
        {str(a().command) || str(a().sql)}
      </pre>
    </Show>
  );
}

function Card(props: { req: McpApprovalRequest }) {
  const r = () => props.req;
  return (
    <div class="mcp-approval" role="alertdialog" aria-label={t("tools.mcp.wantsTo", { client: r().client, summary: r().summary })} data-tool={r().tool}>
      <div class="mcp-approval-head">
        <PlugZap size={15} />
        <span>
          <strong>{r().client}</strong> {t("tools.mcp.wantsTo2")} <strong>{r().summary}</strong>
        </span>
      </div>
      <div class="muted mcp-approval-tool">
        {t("tools.mcp.mcpTool")} <span class="mono">{r().tool}</span>
      </div>
      <Preview req={r()} />
      <div class="mcp-approval-actions">
        <button class="btn-primary" onClick={() => void answerApproval(r().id, "once")}>
          <Check size={13} /> {t("tools.mcp.allow")}
        </button>
        <Show when={r().sessionAllowed}>
          <button class="btn-secondary" title={t("tools.mcp.donTAskAgainFor", { tool: r().tool, client: r().client })} onClick={() => void answerApproval(r().id, "session")}>
            <CheckCheck size={13} /> {t("tools.mcp.allowForThisSession")}
          </button>
        </Show>
        <button class="btn-secondary" onClick={() => void answerApproval(r().id, "deny")}>
          <Ban size={13} /> {t("tools.mcp.deny")}
        </button>
      </div>
    </div>
  );
}

export function ApprovalCards() {
  return (
    <Show when={mcpApprovals().length}>
      <Layer kind="notification">
        <div class="mcp-approvals" data-testid="mcp-approvals">
          <For each={mcpApprovals()}>{(req) => <Card req={req} />}</For>
        </div>
      </Layer>
    </Show>
  );
}
