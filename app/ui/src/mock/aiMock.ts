/** Browser-mode AI backend: canned providers, models and a timer-driven stream. */
import { emitMock, registerMock } from "@/ipc";
import type {
  AiFeature,
  AiRequest,
  ModelInfo,
  ModelSelection,
  ProviderConfig,
  ProviderInfo,
  ProviderKind,
  ProvidersState,
  TestResult,
} from "@/ipc/ai";
import { presetFor } from "@/ipc/ai";

const providers: ProviderConfig[] = [
  { id: "anthropic", kind: "anthropic", name: "Anthropic", baseUrl: "https://api.anthropic.com/v1", defaultModel: "claude-sonnet-5-5", enabled: true, headers: {} },
  { id: "openrouter", kind: "openrouter", name: "OpenRouter", baseUrl: "https://openrouter.ai/api/v1", defaultModel: "anthropic/claude-sonnet-5.5", enabled: true, headers: { ...presetFor("openrouter").headers } },
  { id: "ollama", kind: "ollama", name: "Ollama", baseUrl: "http://localhost:11434/v1", defaultModel: "qwen2.5-coder:7b", enabled: true, headers: {} },
  { id: "lmstudio", kind: "lmstudio", name: "LM Studio", baseUrl: "http://localhost:1234/v1", defaultModel: null, enabled: false, headers: {} },
];
const keychain = new Map<string, string>([["anthropic", "sk-ant-mock"]]);
const envKeys = new Set<ProviderKind>(["openrouter"]);
let defaultProvider: string | null = "anthropic";
const features: Partial<Record<AiFeature, ModelSelection>> = {
  chat: { provider: "anthropic", model: "claude-sonnet-5-5" },
  commitMessages: { provider: "anthropic", model: "claude-haiku-5" },
  completion: { provider: "ollama", model: "qwen2.5-coder:7b" },
};

const modelsByKind: Record<ProviderKind, ModelInfo[]> = {
  anthropic: [
    { id: "claude-opus-5-5", name: "Claude Opus 5.5" },
    { id: "claude-sonnet-5-5", name: "Claude Sonnet 5.5" },
    { id: "claude-haiku-5", name: "Claude Haiku 5" },
  ],
  openai: [{ id: "gpt-4.1" }, { id: "gpt-4.1-mini" }, { id: "o4-mini" }],
  openrouter: [
    { id: "anthropic/claude-sonnet-5.5", name: "Anthropic: Claude Sonnet 5.5" },
    { id: "deepseek/deepseek-chat-v3", name: "DeepSeek: DeepSeek V3" },
    { id: "google/gemini-2.5-pro", name: "Google: Gemini 2.5 Pro" },
    { id: "openai/gpt-4.1-mini", name: "OpenAI: GPT-4.1 Mini" },
  ],
  ollama: [{ id: "llama3.1:8b" }, { id: "qwen2.5-coder:7b" }, { id: "qwen3:8b" }],
  lmstudio: [{ id: "deepseek-r1-distill-qwen-7b" }, { id: "qwen2.5-coder-7b-instruct" }],
  llamacpp: [{ id: "default" }],
  custom: [{ id: "default" }],
};

const latencyByKind: Record<ProviderKind, number> = {
  anthropic: 312, openai: 287, openrouter: 405, ollama: 24, lmstudio: 31, llamacpp: 18, custom: 60,
};

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function find(id: string): ProviderConfig {
  const p = providers.find((p) => p.id === id);
  if (!p) throw new Error(`unknown AI provider: ${id}`);
  return p;
}

function effectiveDefault(): string | null {
  const d = providers.find((p) => p.id === defaultProvider && p.enabled);
  return d?.id ?? providers.find((p) => p.enabled)?.id ?? null;
}

function info(p: ProviderConfig): ProviderInfo {
  const inKeychain = keychain.has(p.id);
  const inEnv = !inKeychain && envKeys.has(p.kind);
  return {
    ...p,
    hasApiKey: inKeychain || inEnv,
    keySource: inKeychain ? "keychain" : inEnv ? "env" : "none",
    isDefault: effectiveDefault() === p.id,
  };
}

function checkReachable(p: ProviderConfig): void {
  const preset = presetFor(p.kind);
  if (p.kind === "lmstudio" || p.kind === "llamacpp" || p.kind === "custom") {
    const hint = p.kind === "lmstudio" ? "is LM Studio running? Start the local server in LM Studio's Developer tab" : "is the server running and the base URL correct?";
    throw new Error(`${p.name}: cannot connect to ${p.baseUrl} — ${hint}`);
  }
  if (preset.needsKey && !info(p).hasApiKey) {
    throw new Error(`${p.name}: no API key configured (add one in Settings | Tools | AI Providers or set ${preset.envVar})`);
  }
  if (keychain.get(p.id) === "bad") throw new Error(`${p.name}: invalid API key`);
}

function uniqueId(name: string): string {
  const base = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "provider";
  if (!providers.some((p) => p.id === base)) return base;
  for (let n = 2; ; n++) if (!providers.some((p) => p.id === `${base}-${n}`)) return `${base}-${n}`;
}

registerMock("ai_providers_list", (): ProvidersState => ({
  providers: providers.map(info),
  defaultProvider: effectiveDefault(),
  features: { ...features },
}));

registerMock("ai_provider_save", ({ config, apiKey }) => {
  const cfg = { ...(config as ProviderConfig) };
  if (!cfg.name.trim()) throw new Error("provider name must not be empty");
  if (!/^https?:\/\//.test(cfg.baseUrl)) throw new Error(`invalid base URL: ${cfg.baseUrl}`);
  if (!cfg.id) cfg.id = uniqueId(cfg.name);
  if (typeof apiKey === "string") apiKey.trim() ? keychain.set(cfg.id, apiKey.trim()) : keychain.delete(cfg.id);
  const i = providers.findIndex((p) => p.id === cfg.id);
  if (i >= 0) providers[i] = cfg;
  else providers.push(cfg);
  if (!defaultProvider) defaultProvider = cfg.id;
  return cfg;
});

registerMock("ai_provider_delete", ({ id }) => {
  const i = providers.findIndex((p) => p.id === id);
  if (i < 0) throw new Error(`unknown AI provider: ${id}`);
  providers.splice(i, 1);
  keychain.delete(String(id));
  for (const f of Object.keys(features) as AiFeature[]) if (features[f]?.provider === id) delete features[f];
  if (defaultProvider === id) defaultProvider = effectiveDefault();
});

registerMock("ai_provider_test", async ({ id }): Promise<TestResult> => {
  const p = find(String(id));
  const latencyMs = latencyByKind[p.kind];
  await sleep(Math.min(latencyMs, 400) + 150);
  try {
    checkReachable(p);
    return { ok: true, latencyMs, modelCount: modelsByKind[p.kind].length };
  } catch (e) {
    return { ok: false, latencyMs: 2, error: (e as Error).message };
  }
});

registerMock("ai_models", async ({ providerId }) => {
  const p = find(String(providerId));
  await sleep(250);
  checkReachable(p);
  return modelsByKind[p.kind];
});

registerMock("ai_set_default", ({ providerId, model, feature }) => {
  const p = find(String(providerId));
  if (feature) {
    const m = (model as string | null) || p.defaultModel;
    if (!m) throw new Error(`no model selected for ${p.name}`);
    features[feature as AiFeature] = { provider: p.id, model: m };
  } else {
    if (model) p.defaultModel = String(model);
    defaultProvider = p.id;
  }
});

function cannedAnswer(req: AiRequest): string {
  const system = req.system?.toLowerCase() ?? "";
  if (system.includes("commit")) {
    return "Fix user email normalization on registration\n\nLowercase and trim e-mail addresses before validation so duplicate accounts with different casing are rejected.";
  }
  const last = req.messages[req.messages.length - 1]?.content ?? "";
  return `Sure — here is a quick take on "${last.slice(0, 60)}".\n\nIn \`app/Models/User.php\` the \`$fillable\` array controls mass assignment. Keep \`password\` hashed via the \`hashed\` cast and never expose it in \`$visible\`.`;
}

function resolveProvider(req: AiRequest): ProviderConfig {
  const sel = req.feature ? features[req.feature] : undefined;
  const id = req.provider ?? sel?.provider ?? effectiveDefault();
  if (!id) throw new Error("no AI provider configured");
  return find(id);
}

registerMock("ai_complete", async ({ request }) => {
  const req = request as AiRequest;
  checkReachable(resolveProvider(req));
  await sleep(600);
  return cannedAnswer(req);
});

const streams = new Map<string, ReturnType<typeof setTimeout>>();

registerMock("ai_stream", ({ request, streamId }) => {
  const req = request as AiRequest;
  const event = `ai://stream/${streamId}`;
  try {
    checkReachable(resolveProvider(req));
  } catch (e) {
    const error = (e as Error).message;
    setTimeout(() => emitMock(event, { error }), 0);
    throw new Error(error);
  }
  const tokens = cannedAnswer(req).match(/\S+\s*|\s+/g) ?? [];
  let i = 0;
  const tick = () => {
    if (i < tokens.length) {
      emitMock(event, { delta: tokens[i++] });
      streams.set(String(streamId), setTimeout(tick, 35));
    } else {
      streams.delete(String(streamId));
      emitMock(event, { done: true });
    }
  };
  streams.set(String(streamId), setTimeout(tick, 120));
});

registerMock("ai_cancel", ({ streamId }) => {
  const t = streams.get(String(streamId));
  if (t) {
    clearTimeout(t);
    streams.delete(String(streamId));
    emitMock(`ai://stream/${streamId}`, { done: true });
  }
});
