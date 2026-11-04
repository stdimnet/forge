/**
 * Settings | Tools | AI Assistant: AI in the editor. The schema settings (`ai.*` in the AI
 * plugin manifest) plus, per section, the provider and model the feature uses (stored in
 * `ai.toml` like Settings | Tools | AI Providers' "Models per feature").
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { aiApi, type AiFeature, type ModelInfo } from "@/ipc/ai";
import { SchemaPage } from "@/features/settings/SchemaPage";
import { openSettings } from "@/features/settings/dialogState";
import { refreshAiStatus } from "./status";
import { t } from "@/i18n";

function ModelPicker(props: { feature: AiFeature; label: string; hint: string }) {
  const [state, { refetch }] = createResource(() => aiApi.providers());
  const [models, setModels] = createSignal<ModelInfo[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  const enabled = () => state()?.providers.filter((p) => p.enabled) ?? [];
  const selection = () => state()?.features[props.feature];
  const providerId = () => selection()?.provider ?? state()?.defaultProvider ?? "";
  const provider = () => enabled().find((p) => p.id === providerId());
  const model = () => selection()?.model ?? provider()?.defaultModel ?? "";
  const choose = async (id: string, m?: string) => {
    setError(null);
    try {
      await aiApi.setDefault(id, m || undefined, props.feature);
      await refetch();
      void refreshAiStatus();
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  };
  const fetchModels = async () => {
    if (!providerId()) return;
    try {
      setModels(await aiApi.models(providerId()));
    } catch (e) {
      setError(String(e instanceof Error ? e.message : e));
    }
  };
  const listId = `ai-models-${props.feature}`;
  return (
    <div class="ai-assistant-model">
      <Show
        when={enabled().length}
        fallback={
          <span class="muted">
            {t("tools.aiEditor.noAiProviderIsConfigured")}
            <a href="#" onClick={(e) => (e.preventDefault(), openSettings("tools.ai-providers"))}>
              {t("tools.aiEditor.addAProviderEllipsis")}
            </a>
          </span>
        }
      >
        <label>{props.label}:</label>
        <select value={providerId()} onChange={(e) => void choose(e.currentTarget.value, enabled().find((p) => p.id === e.currentTarget.value)?.defaultModel ?? undefined)}>
          <For each={enabled()}>{(p) => <option value={p.id}>{p.name}</option>}</For>
        </select>
        <input
          list={listId}
          value={model()}
          placeholder={t("tools.aiEditor.providerDefault")}
          onFocus={() => void fetchModels()}
          onChange={(e) => void choose(providerId(), e.currentTarget.value.trim())}
        />
        <datalist id={listId}>
          <For each={models()}>{(m) => <option value={m.id}>{m.name ?? m.id}</option>}</For>
        </datalist>
        <span class="muted ai-assistant-hint">{props.hint}</span>
        <Show when={error()}>
          <span class="ai-assistant-error">{error()}</span>
        </Show>
      </Show>
    </div>
  );
}

export function AiAssistantPage() {
  return (
    <SchemaPage
      pageId="tools.ai-assistant"
      sectionHeader={(section) =>
        section === "General" ? (
          <ModelPicker
            feature="chat"
            label={t("tools.aiEditor.actionsModel")}
            hint={t("tools.aiEditor.explainFixGenerateDocumentationAnd")}
          />
        ) : section === "Inline Completion" ? (
          <ModelPicker feature="completion" label={t("tools.aiEditor.completionModel")} hint={t("tools.aiEditor.preferAFastLocalCode")} />
        ) : undefined
      }
    />
  );
}
