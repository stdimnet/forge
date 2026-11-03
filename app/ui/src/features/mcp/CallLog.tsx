/** The MCP call log: time, client, tool, status and duration, newest first. */
import { For, Show } from "solid-js";
import { Ban, Check, CircleX, ShieldOff } from "lucide-solid";
import type { McpCallLogEntry } from "@/ipc/mcp";
import { durationText, timeOf } from "./store";

const STATUS_TITLE: Record<McpCallLogEntry["status"], string> = {
  ok: "Done",
  error: "Failed",
  refused: "Refused by the settings",
  denied: "Denied by you",
};

function StatusIcon(props: { status: McpCallLogEntry["status"] }) {
  return (
    <span class="mcp-log-status" data-status={props.status} title={STATUS_TITLE[props.status]}>
      {props.status === "ok" ? (
        <Check size={13} />
      ) : props.status === "error" ? (
        <CircleX size={13} />
      ) : props.status === "refused" ? (
        <ShieldOff size={13} />
      ) : (
        <Ban size={13} />
      )}
    </span>
  );
}

export function CallLog(props: { entries: McpCallLogEntry[]; limit?: number }) {
  const rows = () => {
    const newest = [...props.entries].reverse();
    return props.limit ? newest.slice(0, props.limit) : newest;
  };
  return (
    <div class="mcp-log" data-testid="mcp-call-log">
      <Show when={rows().length} fallback={<div class="mcp-log-empty muted">No tool calls yet.</div>}>
        <For each={rows()}>
          {(e) => (
            <div class="mcp-log-row" data-tool={e.tool} title={e.message ? `${e.arguments}\n\n${e.message}` : e.arguments}>
              <span class="mcp-log-time muted mono">{timeOf(e.startedAt)}</span>
              <StatusIcon status={e.status} />
              <span class="mcp-log-tool mono">{e.tool}</span>
              <span class="mcp-log-args muted mono">{e.message ?? e.arguments}</span>
              <span class="mcp-log-client muted">{e.client}</span>
              <span class="mcp-log-duration muted">{durationText(e.durationMs)}</span>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}
