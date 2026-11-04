/**
 * Settings | Tools | AI Providers — configure Anthropic / OpenAI-compatible providers,
 * their keys and models, and which model each AI feature uses.
 */
import { createEffect, createMemo, createResource, createSignal, For, Match, on, onCleanup, Show, Switch } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import {
  ChevronDown,
  CircleCheck,
  CircleX,
  KeyRound,
  LoaderCircle,
  Minus,
  Plug,
  Plus,
  RefreshCw,
  Star,
} from "lucide-solid";
import {
  aiApi,
  presetFor,
  providerPresets,
  type AiFeature,
  type ModelInfo,
  type ProviderConfig,
  type ProviderInfo,
  type ProviderPreset,
  type TestResult,
} from "@/ipc/ai";
import { KindIcon } from "./kinds";
import { t as tr } from "@/i18n";
import "./ai.css";

type TestState = { running: true } | ({ running: false } & TestResult);

const NEW_ID = "__new__";

const featureRows: { feature: AiFeature; label: string; hint: string }[] = [
  { feature: "chat", label: "Chat", hint: "AI Chat tool window and inline questions" },
  { feature: "commitMessages", label: "Commit messages", hint: "Generate Commit Message in the Commit tool window" },
  { feature: "completion", label: "Code completion", hint: "Inline completion while typing; prefer a fast local model" },
];

const errorText = (e: unknown) => String((e as Error)?.message ?? e);

export function AiProvidersPage() {
  const [state, { refetch }] = createResource(aiApi.providers);
  const providers = () => state()?.providers ?? [];

  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [draft, setDraft] = createStore<ProviderConfig>(emptyDraft("anthropic"));
  const [apiKey, setApiKey] = createSignal<string | undefined>(undefined);
  const [dirty, setDirty] = createSignal(false);
  const [saveError, setSaveError] = createSignal<string>();
  const [pickerOpen, setPickerOpen] = createSignal(false);
  const [tests, setTests] = createStore<Record<string, TestState>>({});
  const [models, setModels] = createStore<Record<string, ModelInfo[]>>({});
  const [modelsLoading, setModelsLoading] = createSignal<string | null>(null);
  const [modelsError, setModelsError] = createSignal<string>();

  const selected = createMemo(() => providers().find((p) => p.id === selectedId()));
  const isNew = () => selectedId() === NEW_ID;

  // Select the default provider once loaded.
  createEffect(() => {
    const list = providers();
    if (selectedId() === null && list.length) select(list.find((p) => p.isDefault)?.id ?? list[0].id);
  });

  function select(id: string) {
    const p = providers().find((p) => p.id === id);
    if (!p) return;
    setSelectedId(id);
    const { hasApiKey: _h, keySource: _k, isDefault: _d, ...config } = p;
    setDraft(reconcile({ ...config, headers: { ...config.headers } }));
    resetForm();
  }

  function resetForm() {
    setApiKey(undefined);
    setDirty(false);
    setSaveError(undefined);
    setModelsError(undefined);
  }

  function addFromPreset(preset: ProviderPreset) {
    setPickerOpen(false);
    setSelectedId(NEW_ID);
    setDraft(reconcile(emptyDraft(preset.kind)));
    resetForm();
    setDirty(true);
  }

  const update = <K extends keyof ProviderConfig>(key: K, value: ProviderConfig[K]) => {
    setDraft(key, value as never);
    setDirty(true);
  };

  async function save(): Promise<string | undefined> {
    setSaveError(undefined);
    try {
      const saved = await aiApi.saveProvider({ ...draft, id: isNew() ? "" : draft.id }, apiKey());
      await refetch();
      setSelectedId(saved.id);
      setDraft("id", saved.id);
      setApiKey(undefined);
      setDirty(false);
      return saved.id;
    } catch (e) {
      setSaveError(errorText(e));
      return undefined;
    }
  }

  async function remove() {
    const id = selectedId();
    if (!id) return;
    if (id !== NEW_ID) {
      await aiApi.deleteProvider(id);
      await refetch();
    }
    setSelectedId(null);
    const first = providers()[0];
    if (first) select(first.id);
  }

  async function test() {
    const id = dirty() || isNew() ? await save() : selectedId();
    if (!id) return;
    setTests(id, { running: true });
    try {
      setTests(id, { running: false, ...(await aiApi.testProvider(id)) });
    } catch (e) {
      setTests(id, { running: false, ok: false, latencyMs: 0, error: errorText(e) });
    }
  }

  async function fetchModels(id: string): Promise<void> {
    setModelsLoading(id);
    setModelsError(undefined);
    try {
      setModels(id, await aiApi.models(id));
    } catch (e) {
      setModelsError(errorText(e));
    } finally {
      setModelsLoading(null);
    }
  }

  async function fetchDraftModels() {
    const id = dirty() || isNew() ? await save() : selectedId();
    if (id) await fetchModels(id);
  }

  async function setFeature(feature: AiFeature, providerId: string, model?: string) {
    await aiApi.setDefault(providerId, model, feature);
    await refetch();
  }

  async function setDefaultProvider(providerId: string) {
    await aiApi.setDefault(providerId);
    await refetch();
  }

  const preset = () => presetFor(draft.kind);
  const keyHint = () => {
    const p = selected();
    if (apiKey() !== undefined && apiKey() !== "") return "Will be stored in the system keychain on Apply";
    if (p?.keySource === "keychain") return "Stored in the system keychain";
    if (p?.keySource === "env") return `Using ${preset().envVar} from the environment`;
    return preset().needsKey
      ? `Stored in the system keychain${preset().envVar ? `; falls back to ${preset().envVar}` : ""}`
      : "Optional for local servers";
  };

  const enabledProviders = () => providers().filter((p) => p.enabled);

  return (
    <div class="ai-settings">
      <p class="ai-settings-intro muted">
        {tr("tools.ai.connectForgeToCloudOr")}
      </p>

      <div class="ai-master-detail">
        {/* Provider list */}
        <div class="ai-list-pane">
          <div class="ai-list-toolbar">
            <button class="icon-btn" title={tr("tools.ai.addProvider")} data-testid="ai-add-provider" onClick={() => setPickerOpen((v) => !v)}>
              <Plus size={16} />
            </button>
            <button class="icon-btn" title={tr("tools.ai.removeProvider")} disabled={!selectedId()} onClick={remove}>
              <Minus size={16} />
            </button>
            <button
              class="icon-btn"
              title={tr("tools.ai.setAsDefaultProvider")}
              disabled={!selected() || selected()!.isDefault}
              onClick={() => selected() && setDefaultProvider(selected()!.id)}
            >
              <Star size={15} />
            </button>
            <Show when={pickerOpen()}>
              <PresetPicker onPick={addFromPreset} onClose={() => setPickerOpen(false)} />
            </Show>
          </div>
          <div class="ai-list">
            <For each={providers()}>
              {(p) => (
                <ProviderRow
                  provider={p}
                  selected={selectedId() === p.id}
                  test={tests[p.id]}
                  onClick={() => select(p.id)}
                />
              )}
            </For>
            <Show when={isNew()}>
              <div class="ai-row selected">
                <KindIcon kind={draft.kind} size={26} />
                <div class="ai-row-text">
                  <div class="ai-row-title">{draft.name || tr("tools.ai.newProvider")}</div>
                  <div class="ai-row-sub muted">{tr("common.notSavedYet")}</div>
                </div>
              </div>
            </Show>
            <Show when={!providers().length && !isNew() && !state.loading}>
              <div class="ai-empty muted">
                {tr("tools.ai.noProvidersYet")}
                <button class="ai-link" onClick={() => setPickerOpen(true)}>
                  {tr("tools.ai.addProviderEllipsis")}
                </button>
              </div>
            </Show>
          </div>
        </div>

        {/* Provider details */}
        <div class="ai-detail-pane">
          <Show when={selectedId()} fallback={<div class="ai-empty muted">{tr("tools.ai.selectAProviderToConfigure")}</div>}>
            <div class="ai-detail-header">
              <KindIcon kind={draft.kind} size={32} />
              <div>
                <div class="ai-detail-title">{draft.name || preset().name}</div>
                <div class="muted ai-detail-sub">
                  {draft.kind === "anthropic" ? tr("tools.ai.anthropicMessagesApi") : tr("tools.ai.openaiCompatibleApi", { value: preset().name === "OpenAI-compatible" ? "Custom" : preset().name })}
                </div>
              </div>
            </div>

            <div class="ai-form">
              <label for="ai-name">{tr("common.nameLabel")}</label>
              <input id="ai-name" value={draft.name} onInput={(e) => update("name", e.currentTarget.value)} />

              <label for="ai-url">{tr("tools.ai.baseUrlLabel")}</label>
              <input
                id="ai-url"
                class="mono"
                value={draft.baseUrl}
                spellcheck={false}
                onInput={(e) => update("baseUrl", e.currentTarget.value)}
              />

              <label for="ai-key">{tr("tools.ai.apiKeyLabel")}</label>
              <div class="ai-field">
                <div class="ai-key-input">
                  <KeyRound size={14} class="ai-key-icon" />
                  <input
                    id="ai-key"
                    type="password"
                    autocomplete="off"
                    placeholder={selected()?.hasApiKey ? "••••••••••••••••••••" : preset().needsKey ? tr("tools.ai.pasteApiKey") : tr("tools.ai.notRequired")}
                    value={apiKey() ?? ""}
                    onInput={(e) => {
                      setApiKey(e.currentTarget.value);
                      setDirty(true);
                    }}
                  />
                </div>
                <div class="ai-hint muted">{keyHint()}</div>
              </div>

              <label for="ai-model">{tr("tools.ai.defaultModelLabel")}</label>
              <div class="ai-field">
                <div class="ai-inline">
                  <ModelCombo
                    id="ai-model"
                    value={draft.defaultModel ?? ""}
                    models={models[draft.id] ?? []}
                    placeholder={preset().local ? tr("tools.ai.fetchModelsFromTheRunning") : tr("tools.ai.modelId")}
                    onChange={(m) => update("defaultModel", m)}
                  />
                  <button class="btn-secondary" onClick={fetchDraftModels} disabled={modelsLoading() !== null}>
                    <Show when={modelsLoading() !== null} fallback={<RefreshCw size={14} />}>
                      <LoaderCircle size={14} class="spin" />
                    </Show>
                    {tr("tools.ai.fetchModels")}
                  </button>
                </div>
                <Show when={modelsError()}>
                  <div class="ai-hint ai-error-text">{modelsError()}</div>
                </Show>
                <Show when={!modelsError() && models[draft.id]}>
                  <div class="ai-hint muted">{tr("tools.ai.modelsAvailable", { count: models[draft.id]!.length })}</div>
                </Show>
              </div>

              <span />
              <label class="ai-check">
                <input type="checkbox" checked={draft.enabled} onChange={(e) => update("enabled", e.currentTarget.checked)} />
                {tr("common.enabled")}
              </label>
            </div>

            <div class="ai-actions">
              <button class="btn-secondary" data-testid="ai-test" onClick={test} disabled={tests[draft.id]?.running}>
                <Plug size={14} />
                {tr("common.testConnection")}
              </button>
              <TestChip test={tests[draft.id]} />
              <span class="ai-spacer" />
              <Show when={saveError()}>
                <span class="ai-error-text">{saveError()}</span>
              </Show>
              <button class="btn-secondary" disabled={!dirty() || isNew()} onClick={() => selectedId() && select(selectedId()!)}>
                {tr("common.reset")}
              </button>
              <button class="btn-primary" disabled={!dirty()} onClick={save}>
                {tr("common.apply")}
              </button>
            </div>
          </Show>
        </div>
      </div>

      {/* Per-feature models */}
      <div class="ai-section-title">
        <span>{tr("tools.ai.modelsPerFeature")}</span>
        <span class="ai-section-line" />
      </div>
      <div class="ai-feature-grid">
        <label>{tr("tools.ai.defaultProviderLabel")}</label>
        <select
          value={state()?.defaultProvider ?? ""}
          onChange={(e) => setDefaultProvider(e.currentTarget.value)}
          disabled={!enabledProviders().length}
        >
          <For each={enabledProviders()}>{(p) => <option value={p.id}>{p.name}</option>}</For>
        </select>
        <span class="ai-hint muted">{tr("tools.ai.usedByFeaturesWithoutTheir")}</span>

        <For each={featureRows}>
          {(row) => (
            <FeatureRow
              label={row.label}
              hint={row.hint}
              providers={enabledProviders()}
              selection={state()?.features[row.feature]}
              defaultProvider={state()?.defaultProvider ?? undefined}
              models={models}
              loading={modelsLoading()}
              onFetch={fetchModels}
              onChange={(provider, model) => setFeature(row.feature, provider, model)}
            />
          )}
        </For>
      </div>
    </div>
  );
}

function emptyDraft(kind: ProviderConfig["kind"]): ProviderConfig {
  const p = presetFor(kind);
  return {
    id: "",
    kind,
    name: p.name,
    baseUrl: p.baseUrl,
    defaultModel: p.defaultModel ?? null,
    enabled: true,
    headers: { ...(p.headers ?? {}) },
  };
}

function ProviderRow(props: { provider: ProviderInfo; selected: boolean; test?: TestState; onClick: () => void }) {
  const p = () => props.provider;
  const preset = () => presetFor(p().kind);
  const sub = () => p().defaultModel ?? p().baseUrl.replace(/^https?:\/\//, "");
  return (
    <div
      class="ai-row"
      classList={{ selected: props.selected, disabled: !p().enabled }}
      data-provider={p().id}
      onClick={props.onClick}
    >
      <KindIcon kind={p().kind} size={26} />
      <div class="ai-row-text">
        <div class="ai-row-title">
          {p().name}
          <Show when={p().isDefault}>
            <span class="ai-badge accent">{tr("common.default")}</span>
          </Show>
        </div>
        <div class="ai-row-sub muted">{sub()}</div>
      </div>
      <div class="ai-row-badges">
        <Switch>
          <Match when={!p().enabled}>
            <span class="ai-badge">{tr("tools.ai.disabled")}</span>
          </Match>
          <Match when={preset().local}>
            <span class="ai-badge">{tr("tools.ai.local")}</span>
          </Match>
          <Match when={preset().needsKey && !p().hasApiKey}>
            <span class="ai-badge warn">{tr("tools.ai.noKey")}</span>
          </Match>
          <Match when={p().keySource === "env"}>
            <span class="ai-badge" title={tr("tools.ai.keyFrom", { envVar: preset().envVar })}>
              env
            </span>
          </Match>
        </Switch>
        <Show when={props.test && !props.test.running}>
          <span
            class="ai-dot"
            classList={{ ok: (props.test as TestResult).ok, bad: !(props.test as TestResult).ok }}
            title={(props.test as TestResult).ok ? tr("tools.ai.connected") : (props.test as TestResult).error ?? tr("tools.ai.failed")}
          />
        </Show>
      </div>
    </div>
  );
}

function TestChip(props: { test?: TestState }) {
  return (
    <Show when={props.test}>
      {(t) => (
        <Switch>
          <Match when={t().running}>
            <span class="ai-chip" data-testid="ai-test-result">
              <LoaderCircle size={13} class="spin" /> {tr("tools.ai.testingEllipsis")}
            </span>
          </Match>
          <Match when={!t().running && (t() as TestResult).ok}>
            <span class="ai-chip ok" data-testid="ai-test-result">
              <CircleCheck size={13} />
              {tr("tools.ai.connectedMs", { latencyMs: (t() as TestResult).latencyMs })}
              <Show when={(t() as TestResult).modelCount != null}> {tr("tools.ai.models", { modelCount: (t() as TestResult).modelCount })}</Show>
            </span>
          </Match>
          <Match when={!t().running}>
            <span class="ai-chip bad" data-testid="ai-test-result" title={(t() as TestResult).error ?? ""}>
              <CircleX size={13} />
              <span class="ai-chip-text">{(t() as TestResult).error ?? tr("tools.ai.connectionFailed")}</span>
            </span>
          </Match>
        </Switch>
      )}
    </Show>
  );
}

function PresetPicker(props: { onPick: (p: ProviderPreset) => void; onClose: () => void }) {
  let ref!: HTMLDivElement;
  const onDoc = (e: MouseEvent) => {
    if (!ref.contains(e.target as Node) && !(e.target as HTMLElement).closest?.("[data-testid=ai-add-provider]")) props.onClose();
  };
  const onKey = (e: KeyboardEvent) => e.key === "Escape" && props.onClose();
  document.addEventListener("mousedown", onDoc);
  document.addEventListener("keydown", onKey);
  onCleanup(() => {
    document.removeEventListener("mousedown", onDoc);
    document.removeEventListener("keydown", onKey);
  });
  const group = (local: boolean) => providerPresets.filter((p) => p.local === local && p.kind !== "custom");
  const item = (p: ProviderPreset) => (
    <button class="ai-preset" data-preset={p.kind} onClick={() => props.onPick(p)}>
      <KindIcon kind={p.kind} size={24} />
      <span class="ai-preset-text">
        <span class="ai-preset-name">{p.name}</span>
        <span class="ai-preset-desc muted">{p.description}</span>
      </span>
      <Show when={p.needsKey}>
        <KeyRound size={13} class="muted" />
      </Show>
    </button>
  );
  return (
    <div class="ai-picker" ref={ref} data-testid="ai-preset-picker">
      <div class="ai-picker-group">{tr("tools.ai.cloud")}</div>
      <For each={group(false)}>{item}</For>
      <div class="ai-picker-group">{tr("tools.ai.local")}</div>
      <For each={group(true)}>{item}</For>
      <div class="ai-picker-sep" />
      {item(presetFor("custom"))}
    </div>
  );
}

function ModelCombo(props: {
  id?: string;
  value: string;
  models: ModelInfo[];
  placeholder?: string;
  onChange: (model: string) => void;
}) {
  const [open, setOpen] = createSignal(false);
  let ref!: HTMLDivElement;
  const onDoc = (e: MouseEvent) => !ref.contains(e.target as Node) && setOpen(false);
  createEffect(
    on(open, (o) => {
      if (o) document.addEventListener("mousedown", onDoc);
      else document.removeEventListener("mousedown", onDoc);
    }),
  );
  onCleanup(() => document.removeEventListener("mousedown", onDoc));
  return (
    <div class="ai-combo" ref={ref}>
      <input
        id={props.id}
        class="mono"
        value={props.value}
        placeholder={props.placeholder}
        spellcheck={false}
        onInput={(e) => props.onChange(e.currentTarget.value)}
      />
      <button class="ai-combo-btn" title={tr("tools.ai.showModels")} disabled={!props.models.length} onClick={() => setOpen((v) => !v)}>
        <ChevronDown size={14} />
      </button>
      <Show when={open() && props.models.length}>
        <div class="ai-combo-list">
          <For each={props.models}>
            {(m) => (
              <button
                class="ai-combo-item"
                classList={{ selected: m.id === props.value }}
                onClick={() => {
                  props.onChange(m.id);
                  setOpen(false);
                }}
              >
                <span class="mono">{m.id}</span>
                <Show when={m.name && m.name !== m.id}>
                  <span class="muted">{m.name}</span>
                </Show>
              </button>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

function FeatureRow(props: {
  label: string;
  hint: string;
  providers: ProviderInfo[];
  selection?: { provider: string; model: string };
  defaultProvider?: string;
  models: Record<string, ModelInfo[]>;
  loading: string | null;
  onFetch: (providerId: string) => Promise<void>;
  onChange: (provider: string, model?: string) => void;
}) {
  const providerId = () => props.selection?.provider ?? props.defaultProvider ?? "";
  const provider = () => props.providers.find((p) => p.id === providerId());
  const [model, setModel] = createSignal("");
  createEffect(() => setModel(props.selection?.model ?? provider()?.defaultModel ?? ""));
  let timer: ReturnType<typeof setTimeout> | undefined;
  const commitModel = (m: string) => {
    setModel(m);
    clearTimeout(timer);
    timer = setTimeout(() => m.trim() && props.onChange(providerId(), m.trim()), 400);
  };
  onCleanup(() => clearTimeout(timer));
  return (
    <>
      <label>{props.label}:</label>
      <div class="ai-feature-controls">
        <select
          value={providerId()}
          onChange={(e) => {
            const p = props.providers.find((x) => x.id === e.currentTarget.value);
            props.onChange(e.currentTarget.value, p?.defaultModel ?? undefined);
          }}
        >
          <For each={props.providers}>{(p) => <option value={p.id}>{p.name}</option>}</For>
        </select>
        <ModelCombo
          value={model()}
          models={props.models[providerId()] ?? []}
          placeholder={tr("tools.ai.providerDefault")}
          onChange={commitModel}
        />
        <button
          class="icon-btn"
          title={tr("tools.ai.fetchModels")}
          disabled={!providerId() || props.loading !== null}
          onClick={() => props.onFetch(providerId())}
        >
          <Show when={props.loading === providerId()} fallback={<RefreshCw size={14} />}>
            <LoaderCircle size={14} class="spin" />
          </Show>
        </button>
      </div>
      <span class="ai-hint muted">{props.hint}</span>
    </>
  );
}
