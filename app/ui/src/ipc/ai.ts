/**
 * Typed client for the AI backend (crates/forge-ai via app/src-tauri/src/commands/ai.rs).
 * `complete` / `stream` follow the shared contract in docs/ARCHITECTURE.md.
 */
import { invoke, listen } from "./index";

export type AiRole = "user" | "assistant";
export type AiFeature = "chat" | "commitMessages" | "completion";
export type ProviderKind = "anthropic" | "openai" | "openrouter" | "lmstudio" | "ollama" | "llamacpp" | "custom";
export type KeySource = "keychain" | "env" | "none";

export interface AiMessage {
  role: AiRole;
  content: string;
}

export interface AiRequest {
  system?: string;
  messages: AiMessage[];
  /** Provider id; defaults to the feature's or the default provider. */
  provider?: string;
  model?: string;
  maxTokens?: number;
  temperature?: number;
  stop?: string[];
  /** Lets the backend pick the per-feature model (Settings | Tools | AI Providers). */
  feature?: AiFeature;
}

export interface AiStreamEvent {
  delta?: string;
  done?: boolean;
  error?: string;
}

export interface ProviderConfig {
  id: string;
  kind: ProviderKind;
  name: string;
  baseUrl: string;
  defaultModel?: string | null;
  enabled: boolean;
  headers: Record<string, string>;
}

export interface ProviderInfo extends ProviderConfig {
  hasApiKey: boolean;
  keySource: KeySource;
  isDefault: boolean;
}

export interface ModelSelection {
  provider: string;
  model: string;
}

export interface ProvidersState {
  providers: ProviderInfo[];
  defaultProvider?: string | null;
  features: Partial<Record<AiFeature, ModelSelection>>;
}

export interface ModelInfo {
  id: string;
  name?: string | null;
}

export interface TestResult {
  ok: boolean;
  latencyMs: number;
  error?: string | null;
  modelCount?: number | null;
}

let streamCounter = 0;
const newStreamId = () => `s${Date.now().toString(36)}${(streamCounter++).toString(36)}`;

export interface StreamHandlers {
  onDelta?: (text: string) => void;
  onDone?: (fullText: string) => void;
  onError?: (message: string) => void;
}

export interface StreamHandle {
  streamId: string;
  cancel: () => Promise<void>;
  /** Resolves with the full text when the stream ends; rejects on error. */
  result: Promise<string>;
}

/** Start a stream with callbacks. Cancelling resolves `result` with the text so far. */
export async function streamWithCallbacks(request: AiRequest, handlers: StreamHandlers = {}): Promise<StreamHandle> {
  const streamId = newStreamId();
  let text = "";
  let settle!: { resolve: (t: string) => void; reject: (e: Error) => void };
  const result = new Promise<string>((resolve, reject) => (settle = { resolve, reject }));
  result.catch(() => {}); // callers using callbacks only must not see unhandled rejections
  let finished = false;
  const finish = (error?: string) => {
    if (finished) return;
    finished = true;
    unlisten();
    if (error) {
      handlers.onError?.(error);
      settle.reject(new Error(error));
    } else {
      handlers.onDone?.(text);
      settle.resolve(text);
    }
  };
  // Subscribe before starting so no early delta is lost.
  const unlisten = await listen<AiStreamEvent>(`ai://stream/${streamId}`, (ev) => {
    if (ev.delta) {
      text += ev.delta;
      handlers.onDelta?.(ev.delta);
    }
    if (ev.error) finish(ev.error);
    else if (ev.done) finish();
  });
  invoke<void>("ai_stream", { request, streamId }).catch((e) => finish(String(e?.message ?? e)));
  return {
    streamId,
    result,
    cancel: async () => {
      await invoke<void>("ai_cancel", { streamId });
      finish();
    },
  };
}

/** Stream as an async iterator of text deltas. Breaking out of the loop cancels the stream. */
export async function* stream(request: AiRequest): AsyncGenerator<string, void, undefined> {
  const queue: string[] = [];
  let wake: (() => void) | undefined;
  let done = false;
  let error: Error | undefined;
  const handle = await streamWithCallbacks(request, {
    onDelta: (d) => {
      queue.push(d);
      wake?.();
    },
    onDone: () => {
      done = true;
      wake?.();
    },
    onError: (m) => {
      error = new Error(m);
      wake?.();
    },
  });
  try {
    for (;;) {
      if (queue.length) {
        yield queue.shift()!;
        continue;
      }
      if (error) throw error;
      if (done) return;
      await new Promise<void>((r) => (wake = r));
      wake = undefined;
    }
  } finally {
    if (!done && !error) await handle.cancel();
  }
}

export const aiApi = {
  complete: (request: AiRequest) => invoke<string>("ai_complete", { request }),
  stream,
  streamWithCallbacks,
  cancel: (streamId: string) => invoke<void>("ai_cancel", { streamId }),
  providers: () => invoke<ProvidersState>("ai_providers_list"),
  saveProvider: (config: ProviderConfig, apiKey?: string) =>
    invoke<ProviderConfig>("ai_provider_save", { config, apiKey: apiKey ?? null }),
  deleteProvider: (id: string) => invoke<void>("ai_provider_delete", { id }),
  testProvider: (id: string) => invoke<TestResult>("ai_provider_test", { id }),
  models: (providerId: string) => invoke<ModelInfo[]>("ai_models", { providerId }),
  setDefault: (providerId: string, model?: string, feature?: AiFeature) =>
    invoke<void>("ai_set_default", { providerId, model: model ?? null, feature: feature ?? null }),
};

/** Presets offered by "Add provider" (mirrors ProviderKind in crates/forge-ai/src/config.rs). */
export interface ProviderPreset {
  kind: ProviderKind;
  name: string;
  description: string;
  baseUrl: string;
  defaultModel?: string;
  needsKey: boolean;
  envVar?: string;
  local: boolean;
  headers?: Record<string, string>;
}

export const providerPresets: ProviderPreset[] = [
  { kind: "anthropic", name: "Anthropic", description: "Claude models via the Anthropic API", baseUrl: "https://api.anthropic.com/v1", defaultModel: "claude-sonnet-5-5", needsKey: true, envVar: "ANTHROPIC_API_KEY", local: false },
  { kind: "openai", name: "OpenAI", description: "GPT models via the OpenAI API", baseUrl: "https://api.openai.com/v1", defaultModel: "gpt-4.1", needsKey: true, envVar: "OPENAI_API_KEY", local: false },
  { kind: "openrouter", name: "OpenRouter", description: "Hundreds of models through one API key", baseUrl: "https://openrouter.ai/api/v1", defaultModel: "anthropic/claude-sonnet-5.5", needsKey: true, envVar: "OPENROUTER_API_KEY", local: false, headers: { "HTTP-Referer": "https://github.com/Reeperk/PHPForge", "X-Title": "Forge IDE" } },
  { kind: "ollama", name: "Ollama", description: "Local models served by Ollama", baseUrl: "http://localhost:11434/v1", needsKey: false, local: true },
  { kind: "lmstudio", name: "LM Studio", description: "Local models served by LM Studio", baseUrl: "http://localhost:1234/v1", needsKey: false, local: true },
  { kind: "llamacpp", name: "llama.cpp", description: "llama-server with an OpenAI-compatible API", baseUrl: "http://localhost:8080/v1", needsKey: false, local: true },
  { kind: "custom", name: "OpenAI-compatible", description: "Any server implementing /v1/chat/completions", baseUrl: "http://localhost:8000/v1", needsKey: false, local: false },
];

export const presetFor = (kind: ProviderKind): ProviderPreset => providerPresets.find((p) => p.kind === kind)!;
