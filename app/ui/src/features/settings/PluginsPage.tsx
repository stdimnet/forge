/**
 * Settings > Plugins: PHPStorm's "Marketplace" and "Installed" tabs. Installed plugins are
 * grouped by pack, with enable toggles, a search field and a details pane; third-party
 * plugins ("Downloaded") also show their permissions and can be uninstalled. The gear menu
 * has Install Plugin from Disk…, Check for Updates and the marketplace URL.
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { Puzzle, Search, Settings as Gear } from "lucide-solid";
import { MarketplaceTab, PermissionList, PermissionPromptView } from "@/features/plugins/PluginViews";
import { checkPluginUpdates, installFromDisk, installed, refreshInstalled, uninstall } from "@/features/plugins/store";
import { getSetting, saveSetting } from "@/state/settings";
import type { PluginInfo } from "@/ipc/settings";
import { settingsState } from "@/state/settings";
import { pluginEnabled, pluginDraft, setPluginDraftEnabled } from "./dialogState";
import { createSettingsSearch, segmentsOf, type SearchResult } from "./search";
import { showContextMenu } from "@/features/contextMenu/state";

export const PACKS: Record<string, { label: string; order: number }> = {
  thirdParty: { label: "Downloaded", order: -1 },
  core: { label: "Bundled", order: 0 },
  phpforge: { label: "PHPForge", order: 1 },
  webforge: { label: "WebForge", order: 2 },
  luaforge: { label: "LuaForge", order: 3 },
  pyforge: { label: "PyForge", order: 4 },
  goforge: { label: "GoForge", order: 5 },
  rustforge: { label: "RustForge", order: 6 },
  rubyforge: { label: "RubyForge", order: 7 },
  javaforge: { label: "JavaForge", order: 8 },
  kotlinforge: { label: "KotlinForge", order: 9 },
  cforge: { label: "CForge", order: 10 },
  dotnetforge: { label: "DotNetForge", order: 11 },
  scriptforge: { label: "ScriptForge", order: 11 },
  configforge: { label: "ConfigForge", order: 12 },
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
  if (c.lspServers.length) out.push(`Language servers: ${c.lspServers.map((s) => s.id).join(", ")}`);
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

const MARKETPLACE_URL = "plugins.marketplace.url";

function MarketplaceUrlEditor(props: { onClose: () => void }) {
  const [url, setUrl] = createSignal(getSetting<string>(MARKETPLACE_URL) ?? "");
  const save = () => {
    void saveSetting(MARKETPLACE_URL, url().trim() || null, "user");
    props.onClose();
  };
  return (
    <div class="plg-url plugins-note" data-testid="marketplace-url">
      <span>Marketplace index URL (JSON, index format 1). The default plugins.forge.example is a placeholder: no public Forge marketplace is live yet.</span>
      <input value={url()} spellcheck={false} onInput={(e) => setUrl(e.currentTarget.value)} onKeyDown={(e) => e.key === "Enter" && save()} />
      <div class="sync-status-row">
        <button class="btn-primary set-btn" onClick={save}>
          Save
        </button>
        <button class="btn-secondary set-btn" onClick={props.onClose}>
          Cancel
        </button>
      </div>
    </div>
  );
}

export function PluginsPage() {
  const [tab, setTab] = createSignal<"marketplace" | "installed">("installed");
  const [editUrl, setEditUrl] = createSignal(false);
  onMount(() => void refreshInstalled());
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
        <button class="plugins-tab" classList={{ active: tab() === "marketplace" }} data-testid="plugins-tab-marketplace" onClick={() => setTab("marketplace")}>
          Marketplace
        </button>
        <button class="plugins-tab" classList={{ active: tab() === "installed" }} data-testid="plugins-tab-installed" onClick={() => setTab("installed")}>
          Installed
        </button>
        <span class="plugins-count muted">{settingsState.plugins.length} plugins</span>
        <button
          class="icon-btn plg-gear"
          title="Manage plugins"
          data-testid="plugins-gear"
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            showContextMenu(
              [
                { type: "item", label: "Install Plugin from Disk…", run: () => void installFromDisk() },
                { type: "item", label: "Check for Updates", run: () => void checkPluginUpdates() },
                { type: "separator" },
                { type: "item", label: "Marketplace URL…", run: () => setEditUrl(true) },
              ],
              r.left,
              r.bottom + 2,
            );
          }}
        >
          <Gear size={14} />
        </button>
      </div>
      <Show when={editUrl()}>
        <MarketplaceUrlEditor onClose={() => setEditUrl(false)} />
      </Show>
      <Show when={tab() === "marketplace"}>
        <MarketplaceTab />
      </Show>
      <div class="plugins-body" style={{ display: tab() === "installed" ? undefined : "none" }}>
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
              <Show when={installed().find((i) => i.manifest.id === p().id)}>
                {(i) => (
                  <>
                    <div class="plugin-details-row muted">
                      {i().source.kind === "marketplace" ? "From the marketplace" : `Installed from disk: ${(i().source as { file: string }).file}`}
                      {i().needsTrust ? " · not loaded in Safe Mode" : ""}
                    </div>
                    <div class="plg-prompt-section">Permissions</div>
                    <PermissionList lines={i().permissionLines} />
                    <button class="btn-secondary" data-testid="plugin-uninstall" onClick={() => void uninstall(i().manifest.id, i().manifest.name)}>
                      Uninstall
                    </button>
                  </>
                )}
              </Show>
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
      <PermissionPromptView />
      <Show when={pending()}>
        <div class="plugins-note">
          {pending()} plugin change{pending() > 1 ? "s" : ""} will be applied when you click Apply or OK.
          Dependent plugins are enabled or disabled along with them.
        </div>
      </Show>
    </div>
  );
}
