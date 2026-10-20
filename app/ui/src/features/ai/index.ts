import { registerSettingsPage } from "@/features/settingsPages";
import { registerCommand } from "@/state/commands";
import { AiProvidersPage } from "./AiProvidersPage";
import { openSettings } from "@/features/settings/dialogState";

registerSettingsPage({
  id: "tools.ai-providers",
  title: "AI Providers",
  group: "Tools",
  order: 10,
  keywords: ["AI", "LLM", "Anthropic", "Claude", "OpenAI", "OpenRouter", "Ollama", "LM Studio", "llama.cpp", "API key", "model"],
  component: AiProvidersPage,
});

registerCommand({
  id: "ai.settings",
  title: "AI Providers…",
  category: "AI",
  run: () => openSettings("tools.ai-providers"),
});
