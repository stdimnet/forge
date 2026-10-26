/**
 * Settings > Plugins: installed plugins grouped by pack, with enable toggles, a search
 * field and a details pane (PHPStorm's "Installed" tab).
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { Puzzle, Search } from "lucide-solid";
import type { PluginInfo } from "@/ipc/settings";
import { settingsState } from "@/state/settings";
import { pluginEnabled, pluginDraft, setPluginDraftEnabled } from "./dialogState";
import { createSettingsSearch, segmentsOf, type SearchResult } from "./search";

export const PACKS: Record<string, { label: string; order: number }> = {
  core: { label: "Bundled", order: 0 },
  phpforge: { label: "PHPForge", order: 1 },
  webforge: { label: "WebForge", order: 2 },
  luaforge: { label: "LuaForge", order: 3 },
  pyforge: { label: "PyForge", order: 4 },
  javaforge: { label: "JavaForge", order: 5 },
  kotlinforge: { label: "KotlinForge", order: 6 },
  scriptforge: { label: "ScriptForge", order: 7 },
  configforge: { label: "ConfigForge", order: 8 },
};

const packLabel = (pack: string) => PACKS[pack]?.label ?? pack;

export function PackBadge(props: { pack: string }) {
  return <span class={`pack-badge pack-${props.pack}`}>{packLabel(props.pack)}</span>;
}

/** Text the plugin search (and the dialog search) matches against. */
export const pluginText = (p: PluginInfo) =>
  [p.name, p.id, p.description, packLabel(p.pack), ...p.contributions.languages].join(" ");

function Marked(props: { text: string; result: SearchResult }) {
  return (
    <For each={segmentsOf(props.result, props.text)}>
      {(s) => (s.hit ? <mark class="set-hl">{s.text}</mark> : s.text)}
    </For>
  );
}

function summary(p: PluginInfo): string[] {
  const c = p.contributions;
  const out: string[] = [];
  if (c.languages.length) out.push(`Languages: ${c.languages.join(", ")}`);
  if (c.fileTypes.length) out.push(`File types: ${c.fileTypes.map((f) => f.name).join(", ")}`);
  if (c.lspServers.length) out.push(`Language servers: ${c.lspServers.map((s) => s.command).join(", ")}`);
  if (c.runConfigurationTypes.length)
    out.push(`Run configurations: ${c.runConfigurationTypes.map((r) => r.name).join(", ")}`);
  if (c.aiProviders.length) out.push(`AI providers: ${c.aiProviders.map((a) => a.name).join(", ")}`);
  if (c.chatAgents.length) out.push(`Chat agents: ${c.chatAgents.map((a) => a.name).join(", ")}`);
  if (c.codeGenerators.length) out.push(`Generate: ${c.codeGenerators.map((g) => g.title).join(", ")}`);
  if (c.toolWindows.length) out.push(`Tool windows: ${c.toolWindows.map((t) => t.title).join(", ")}`);
  if (c.commands.length) out.push(`${c.commands.length} action${c.commands.length > 1 ? "s" : ""}`);
  if (c.settings.length) out.push(`${c.settings.length} setting${c.settings.length > 1 ? "s" : ""}`);
  return out;
}

function PluginIcon(props: { plugin: PluginInfo; size?: "lg" }) {
  return (
    <span class={`plugin-icon pack-${props.plugin.pack}`} classList={{ lg: props.size === "lg" }}>
      <Puzzle size={props.size === "lg" ? 26 : 16} stroke-width={1.75} />
    </span>
  );
}

export function PluginsPage() {
  const [filter, setFilter] = createSignal("");
  const [selectedId, setSelectedId] = createSignal<string | null>(null);

  // Matching and highlighting run in Rust (`settings_search`).
  const search = createSettingsSearch(() => ({
    query: filter(),
    entries: settingsState.plugins.map((p) => ({ id: p.id, text: pluginText(p) })),
    highlight: settingsState.plugins.map((p) => p.name),
  }));
  const groups = createMemo(() => {
    const hits = new Set(search().response.entries);
    const visible = settingsState.plugins.filter((p) => !search().ready || hits.has(p.id));
    const packs = [...new Set(visible.map((p) => p.pack))].sort(
      (a, b) => (PACKS[a]?.order ?? 99) - (PACKS[b]?.order ?? 99),
    );
    return packs.map((pack) => ({ pack, plugins: visible.filter((p) => p.pack === pack) }));
  });
  const selected = () =>
    settingsState.plugins.find((p) => p.id === selectedId()) ?? groups()[0]?.plugins[0];
  const pending = () => Object.keys(pluginDraft).length;
  const depNames = (p: PluginInfo) =>
    p.depends.map((d) => settingsState.plugins.find((q) => q.id === d)?.name ?? d).join(", ");

  return (
    <div class="plugins-page">
      <div class="plugins-tabs">
        <button class="plugins-tab" disabled title="Marketplace is not available yet">
          Marketplace
        </button>
        <button class="plugins-tab active">Installed</button>
        <span class="plugins-count muted">{settingsState.plugins.length} plugins</span>
      </div>
      <div class="plugins-body">
        <div class="plugins-list-pane">
          <div class="set-search plugins-search">
            <Search size={14} class="muted" />
            <input
              placeholder="Search installed plugins"
              value={filter()}
              spellcheck={false}
              onInput={(e) => setFilter(e.currentTarget.value)}
            />
          </div>
          <div class="plugins-list">
            <For each={groups()} fallback={<div class="set-empty">Nothing found</div>}>
              {(g) => (
                <>
                  <div class="plugins-group">
                    {packLabel(g.pack)} <span class="muted">{g.plugins.length}</span>
                  </div>
                  <For each={g.plugins}>
                    {(p) => (
                      <div
                        class="plugin-item"
                        classList={{ selected: selected()?.id === p.id, disabled: !pluginEnabled(p.id) }}
                        data-plugin={p.id}
                        onClick={() => setSelectedId(p.id)}
                      >
                        <PluginIcon plugin={p} />
                        <div class="plugin-item-text">
                          <div class="plugin-item-name">
                            <Marked text={p.name} result={search()} />
                          </div>
                          <div class="plugin-item-meta">
                            <PackBadge pack={p.pack} />
                            <span class="muted">{p.version}</span>
                          </div>
                        </div>
                        <input
                          type="checkbox"
                          title={p.required ? "Required by the IDE" : pluginEnabled(p.id) ? "Disable" : "Enable"}
                          checked={pluginEnabled(p.id)}
                          disabled={p.required}
                          onClick={(e) => e.stopPropagation()}
                          onChange={(e) => setPluginDraftEnabled(p.id, e.currentTarget.checked)}
                        />
                      </div>
                    )}
                  </For>
                </>
              )}
            </For>
          </div>
        </div>
        <Show when={selected()}>
          {(p) => (
            <div class="plugin-details">
              <div class="plugin-details-head">
                <PluginIcon plugin={p()} size="lg" />
                <div>
                  <div class="plugin-details-name">{p().name}</div>
                  <div class="plugin-item-meta">
                    <PackBadge pack={p().pack} />
                    <span class="muted">
                      {p().vendor ? `${p().vendor} · ` : ""}
                      {p().version}
                    </span>
                  </div>
                </div>
              </div>
              <div class="plugin-details-actions">
                <Show
                  when={!p().required}
                  fallback={<span class="muted">Required by the IDE and cannot be disabled.</span>}
                >
                  <button
                    class="btn-secondary"
                    onClick={() => setPluginDraftEnabled(p().id, !pluginEnabled(p().id))}
                  >
                    {pluginEnabled(p().id) ? "Disable" : "Enable"}
                  </button>
                </Show>
              </div>
              <p class="plugin-details-desc">{p().description}</p>
              <Show when={p().depends.length}>
                <div class="plugin-details-row">
                  <span class="muted">Depends on:</span> {depNames(p())}
                </div>
              </Show>
              <ul class="plugin-details-list">
                <For each={summary(p())}>{(line) => <li>{line}</li>}</For>
              </ul>
            </div>
          )}
        </Show>
      </div>
      <Show when={pending()}>
        <div class="plugins-note">
          {pending()} plugin change{pending() > 1 ? "s" : ""} will be applied when you click Apply or OK.
          Dependent plugins are enabled or disabled along with them.
        </div>
      </Show>
    </div>
  );
}
