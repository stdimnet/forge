/**
 * The chat conversation as rendered. The backend folds agent events into turns and blocks
 * (`forge_agent_cli::conversation`) and sends `chat` updates; this file only applies them.
 */
import type { ChatBlock, ChatUpdate, MessageContext, PermissionBlock, ToolBlock, TurnStatus, TurnSummary } from "@/ipc/agent";

export type { PermissionBlock, PermissionStatus, ToolBlock, ToolStatus } from "@/ipc/agent";
export type Block = ChatBlock;

export interface Turn {
  id: number;
  prompt: string;
  context: MessageContext;
  blocks: Block[];
  status: TurnStatus;
  summary?: TurnSummary;
  /** Files edited in the turn. */
  changedFiles: string[];
  awaitingPermission: boolean;
  /** Running and neither streaming text nor waiting for the user. */
  working: boolean;
}

export interface Conversation {
  /** Conversation id reported by the CLI; used to resume. */
  conversationId: string | null;
  /** Model reported by the CLI on start. */
  model: string | null;
  /** Working directory of the CLI. */
  cwd: string | null;
  turns: Turn[];
}

export const emptyConversation = (): Conversation => ({ conversationId: null, model: null, cwd: null, turns: [] });

let nextTurnId = 1;

/** The message the user just sent; the backend fills it through updates. */
export function addTurn(conv: Conversation, prompt: string, context: MessageContext): Turn {
  const turn: Turn = { id: nextTurnId++, prompt, context, blocks: [], status: "running", changedFiles: [], awaitingPermission: false, working: true };
  conv.turns.push(turn);
  return turn;
}

/** Apply a `chat` update from the backend (mutates a draft, for Solid's `produce`). */
export function applyChatUpdate(conv: Conversation, u: ChatUpdate): void {
  conv.conversationId = u.conversationId ?? conv.conversationId;
  conv.model = u.model ?? conv.model;
  conv.cwd = u.cwd ?? conv.cwd;
  const t = u.turn;
  const turn = t && conv.turns.find((x) => x.id === t.id);
  if (!t || !turn) return;
  turn.status = t.status;
  turn.summary = t.summary;
  turn.changedFiles = t.changedFiles;
  turn.awaitingPermission = t.awaitingPermission;
  turn.working = t.working;
  for (const [i, block] of t.blocks) {
    if (i < turn.blocks.length) turn.blocks[i] = block;
    else turn.blocks.push(block);
  }
}

/** The last turn failed before the backend could answer (e.g. the CLI did not start). */
export function failTurn(conv: Conversation, message: string): void {
  const turn = conv.turns[conv.turns.length - 1];
  if (!turn) return;
  turn.blocks.push({ kind: "error", message });
  turn.status = "error";
  turn.working = false;
}

/** The permission request with this id, if any. */
export function findPermission(conv: Conversation, requestId: string): PermissionBlock | undefined {
  for (const t of conv.turns) for (const b of t.blocks) if (b.kind === "permission" && b.requestId === requestId) return b;
  return undefined;
}

/** Whether the agent is waiting for the user to answer a permission request. */
export const awaitingPermission = (conv: Conversation): boolean => !!conv.turns[conv.turns.length - 1]?.awaitingPermission;

export const isToolBlock = (b: Block): b is ToolBlock => b.kind === "tool";
