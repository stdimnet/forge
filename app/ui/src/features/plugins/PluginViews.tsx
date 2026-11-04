/**
 * Views of Settings | Plugins for third-party plugins: the permission list and prompt, and
 * the Marketplace tab (search, details, install, updates). Search, ranking and the update
 * check run in Rust (`forge_plugins::marketplace`).
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { AlertTriangle, Download, Puzzle, RefreshCw, Search, ShieldCheck } from "lucide-solid";
import { pluginsApi, type MarketplaceEntry, type PermissionLine } from "@/ipc/plugins";
import { busy, closePrompt, acceptPrompt, installFromMarketplace, installed, prompt } from "./store";
import { t as tr } from "@/i18n";
import "./plugins.css";

export function PermissionList(props: { lines: PermissionLine[]; added?: string[] }) {
  return (
    <Show when={props.lines.length} fallback={<div class="plg-perm-none">{tr("tools.plugins.noSpecialPermissionsThePlugin")}</div>}>
      <ul class="plg-perms" data-testid="plugin-permissions">
        <For each={props.lines}>
          {(l) => (
            <li class={`plg-perm risk-${l.risk}`} data-permission={l.kind}>
              <span class="plg-perm-dot" />
              <div>
                <div class="plg-perm-label">
                  {l.label}
                  <Show when={props.added?.includes(l.kind)}>
                    <span class="plg-perm-new">new</span>
                  </Show>
                </div>
                <div class="plg-perm-detail">{l.detail}</div>
              </div>
            </li>
          )}
        </For>
      </ul>
    </Show>
  );
}

/** The install / update prompt, over the Plugins page. */
export function PermissionPromptView() {
  return (
    <Show when={prompt()}>
      {(p) => (
        <div class="plg-prompt-backdrop" data-testid="plugin-permission-prompt">
          <div class="plg-prompt" role="dialog" aria-label={`Install ${p().name}`}>
            <div class="plg-prompt-head">
              <span class="plugin-icon lg pack-thirdParty">
                <Puzzle size={26} stroke-width={1.75} />
              </span>
              <div>
                <div class="plugin-details-name">
                  {p().update ? tr("common.update") : tr("common.install")} {p().name} {p().version}
                </div>
                <div class="muted">
                  {p().publisher || tr("tools.plugins.unknownPublisher")} · {p().origin}
                </div>
              </div>
            </div>
            <p class="plugin-details-desc">{p().description}</p>
            <div class="plg-prompt-section">{tr("tools.plugins.thisPluginWillBeAbleLabel")}</div>
            <PermissionList lines={p().lines} added={p().update ? p().added : undefined} />
            <p class="plg-prompt-note">
              <AlertTriangle size={13} /> {tr("tools.plugins.thirdPartyPluginsAreNot")}
            </p>
            <footer class="plg-prompt-footer">
              <span class="mnt-spacer" />
              <button class="btn-secondary set-btn" onClick={closePrompt}>
                {tr("common.cancel")}
              </button>
              <button class="btn-primary set-btn" data-testid="plugin-accept" onClick={() => void acceptPrompt()}>
                {p().update ? tr("tools.plugins.acceptAndUpdate") : tr("tools.plugins.acceptAndInstall")}
              </button>
            </footer>
          </div>
        </div>
      )}
    </Show>
  );
}

const downloads = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}K` : String(n));

export function MarketplaceTab() {
  const [query, setQuery] = createSignal("");
  const [refresh, setRefresh] = createSignal(0);
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [view] = createResource(
    () => ({ q: query(), r: refresh() }),
    async ({ q, r }) => {
      try {
        return { ok: await pluginsApi.marketplace(q, r > 0), error: null };
      } catch (e) {
        return { ok: null, error: e instanceof Error ? e.message : String(e) };
      }
    },
  );
  const entries = () => view()?.ok?.entries ?? [];
  const selected = () => entries().find((e) => e.id === selectedId()) ?? entries()[0];
  const isInstalled = (e: MarketplaceEntry) => installed().some((p) => p.manifest.id === e.id) || !!e.installedVersion;

  return (
    <div class="plugins-body">
      <div class="plugins-list-pane">
        <div class="set-search plugins-search">
          <Search size={14} class="muted" />
          <input
            placeholder={tr("tools.plugins.searchPluginsInMarketplace")}
            value={query()}
            spellcheck={false}
            data-testid="marketplace-search"
            onInput={(e) => setQuery(e.currentTarget.value)}
          />
          <button class="icon-btn" title={tr("common.refresh")} onClick={() => setRefresh((n) => n + 1)}>
            <RefreshCw size={13} />
          </button>
        </div>
        <Show when={view()?.ok?.wanted.length}>
          <div class="plg-wanted">
            {tr("tools.plugins.importedSettingsListPluginThatLabel", { count: view()!.ok!.wanted.length, value: view()!.ok!.wanted.length > 1 ? "s" : "", value2: view()!.ok!.wanted.length > 1 ? "are" : "is" })} {view()!.ok!.wanted.map((w) => w.name || w.id).join(", ")}.
          </div>
        </Show>
        <div class="plugins-list" data-testid="marketplace-list">
          <Show when={!view.loading} fallback={<div class="set-empty">{tr("common.loadingEllipsis")}</div>}>
            <Show when={!view()?.error} fallback={<div class="plg-error">{view()?.error}</div>}>
              <For each={entries()} fallback={<div class="set-empty">{tr("common.nothingFound")}</div>}>
                {(e) => (
                  <div class="plugin-item" classList={{ selected: selected()?.id === e.id }} data-plugin={e.id} onClick={() => setSelectedId(e.id)}>
                    <span class="plugin-icon pack-thirdParty">
                      <Puzzle size={16} stroke-width={1.75} />
                    </span>
                    <div class="plugin-item-text">
                      <div class="plugin-item-name">{e.name}</div>
                      <div class="plugin-item-meta">
                        <span class="muted">{e.publisher}</span>
                        <span class="muted">
                          <Download size={10} /> {downloads(e.downloads)}
                        </span>
                      </div>
                    </div>
                    <Show
                      when={e.updateAvailable}
                      fallback={
                        <Show when={!isInstalled(e)} fallback={<span class="plg-tag">{tr("common.installed")}</span>}>
                          <button class="btn-secondary plg-small" disabled={!e.compatible || busy() === e.id} onClick={(ev) => (ev.stopPropagation(), installFromMarketplace(e, () => setRefresh((n) => n + 1)))}>
                            {tr("common.install")}
                          </button>
                        </Show>
                      }
                    >
                      <button class="btn-primary plg-small" onClick={(ev) => (ev.stopPropagation(), installFromMarketplace(e, () => setRefresh((n) => n + 1)))}>
                        {tr("common.update")}
                      </button>
                    </Show>
                  </div>
                )}
              </For>
            </Show>
          </Show>
        </div>
        <div class="plg-source muted" title={view()?.ok?.url}>
          <Show when={view()?.ok} fallback={<>{tr("tools.plugins.indexNotLoaded")}</>}>
            {tr("tools.plugins.format1", { value: view()!.ok!.name || tr("tools.plugins.marketplace") })}
          </Show>
        </div>
      </div>
      <Show when={selected()} fallback={<div class="plugin-details" />}>
        {(e) => (
          <div class="plugin-details" data-testid="marketplace-details">
            <div class="plugin-details-head">
              <span class="plugin-icon lg pack-thirdParty">
                <Puzzle size={26} stroke-width={1.75} />
              </span>
              <div>
                <div class="plugin-details-name">{e().name}</div>
                <div class="plugin-item-meta">
                  <span class="muted">
                    {tr("tools.plugins.downloads", { publisher: e().publisher, version: e().latest.version, value: e().latest.released ? ` · ${e().latest.released}` : "", value2: downloads(e().downloads) })}
                  </span>
                </div>
              </div>
            </div>
            <div class="plugin-details-actions">
              <Show
                when={e().compatible}
                fallback={<span class="muted">{tr("tools.plugins.needsForge", { forge: e().latest.engines.forge })}</span>}
              >
                <Show
                  when={e().updateAvailable || !isInstalled(e())}
                  fallback={<span class="muted">{tr("tools.plugins.installed", { installedVersion: e().installedVersion })}</span>}
                >
                  <button class="btn-primary" disabled={busy() === e().id} onClick={() => installFromMarketplace(e(), () => setRefresh((n) => n + 1))}>
                    {busy() === e().id ? tr("tools.plugins.installingEllipsis") : e().updateAvailable ? tr("tools.plugins.updateTo", { version: e().latest.version }) : tr("common.install")}
                  </button>
                </Show>
              </Show>
            </div>
            <p class="plugin-details-desc">{e().description}</p>
            <Show when={e().latest.changes}>
              <div class="plugin-details-row">
                <span class="muted">{tr("tools.plugins.whatSNewLabel")}</span> {e().latest.changes}
              </div>
            </Show>
            <Show when={e().tags.length}>
              <div class="plugin-details-row">
                <For each={e().tags}>{(t) => <span class="plg-tag">{t}</span>}</For>
              </div>
            </Show>
            <div class="plg-prompt-section">{tr("tools.plugins.permissions")}</div>
            <PermissionList lines={e().permissionLines} />
            <div class="plg-verify muted">
              <ShieldCheck size={13} /> {tr("tools.plugins.downloadsAreCheckedAgainstSha", { sha256: e().latest.sha256.slice(0, 12), value: e().latest.signature ? tr("tools.plugins.andThePublisherSSignature") : "" })}
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}
