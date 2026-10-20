import { For, onMount, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { ClipboardList, Eye, FilePen, Hand, History, MessageSquarePlus, ShieldAlert, Sparkles } from "lucide-solid";
import type { PermissionMode } from "@/ipc/agent";
import { workspace } from "@/state/workspace";
import { chatAgents, findChatAgent } from "./agents";
import { agentModes, chat, detectAgents, newChat, openHistory, setAgent, setMode, setModel } from "./store";
import { Dropdown } from "./Dropdown";
import { MessageList } from "./MessageList";
import { Composer, focusChatInput, setComposerText } from "./Composer";

const MODE_ICONS: Record<PermissionMode, typeof Hand> = {
  default: Hand,
  acceptEdits: FilePen,
  plan: ClipboardList,
  bypassPermissions: ShieldAlert,
};

/** Icon of a mode; an agent that cannot ask is read-only in its default mode. */
const modeIcon = (mode: PermissionMode) =>
  mode === "default" && chat.detected[chat.agentId]?.permissionPrompts === false ? Eye : MODE_ICONS[mode];

function ChatToolbar() {
  const agent = () => findChatAgent(chat.agentId);
  const modes = () => agentModes(chat.agentId);
  const mode = () => modes().find((m) => m.value === chat.mode) ?? modes()[0];
  const modelLabel = () => agent()?.models.find((m) => m.id === chat.model)?.label ?? chat.model;
  return (
    <div class="chat-toolbar">
      <Dropdown
        title="Agent"
        header="Agent"
        value={chat.agentId}
        items={chatAgents().map((a) => ({
          value: a.id,
          label: a.name,
          icon: <Dynamic component={a.icon} size={14} />,
          hint: a.comingSoon
            ? undefined
            : chat.detected[a.id] && !chat.detected[a.id].installed
              ? "Not installed"
              : chat.detected[a.id]?.supported === false
                ? "Not supported yet"
                : undefined,
          badge: a.comingSoon ? "Soon" : undefined,
          disabled: a.comingSoon,
        }))}
        onSelect={(id) => void setAgent(id)}
        trigger={
          <>
            <span class="chat-agent-icon">
              <Dynamic component={agent()?.icon ?? Sparkles} size={13} />
            </span>
            <span class="chat-dropdown-label strong">{agent()?.name ?? chat.agentId}</span>
          </>
        }
      />
      <Dropdown
        title="Model"
        header="Model"
        value={chat.model}
        items={(agent()?.models ?? []).map((m) => ({ value: m.id, label: m.label, hint: m.hint }))}
        onSelect={(m) => void setModel(m)}
        trigger={<span class="chat-dropdown-label">{modelLabel()}</span>}
      />
      <span class="chat-flex" />
      <Dropdown
        title="Permission mode"
        header="Permission mode"
        align="right"
        class={`chat-mode chat-mode-${chat.mode}`}
        value={chat.mode}
        items={modes().map((m) => ({ value: m.value, label: m.label, hint: m.hint, icon: <Dynamic component={modeIcon(m.value)} size={14} /> }))}
        onSelect={(m) => void setMode(m)}
        trigger={
          <>
            <Dynamic component={modeIcon(mode()?.value ?? "default")} size={12} />
            <span class="chat-dropdown-label">{mode()?.short ?? chat.mode}</span>
          </>
        }
      />
    </div>
  );
}

function EmptyState() {
  const agent = () => findChatAgent(chat.agentId);
  const info = () => chat.detected[chat.agentId];
  const suggestions = [
    "Explain how orders are paid and refunded",
    "Add a scope for paid orders to Order model",
    "Write a feature test for OrderController::show",
  ];
  return (
    <div class="chat-empty">
      <div class="chat-empty-logo">
        <Dynamic component={agent()?.icon ?? Sparkles} size={26} stroke-width={1.6} />
      </div>
      <div class="chat-empty-title">{agent()?.name ?? "AI Chat"}</div>
      <div class="chat-empty-text">
        {agent()?.tagline ?? "Chat with an AI agent about your project."}
        <Show when={workspace()}>
          {(ws) => (
            <>
              {" "}
              Working in <strong>{ws().name}</strong>.
            </>
          )}
        </Show>
      </div>
      <Show when={info()}>
        {(i) => (
          <div class="chat-empty-status" classList={{ missing: !i().installed }}>
            <span class="chat-status-dot" />
            <Show
              when={i().installed}
              fallback={
                <span>
                  {i().name} not found on PATH. Install it with <code>{i().installHint}</code> and sign in once in a terminal, or
                  set its path in Settings | Tools | AI Agents.
                </span>
              }
            >
              <span class="chat-ellipsis" title={i().path ?? ""}>
                {/\bCLI$/.test(i().name) ? i().name : `${i().name} CLI`} ready · uses your own login
              </span>
            </Show>
          </div>
        )}
      </Show>
      <div class="chat-suggestions">
        <For each={suggestions}>
          {(s) => (
            <button
              class="chat-suggestion"
              onClick={() => {
                setComposerText(s);
                focusChatInput();
              }}
            >
              {s}
            </button>
          )}
        </For>
      </div>
    </div>
  );
}

export function ChatPanel() {
  onMount(() => void detectAgents());
  return (
    <div class="chat-panel">
      <ChatToolbar />
      <Show when={chat.conv.turns.length} fallback={<EmptyState />}>
        <MessageList />
      </Show>
      <Composer />
    </div>
  );
}

/** Tool window header actions: New chat + history. */
export function ChatActions() {
  const ago = (t: number) => {
    const m = Math.round((Date.now() - t) / 60000);
    return m < 1 ? "just now" : m < 60 ? `${m} min ago` : `${Math.round(m / 60)} h ago`;
  };
  return (
    <>
      <button class="icon-btn" title="New chat" onClick={() => void newChat()}>
        <MessageSquarePlus size={15} />
      </button>
      <Dropdown
        class="chat-history"
        title="Chat history"
        header="Recent chats"
        align="right"
        hideChevron
        empty="No previous chats yet"
        items={chat.history.map((h) => ({ value: h.id, label: h.title, hint: `${findChatAgent(h.agentId)?.name ?? h.agentId} · ${ago(h.at)}` }))}
        onSelect={(id) => void openHistory(id)}
        trigger={<History size={15} />}
      />
    </>
  );
}
