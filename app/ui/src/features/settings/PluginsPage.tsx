/**
 * Settings > Plugins: PHPStorm's "Marketplace" and "Installed" tabs. Installed plugins are
 * grouped by pack, with enable toggles, a search field and a details pane; third-party
 * plugins ("Downloaded") also show their permissions and can be uninstalled. The gear menu
 * has Install Plugin from Disk…, Check for Updates and Marketplace Settings (index URL and
 * the access token of a private marketplace, kept in the OS keychain).
 */
import { createMemo, createResource, createSignal, For, onMount, Show } from "solid-js";
import { Puzzle, Search, Settings as Gear } from "lucide-solid";
import { MarketplaceTab, PermissionList, PermissionPromptView } from "@/features/plugins/PluginViews";
import { checkPluginUpdates, installFromDisk, installed, refreshInstalled, uninstall } from "@/features/plugins/store";
import { getSetting, saveSetting } from "@/state/settings";
import type { PluginInfo } from "@/ipc/settings";
import { pluginsApi } from "@/ipc/plugins";
import { settingsState } from "@/state/settings";
import { pluginEnabled, pluginDraft, setPluginDraftEnabled } from "./dialogState";
import { createSettingsSearch, segmentsOf, type SearchResult } from "./search";
import { showContextMenu } from "@/features/contextMenu/state";
import { t, tOr, t as tr } from "@/i18n";

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

const packLabel = (pack: string) => tOr(`plugins.pack.${pack}`, PACKS[pack]?.label ?? pack);

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
/** `forge_plugins::marketplace::DEFAULT_URL`. */
export const DEFAULT_MARKETPLACE_URL = "https://raw.githubusercontent.com/stdimnet/marketplace/main/index.json";

/** Marketplace Settings: the index URL and a private marketplace's access token, which
 * Rust keeps in the OS keychain (the UI never reads it back). */
function MarketplaceSettings(props: { onClose: (saved: boolean) => void }) {
  const [url, setUrl] = createSignal(getSetting<string>(MARKETPLACE_URL) ?? DEFAULT_MARKETPLACE_URL);
  const [token, setToken] = createSignal("");
  const [removeToken, setRemoveToken] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [status, { refetch }] = createResource(() => pluginsApi.tokenStatus().catch(() => null));
  const save = async () => {
    try {
      const u = url().trim();
      await saveSetting(MARKETPLACE_URL, !u || u === DEFAULT_MARKETPLACE_URL ? null : u, "user");
      if (token().trim()) await pluginsApi.setToken(token().trim());
      else if (removeToken()) await pluginsApi.setToken(null);
      props.onClose(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      void refetch();
    }
  };
  const onKey = (e: KeyboardEvent) => e.key === "Enter" && void save();
  return (
    <div class="plg-url plugins-note" data-testid="marketplace-settings">
      <label for="plg-market-url">{tr("tools.settings.marketplaceIndexUrlJsonIndex")}</label>
      <input id="plg-market-url" data-testid="marketplace-url" value={url()} spellcheck={false} placeholder={DEFAULT_MARKETPLACE_URL} onInput={(e) => setUrl(e.currentTarget.value)} onKeyDown={onKey} />
      <label for="plg-market-token">{tr("tools.settings.marketplaceAccessToken")}</label>
      <input
        id="plg-market-token"
        type="password"
        autocomplete="off"
        data-testid="marketplace-token"
        value={token()}
        placeholder={status()?.set && !removeToken() ? tr("tools.settings.marketplaceTokenStored") : ""}
        onInput={(e) => setToken(e.currentTarget.value)}
        onKeyDown={onKey}
      />
      <span class="muted">
        {status()?.set && !removeToken()
          ? tr("tools.settings.marketplaceTokenIsSet", { scope: status()!.scope })
          : tr("tools.settings.marketplaceTokenHint", { scope: status()?.scope || "—" })}
      </span>
      <Show when={error()}>
        <span class="plg-error-detail">{error()}</span>
      </Show>
      <div class="sync-status-row">
        <button class="btn-primary set-btn" onClick={() => void save()}>
          {tr("common.save")}
        </button>
        <button class="btn-secondary set-btn" onClick={() => props.onClose(false)}>
          {tr("common.cancel")}
        </button>
        <Show when={status()?.set && !removeToken()}>
          <button class="btn-secondary set-btn" data-testid="marketplace-token-remove" onClick={() => (setRemoveToken(true), setToken(""))}>
            {tr("tools.settings.removeMarketplaceToken")}
          </button>
        </Show>
      </div>
    </div>
  );
}

export function PluginsPage() {
  const [tab, setTab] = createSignal<"marketplace" | "installed">("installed");
  const [editUrl, setEditUrl] = createSignal(false);
  // Bumped when Marketplace Settings were saved, so the tab reads the index again.
  const [reload, setReload] = createSignal(0);
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
          {t("plugins.marketplace")}
        </button>
        <button class="plugins-tab" classList={{ active: tab() === "installed" }} data-testid="plugins-tab-installed" onClick={() => setTab("installed")}>
          {t("plugins.installed")}
        </button>
        <span class="plugins-count muted">{t("plugins.count", { count: settingsState.plugins.length })}</span>
        <button
          class="icon-btn plg-gear"
          title={t("plugins.manage")}
          data-testid="plugins-gear"
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            showContextMenu(
              [
                { type: "item", label: t("plugins.installFromDisk"), run: () => void installFromDisk() },
                { type: "item", label: t("plugins.checkUpdates"), run: () => void checkPluginUpdates() },
                { type: "separator" },
                { type: "item", label: t("plugins.marketplaceSettings"), run: () => setEditUrl(true) },
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
        <MarketplaceSettings
          onClose={(saved) => {
            setEditUrl(false);
            if (saved) setReload((n) => n + 1);
          }}
        />
      </Show>
      <Show when={tab() === "marketplace"}>
        <MarketplaceTab reload={reload} onSettings={() => setEditUrl(true)} />
      </Show>
      <div class="plugins-body" style={{ display: tab() === "installed" ? undefined : "none" }}>
        <div class="plugins-list-pane">
          <div class="set-search plugins-search">
            <Search size={14} class="muted" />
            <input
              placeholder={t("plugins.search")}
              value={filter()}
              spellcheck={false}
              onInput={(e) => setFilter(e.currentTarget.value)}
            />
          </div>
          <div class="plugins-list">
            <For each={groups()} fallback={<div class="set-empty">{t("common.nothingFound")}</div>}>
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
                          title={p.required ? t("plugins.requiredShort") : pluginEnabled(p.id) ? t("plugins.disable") : t("plugins.enable")}
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
                  fallback={<span class="muted">{t("plugins.required")}</span>}
                >
                  <button
                    class="btn-secondary"
                    onClick={() => setPluginDraftEnabled(p().id, !pluginEnabled(p().id))}
                  >
                    {pluginEnabled(p().id) ? t("plugins.disable") : t("plugins.enable")}
                  </button>
                </Show>
              </div>
              <p class="plugin-details-desc">{p().description}</p>
              <Show when={installed().find((i) => i.manifest.id === p().id)}>
                {(i) => (
                  <>
                    <div class="plugin-details-row muted">
                      {i().source.kind === "marketplace" ? t("plugins.fromMarketplace") : t("plugins.fromDisk", { file: (i().source as { file: string }).file })}
                      {i().needsTrust ? t("plugins.notInSafeMode") : ""}
                    </div>
                    <div class="plg-prompt-section">{t("plugins.permissions")}</div>
                    <PermissionList lines={i().permissionLines} />
                    <button class="btn-secondary" data-testid="plugin-uninstall" onClick={() => void uninstall(i().manifest.id, i().manifest.name)}>
                      {t("plugins.uninstall")}
                    </button>
                  </>
                )}
              </Show>
              <Show when={p().depends.length}>
                <div class="plugin-details-row">
                  <span class="muted">{t("plugins.dependsOn")}</span> {depNames(p())}
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
          {t("plugins.pending", { count: pending() })}
        </div>
      </Show>
    </div>
  );
}
