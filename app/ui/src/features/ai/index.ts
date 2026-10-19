import { registerSettingsPage } from "@/features/settingsPages";
import { registerCommand } from "@/state/commands";
import { AiProvidersPage } from "./AiProvidersPage";
import { openTempAiSettings } from "./TempSettingsModal";

registerSettingsPage({
  id: "tools.ai-providers",
  title: "AI Providers",
  group: "Tools",
  order: 10,
  keywords: ["AI", "LLM", "Anthropic", "Claude", "OpenAI", "OpenRouter", "Ollama", "LM Studio", "llama.cpp", "API key", "model"],
  component: AiProvidersPage,
});

// TEMPORARY until the Settings dialog lands: opens the page in a standalone modal.
registerCommand({ id: "ai.settings", title: "AI Providers…", category: "AI", run: openTempAiSettings });
