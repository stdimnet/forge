/**
 * Chat agent registry: every CLI agent the AI Chat tool window can talk to. The backend
 * adapter with the same id does the actual work (`forge-agent-cli`).
 */
import type { Component } from "solid-js";
import { createSignal } from "solid-js";

export interface ChatModel {
  /** Value passed to the CLI (`--model`); "default" lets the CLI decide. */
  id: string;
  label: string;
  hint?: string;
}

export interface ChatAgent {
  id: string;
  name: string;
  icon: Component<{ size?: number; "stroke-width"?: number }>;
  models: ChatModel[];
  /** Shown but not selectable yet. */
  comingSoon?: boolean;
  /** One-line description for the empty state. */
  tagline?: string;
  /** Slash commands offered in the composer. */
  slashCommands?: { name: string; description: string }[];
}

const [chatAgents, setChatAgents] = createSignal<ChatAgent[]>([]);

export function registerChatAgent(agent: ChatAgent): void {
  setChatAgents((prev) => [...prev.filter((a) => a.id !== agent.id), agent]);
}

export { chatAgents };

export const findChatAgent = (id: string) => chatAgents().find((a) => a.id === id);
