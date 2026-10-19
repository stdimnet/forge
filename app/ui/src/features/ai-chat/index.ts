/**
 * AI Chat (AIForge): CLI coding agents rendered as a native chat. Forge runs the user's
 * installed CLI headless (`forge-agent-cli`) and renders its structured event stream.
 */
import { Gem, Sparkles } from "lucide-solid";
import { registerToolWindow } from "@/features/registry";
import { registerCommand } from "@/state/commands";
import { showToolWindow } from "@/state/layout";
import { registerChatAgent } from "./agents";
import { ChatActions, ChatPanel } from "./ChatPanel";
import { focusChatInput } from "./Composer";
import { newChat } from "./store";
import "./chat.css";

export { registerChatAgent } from "./agents";

registerChatAgent({
  id: "claude-code",
  name: "Claude Code",
  icon: Sparkles,
  tagline: "Claude reads, edits and runs code in your project using your own Claude Code login.",
  models: [
    { id: "default", label: "Default", hint: "Use the CLI's configured model" },
    { id: "sonnet", label: "Sonnet", hint: "Fast, great for everyday coding" },
    { id: "opus", label: "Opus", hint: "Most capable, for hard problems" },
    { id: "haiku", label: "Haiku", hint: "Fastest and cheapest" },
  ],
  slashCommands: [
    { name: "compact", description: "Summarize the conversation to free context" },
    { name: "review", description: "Review the current changes" },
    { name: "init", description: "Create a CLAUDE.md for this project" },
    { name: "cost", description: "Show token usage of this session" },
  ],
});

registerChatAgent({
  id: "gemini-cli",
  name: "Gemini CLI",
  icon: Gem,
  comingSoon: true,
  tagline: "Gemini works in your project using your own Gemini CLI login.",
  models: [
    { id: "gemini-2.5-pro", label: "Gemini 2.5 Pro" },
    { id: "gemini-2.5-flash", label: "Gemini 2.5 Flash" },
  ],
});

registerToolWindow({
  id: "ai-chat",
  title: "AI Chat",
  icon: Sparkles,
  anchor: "right",
  order: 0,
  component: ChatPanel,
  actions: ChatActions,
});

registerCommand({
  id: "AIChat.Focus",
  title: "Open AI Chat",
  category: "AI",
  run: () => {
    showToolWindow("right", "ai-chat");
    queueMicrotask(focusChatInput);
  },
});

registerCommand({ id: "AIChat.NewChat", title: "New AI Chat", category: "AI", run: () => newChat() });
