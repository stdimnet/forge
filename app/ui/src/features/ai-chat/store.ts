/**
 * AI chat state: selected agent/model/mode, the current conversation and the project's chat
 * history. Chats are saved per project in the user data dir (`agent_history_*`, never in the
 * project) after every turn, listed newest first and reopened from the history popup.
 */
import { createEffect, createRoot, on, untrack } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { getSetting } from "@/state/settings";
import {
  agentApi,
  type AgentEvent,
  type AgentInfo,
  type MessageContext,
  type PermissionDecision,
  type PermissionMode,
  type PermissionModeInfo,
} from "@/ipc/agent";
import { projectStateApi, type ChatSummary } from "@/ipc/projectState";
import { markdownApi } from "@/ipc/markdown";
import { reloadFile } from "@/state/editor";
import { workspace } from "@/state/workspace";
import { addTurn, applyChatUpdate, emptyConversation, failTurn, findPermission, type Conversation } from "./model";

export type HistoryEntry = ChatSummary;

interface ChatState {
  agentId: string;
  model: string;
  mode: PermissionMode;
  conv: Conversation;
  running: boolean;
  /** Backend session id of the running CLI process. */
  sessionId: string | null;
  /** History id of the current conversation (set when it is first saved). */
  chatId: string | null;
  /** Saved chats of the open project, newest first. */
  history: HistoryEntry[];
  /** Detection results from the backend, by agent id. */
  detected: Record<string, AgentInfo>;
}

const [chat, setChat] = createStore<ChatState>({
  agentId: "claude-code",
  model: "default",
  mode: "default",
  conv: emptyConversation(),
  running: false,
  sessionId: null,
  chatId: null,
  history: [],
  detected: {},
});

export { chat };

/** Claude Code's modes, used until the backend reported an agent's own. */
const FALLBACK_MODES: PermissionModeInfo[] = [
  { value: "default", label: "Ask before edits", short: "Ask before edits", hint: "Confirm every edit and command" },
  { value: "acceptEdits", label: "Auto-accept edits", short: "Auto-accept", hint: "Apply file edits without asking" },
  { value: "plan", label: "Plan mode", short: "Plan", hint: "Explore and plan, no changes" },
  { value: "bypassPermissions", label: "Bypass permissions", short: "Bypass", hint: "Never ask — trusted projects only" },
];

/** The permission modes an agent supports, as its CLI understands them (from the backend profile). */
export function agentModes(agentId: string): PermissionModeInfo[] {
  return chat.detected[agentId]?.permissionModes ?? FALLBACK_MODES;
}

/** Default model and permission mode of an agent (Settings > Tools > AI Agents). */
export function agentDefaults(agentId: string): { model: string; mode: PermissionMode } {
  const model = getSetting<string>(`agents.${agentId}.model`, "default") || "default";
  const mode = getSetting<string>(`agents.${agentId}.permissionMode`, "default") as PermissionMode;
  const modes = agentModes(agentId);
  return { model, mode: modes.some((m) => m.value === mode) ? mode : (modes[0]?.value ?? "default") };
}

// A fresh chat follows the configured defaults (also once settings have loaded).
createRoot(() =>
  createEffect(() => {
    const d = agentDefaults(chat.agentId);
    if (untrack(() => !chat.sessionId && !chat.conv.turns.length)) setChat({ model: d.model, mode: d.mode });
  }),
);

let unlisten: (() => void) | undefined;

export async function detectAgents(): Promise<void> {
  try {
    const list = await agentApi.list();
    setChat("detected", Object.fromEntries(list.map((a) => [a.id, a])));
  } catch (e) {
    console.warn("agent detection failed", e);
  }
}

function onEvent(ev: AgentEvent): void {
  if (ev.type === "chat") {
    setChat("conv", produce((conv) => applyChatUpdate(conv, ev)));
    if (ev.turnEnded) {
      setChat("running", false);
      void saveChat();
    }
    for (const path of ev.changedFiles) void reloadFile(path);
    return;
  }
  // An `exited` process needs nothing here: the backend session spawns a new process for
  // the next message and resumes the conversation (per-turn CLIs like Gemini CLI exit after
  // every turn).
}

async function ensureSession(): Promise<string> {
  if (chat.sessionId) return chat.sessionId;
  const id = await agentApi.start(chat.agentId, {
    model: chat.model,
    permissionMode: chat.mode,
    resume: chat.conv.conversationId ?? undefined,
  });
  unlisten = await agentApi.onEvent(id, onEvent);
  setChat("sessionId", id);
  return id;
}

export async function sendMessage(text: string, context: MessageContext): Promise<void> {
  if (chat.running || !text.trim()) return;
  let turnId = 0;
  setChat("conv", produce((conv) => void (turnId = addTurn(conv, text.trim(), context).id)));
  setChat("running", true);
  void saveChat();
  try {
    const id = await ensureSession();
    await agentApi.send(id, text.trim(), context, turnId);
  } catch (e) {
    setChat("conv", produce((conv) => failTurn(conv, String(e instanceof Error ? e.message : e))));
    setChat("running", false);
    void saveChat();
  }
}

/** A turn answered by an AI provider instead of the agent CLI ("Explain with AI"). */
export interface ProviderTurn {
  append(delta: string): void;
  /** `note` goes to the turn footer (provider and model). */
  finish(note?: string): void;
  fail(message: string): void;
}

/**
 * Add a turn whose answer streams from an AI provider (`forge-ai`), e.g. an explanation
 * requested in the editor. `null` while the agent is working on a turn.
 */
export function startProviderTurn(prompt: string, context: MessageContext): ProviderTurn | null {
  if (chat.running) return null;
  let turnId = 0;
  setChat("conv", produce((conv) => void (turnId = addTurn(conv, prompt, context).id)));
  const started = Date.now();
  let text = "";
  let parsing = false;
  let dirty = false;
  let ended = false;
  const update = (fn: (turn: Conversation["turns"][number]) => void) =>
    setChat(
      "conv",
      produce((conv) => {
        const turn = conv.turns.find((t) => t.id === turnId);
        if (turn) fn(turn);
      }),
    );
  // Markdown is parsed in Rust; while a parse runs, newer text waits for the next one.
  const render = async () => {
    if (parsing) {
      dirty = true;
      return;
    }
    parsing = true;
    do {
      dirty = false;
      const snapshot = text;
      const markdown = await markdownApi.parse(snapshot, "chat").catch(() => []);
      update((turn) => {
        const block = { kind: "text" as const, text: snapshot, streaming: !ended, markdown };
        if (turn.blocks[0]?.kind === "text") turn.blocks[0] = block;
        else turn.blocks.unshift(block);
      });
    } while (dirty);
    parsing = false;
  };
  return {
    append(delta) {
      text += delta;
      update((turn) => void (turn.working = false));
      void render();
    },
    finish(note) {
      ended = true;
      void render().then(() => {
        update((turn) => {
          turn.status = "done";
          turn.working = false;
          const secs = ((Date.now() - started) / 1000).toFixed(1);
          turn.summary = { durationMs: Date.now() - started, numTurns: 1, costUsd: null, usage: null, durationText: `${secs}s`, tokensText: note };
        });
        void saveChat();
      });
    },
    fail(message) {
      ended = true;
      setChat("conv", produce((conv) => failTurn(conv, message)));
      void saveChat();
    },
  };
}

/** Answer a permission request shown in the chat (Allow once / for the session / Deny). */
export async function answerPermission(requestId: string, answer: "once" | "session" | "deny"): Promise<void> {
  const id = chat.sessionId;
  if (!id || findPermission(chat.conv, requestId)?.status !== "pending") return;
  const decision: PermissionDecision =
    answer === "deny" ? { behavior: "deny", message: "The user denied this tool call." } : { behavior: "allow", scope: answer };
  // The backend marks the request answered in its next `chat` update.
  try {
    await agentApi.respondPermission(id, requestId, decision);
  } catch (e) {
    const message = String(e instanceof Error ? e.message : e);
    setChat("conv", produce((conv) => void conv.turns[conv.turns.length - 1]?.blocks.push({ kind: "error", message })));
  }
}

export async function interrupt(): Promise<void> {
  if (chat.sessionId && chat.running) await agentApi.interrupt(chat.sessionId);
}

/** Stop the CLI process; the conversation is kept and resumed on the next message. */
async function endSession(): Promise<void> {
  const id = chat.sessionId;
  unlisten?.();
  unlisten = undefined;
  setChat({ sessionId: null, running: false });
  if (id) await agentApi.stop(id).catch(() => undefined);
}

const newChatId = () => `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

/** Put a saved chat into the history list (newest first). */
function remember(entry: ChatSummary): void {
  setChat("history", (h) => [entry, ...h.filter((x) => x.id !== entry.id)].sort((a, b) => b.at - a.at));
}

let saving: Promise<void> = Promise.resolve();

/** Save the current conversation to the project's chat history (empty ones are not kept). */
export function saveChat(): Promise<void> {
  if (!chat.conv.turns.length || !workspace()) return saving;
  const id = chat.chatId ?? newChatId();
  if (!chat.chatId) setChat("chatId", id);
  const record = {
    id,
    agentId: chat.agentId,
    at: Date.now(),
    title: "",
    conversation: JSON.parse(JSON.stringify(chat.conv)) as Conversation,
  };
  // One save at a time, in order.
  saving = saving
    .then(() => projectStateApi.saveChat(record))
    .then((entry) => {
      if (entry) remember(entry);
    })
    .catch((e) => console.warn("the chat could not be saved", e));
  return saving;
}

export async function newChat(): Promise<void> {
  await endSession();
  await saveChat();
  setChat({ conv: emptyConversation(), chatId: null });
}

/** Turns that were still running when the chat was saved cannot continue. */
function settle(conv: Conversation): Conversation {
  for (const turn of conv.turns) {
    if (turn.status === "running") Object.assign(turn, { status: "interrupted", working: false, awaitingPermission: false });
  }
  return conv;
}

export async function openHistory(id: string): Promise<void> {
  if (id === chat.chatId) return;
  const record = await projectStateApi.loadChat(id).catch((e) => {
    console.warn("the chat could not be loaded", e);
    return null;
  });
  if (!record) {
    setChat("history", (h) => h.filter((x) => x.id !== id));
    return;
  }
  await endSession();
  await saveChat();
  const conv = settle(record.conversation as Conversation);
  setChat({ agentId: record.agentId, conv, chatId: record.id, ...agentDefaults(record.agentId) });
}

// Each project has its own chats: switching projects starts a new chat and loads its history.
createRoot(() =>
  createEffect(
    on(
      () => workspace()?.root,
      async (root) => {
        await endSession();
        setChat({ conv: emptyConversation(), chatId: null, history: [] });
        if (!root) return;
        const list = await projectStateApi.listChats().catch(() => [] as ChatSummary[]);
        if (workspace()?.root === root) setChat("history", list);
      },
    ),
  ),
);

export async function setAgent(agentId: string): Promise<void> {
  if (agentId === chat.agentId) return;
  await newChat();
  setChat({ agentId, ...agentDefaults(agentId) });
}

export async function setModel(model: string): Promise<void> {
  if (model === chat.model) return;
  await endSession();
  setChat("model", model);
}

export async function setMode(mode: PermissionMode): Promise<void> {
  if (mode === chat.mode) return;
  await endSession();
  setChat("mode", mode);
}
