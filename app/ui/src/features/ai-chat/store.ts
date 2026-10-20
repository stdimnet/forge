/** AI chat state: selected agent/model/mode, the current conversation and history. */
import { createEffect, createRoot, untrack } from "solid-js";
import { createStore, produce } from "solid-js/store";
import { getSetting } from "@/state/settings";
import {
  agentApi,
  type AgentEvent,
  type AgentInfo,
  type MessageContext,
  type PermissionDecision,
  type PermissionMode,
} from "@/ipc/agent";
import { reloadFile } from "@/state/editor";
import { addTurn, applyChatUpdate, emptyConversation, failTurn, findPermission, type Conversation } from "./model";

export interface HistoryEntry {
  id: number;
  title: string;
  agentId: string;
  at: number;
  conv: Conversation;
}

interface ChatState {
  agentId: string;
  model: string;
  mode: PermissionMode;
  conv: Conversation;
  running: boolean;
  /** Backend session id of the running CLI process. */
  sessionId: string | null;
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
  history: [],
  detected: {},
});

export { chat };

const PERMISSION_MODES: PermissionMode[] = ["default", "acceptEdits", "plan", "bypassPermissions"];

/** Default model and permission mode of an agent (Settings > Tools > AI Agents). */
export function agentDefaults(agentId: string): { model: string; mode: PermissionMode } {
  const model = getSetting<string>(`agents.${agentId}.model`, "default") || "default";
  const mode = getSetting<string>(`agents.${agentId}.permissionMode`, "default") as PermissionMode;
  return { model, mode: PERMISSION_MODES.includes(mode) ? mode : "default" };
}

// A fresh chat follows the configured defaults (also once settings have loaded).
createRoot(() =>
  createEffect(() => {
    const d = agentDefaults(chat.agentId);
    if (untrack(() => !chat.sessionId && !chat.conv.turns.length)) setChat({ model: d.model, mode: d.mode });
  }),
);

let unlisten: (() => void) | undefined;
let historyId = 1;

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
    if (ev.turnEnded) setChat("running", false);
    for (const path of ev.changedFiles) void reloadFile(path);
    return;
  }
  if (ev.type === "exited") {
    // The process is gone; the next message starts (and resumes) a new one.
    unlisten?.();
    unlisten = undefined;
    setChat("sessionId", null);
  }
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
  try {
    const id = await ensureSession();
    await agentApi.send(id, text.trim(), context, turnId);
  } catch (e) {
    setChat("conv", produce((conv) => failTurn(conv, String(e instanceof Error ? e.message : e))));
    setChat("running", false);
  }
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

function archive(): void {
  const conv = chat.conv;
  if (!conv.turns.length) return;
  const entry: HistoryEntry = {
    id: historyId++,
    title: conv.turns[0].prompt.split("\n")[0].slice(0, 80),
    agentId: chat.agentId,
    at: Date.now(),
    conv: JSON.parse(JSON.stringify(conv)) as Conversation,
  };
  setChat("history", (h) => [entry, ...h.filter((x) => x.conv.conversationId !== conv.conversationId || !conv.conversationId)]);
}

export async function newChat(): Promise<void> {
  await endSession();
  archive();
  setChat("conv", emptyConversation());
}

export async function openHistory(id: number): Promise<void> {
  const entry = chat.history.find((h) => h.id === id);
  if (!entry) return;
  await endSession();
  archive();
  setChat({ agentId: entry.agentId, conv: JSON.parse(JSON.stringify(entry.conv)) as Conversation });
  setChat("history", (h) => h.filter((x) => x.id !== id));
}

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
