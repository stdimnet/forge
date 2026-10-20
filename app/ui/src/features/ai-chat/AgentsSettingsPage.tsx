/**
 * Settings > Tools > AI Agents: executable path, default model and permission mode of each
 * agent CLI (schema keys `agents.<id>.*` in `plugins/packs.toml`), with the detected
 * installation shown per agent. The backend uses these values for `agent_list` and
 * `agent_start`.
 */
import { createResource, Match, Show, Switch } from "solid-js";
import { CircleCheck, CircleX, Info, RefreshCw } from "lucide-solid";
import { agentApi, type AgentInfo } from "@/ipc/agent";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { draftValue } from "@/features/settings/dialogState";

/** Section title in the schema → agent id. */
const AGENT_SECTIONS: Record<string, { id: string }> = {
  "Claude Code": { id: "claude-code" },
  "Gemini CLI": { id: "gemini-cli" },
};

/** Paths as currently entered (unsaved edits included), by agent id. */
const enteredPaths = (): Record<string, string> =>
  Object.fromEntries(Object.values(AGENT_SECTIONS).map(({ id }) => [id, draftValue<string>(`agents.${id}.path`) ?? ""]));

export function AgentsSettingsPage() {
  // Re-detect whenever a path changes; the key keeps unrelated edits from refetching.
  const [agents, { refetch }] = createResource(
    () => JSON.stringify(enteredPaths()),
    (key) => agentApi.list(JSON.parse(key) as Record<string, string>).catch((): AgentInfo[] => []),
  );
  const status = (section: string) => {
    const meta = AGENT_SECTIONS[section];
    if (!meta) return null;
    const info = () => agents.latest?.find((a) => a.id === meta.id);
    const custom = () => (enteredPaths()[meta.id] ?? "").trim();
    return (
      <div class="ai-agent-status" data-agent={meta.id}>
        <Switch fallback={<span class="muted">Detecting…</span>}>
          <Match when={info()?.installed && info()}>
            {(a) => (
              <span class="ai-agent-found">
                <CircleCheck size={14} />
                <span>
                  Found <code>{a().path}</code>
                </span>
              </span>
            )}
          </Match>
          <Match when={info() && !info()!.installed}>
            <span class="ai-agent-missing">
              <CircleX size={14} />
              <span>
                {custom() ? "No executable at this path." : "Not found on PATH."} Install with <code>{info()!.installHint}</code>
              </span>
            </span>
          </Match>
        </Switch>
        <Show when={info() && !info()!.supported}>
          <span class="ai-agent-note">
            <Info size={14} />
            <span>Chat sessions with {info()!.name} are not supported yet.</span>
          </span>
        </Show>
        <Show when={info()?.supported && info()!.permissionPrompts === false}>
          <span class="ai-agent-note">
            <Info size={14} />
            <span>{info()!.name} cannot ask before edits or commands in the IDE; the approval mode decides what it may do.</span>
          </span>
        </Show>
        <button class="icon-btn ai-agent-refresh" title="Detect again" onClick={() => void refetch()}>
          <RefreshCw size={13} />
        </button>
      </div>
    );
  };
  return <SchemaPage pageId="tools.ai-agents" sectionHeader={status} />;
}
