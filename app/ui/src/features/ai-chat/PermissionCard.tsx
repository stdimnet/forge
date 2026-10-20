/**
 * Inline approval card for a tool call the agent wants to make (Claude Code's
 * `can_use_tool` request): what would run, as a command, diff or input preview, with
 * Allow once / Allow for this session / Deny. Answered requests collapse to one line.
 */
import { Match, Show, Switch } from "solid-js";
import { Ban, Check, CheckCheck, Clock, ShieldAlert } from "lucide-solid";
import { findChatAgent } from "./agents";
import { DiffView } from "./DiffView";
import type { PermissionBlock } from "./model";
import { answerPermission, chat } from "./store";

const str = (v: unknown) => (typeof v === "string" ? v : "");

const sessionHint = (b: PermissionBlock) =>
  b.toolName === "Bash"
    ? "Don't ask again for this exact command until the chat ends"
    : `Don't ask again for ${b.toolName} until the chat ends`;

function Preview(props: { block: PermissionBlock }) {
  const b = () => props.block;
  return (
    <Switch fallback={<pre class="chat-permission-input">{JSON.stringify(b().input, null, 2)}</pre>}>
      <Match when={b().toolName === "Bash"}>
        <div class="chat-permission-command">
          <pre>
            <span class="chat-permission-prompt">$ </span>
            {str(b().input.command)}
          </pre>
          <Show when={str(b().input.description)}>
            <div class="chat-permission-desc">{str(b().input.description)}</div>
          </Show>
        </div>
      </Match>
      <Match when={b().diff}>
        {(diff) => <DiffView diff={diff()} path={b().path ?? ""} maxLines={14} />}
      </Match>
    </Switch>
  );
}

export function PermissionCard(props: { block: PermissionBlock }) {
  const b = () => props.block;
  const agent = () => findChatAgent(chat.agentId)?.name ?? "The agent";
  /** Headline and short subject, e.g. "run this command" / "edit Order.php" (from the backend). */
  const summary = () => ({ action: b().action, subject: b().subject });
  return (
    <Show
      when={b().status === "pending"}
      fallback={
        <div class="chat-permission-done" data-status={b().status}>
          <Switch>
            <Match when={b().status === "allowed"}>
              <Check size={13} /> Allowed once
            </Match>
            <Match when={b().status === "allowedForSession"}>
              <CheckCheck size={13} /> Allowed for this session
            </Match>
            <Match when={b().status === "denied"}>
              <Ban size={13} /> Denied
            </Match>
            <Match when={b().status === "expired"}>
              <Clock size={13} /> Not answered
            </Match>
          </Switch>
          <span class="chat-ellipsis muted">
            {b().toolName}: {summary().subject}
          </span>
        </div>
      }
    >
      <div class="chat-permission-card" role="alertdialog" aria-label={`${agent()} wants to ${summary().action}`}>
        <div class="chat-permission-head">
          <ShieldAlert size={15} />
          <span>
            Allow {agent()} to <strong>{summary().action}</strong>?
          </span>
        </div>
        <Preview block={b()} />
        <div class="chat-permission-actions">
          <button class="btn-primary" onClick={() => void answerPermission(b().requestId, "once")}>
            Allow once
          </button>
          <button class="btn-secondary" title={sessionHint(b())} onClick={() => void answerPermission(b().requestId, "session")}>
            Allow for this session
          </button>
          <span class="chat-permission-spacer" />
          <button class="btn-secondary" onClick={() => void answerPermission(b().requestId, "deny")}>
            Deny
          </button>
        </div>
      </div>
    </Show>
  );
}
