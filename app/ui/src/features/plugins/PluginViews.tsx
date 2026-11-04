/**
 * Views of Settings | Plugins for third-party plugins: the permission list and prompt, and
 * the Marketplace tab (search, details, install, updates). Search, ranking and the update
 * check run in Rust (`forge_plugins::marketplace`).
 */
import { createResource, createSignal, For, Show } from "solid-js";
import { AlertTriangle, Download, Puzzle, RefreshCw, Search, ShieldCheck } from "lucide-solid";
import { pluginsApi, type MarketplaceEntry, type PermissionLine } from "@/ipc/plugins";
import { busy, closePrompt, acceptPrompt, installFromMarketplace, installed, prompt } from "./store";
import "./plugins.css";

export function PermissionList(props: { lines: PermissionLine[]; added?: string[] }) {
  return (
    <Show when={props.lines.length} fallback={<div class="plg-perm-none">No special permissions: the plugin only adds UI.</div>}>
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
                  {p().update ? "Update" : "Install"} {p().name} {p().version}
                </div>
                <div class="muted">
                  {p().publisher || "Unknown publisher"} · {p().origin}
                </div>
              </div>
            </div>
            <p class="plugin-details-desc">{p().description}</p>
            <div class="plg-prompt-section">This plugin will be able to:</div>
            <PermissionList lines={p().lines} added={p().update ? p().added : undefined} />
            <p class="plg-prompt-note">
              <AlertTriangle size={13} /> Third-party plugins are not reviewed by Forge. They run sandboxed and can only use the
              permissions above. Install plugins only from publishers you trust.
            </p>
            <footer class="plg-prompt-footer">
              <span class="mnt-spacer" />
              <button class="btn-secondary set-btn" onClick={closePrompt}>
                Cancel
              </button>
              <button class="btn-primary set-btn" data-testid="plugin-accept" onClick={() => void acceptPrompt()}>
                {p().update ? "Accept and Update" : "Accept and Install"}
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
            placeholder="Search plugins in Marketplace"
            value={query()}
            spellcheck={false}
            data-testid="marketplace-search"
            onInput={(e) => setQuery(e.currentTarget.value)}
          />
          <button class="icon-btn" title="Refresh" onClick={() => setRefresh((n) => n + 1)}>
            <RefreshCw size={13} />
          </button>
        </div>
        <Show when={view()?.ok?.wanted.length}>
          <div class="plg-wanted">
            Imported settings list {view()!.ok!.wanted.length} plugin{view()!.ok!.wanted.length > 1 ? "s" : ""} that{" "}
            {view()!.ok!.wanted.length > 1 ? "are" : "is"} not installed: {view()!.ok!.wanted.map((w) => w.name || w.id).join(", ")}.
          </div>
        </Show>
        <div class="plugins-list" data-testid="marketplace-list">
          <Show when={!view.loading} fallback={<div class="set-empty">Loading…</div>}>
            <Show when={!view()?.error} fallback={<div class="plg-error">{view()?.error}</div>}>
              <For each={entries()} fallback={<div class="set-empty">Nothing found</div>}>
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
                        <Show when={!isInstalled(e)} fallback={<span class="plg-tag">Installed</span>}>
                          <button class="btn-secondary plg-small" disabled={!e.compatible || busy() === e.id} onClick={(ev) => (ev.stopPropagation(), installFromMarketplace(e, () => setRefresh((n) => n + 1)))}>
                            Install
                          </button>
                        </Show>
                      }
                    >
                      <button class="btn-primary plg-small" onClick={(ev) => (ev.stopPropagation(), installFromMarketplace(e, () => setRefresh((n) => n + 1)))}>
                        Update
                      </button>
                    </Show>
                  </div>
                )}
              </For>
            </Show>
          </Show>
        </div>
        <div class="plg-source muted" title={view()?.ok?.url}>
          <Show when={view()?.ok} fallback={<>Index: not loaded</>}>
            {view()!.ok!.name || "Marketplace"} · format 1
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
                    {e().publisher} · {e().latest.version}
                    {e().latest.released ? ` · ${e().latest.released}` : ""} · {downloads(e().downloads)} downloads
                  </span>
                </div>
              </div>
            </div>
            <div class="plugin-details-actions">
              <Show
                when={e().compatible}
                fallback={<span class="muted">Needs Forge {e().latest.engines.forge}.</span>}
              >
                <Show
                  when={e().updateAvailable || !isInstalled(e())}
                  fallback={<span class="muted">Installed ({e().installedVersion}).</span>}
                >
                  <button class="btn-primary" disabled={busy() === e().id} onClick={() => installFromMarketplace(e(), () => setRefresh((n) => n + 1))}>
                    {busy() === e().id ? "Installing…" : e().updateAvailable ? `Update to ${e().latest.version}` : "Install"}
                  </button>
                </Show>
              </Show>
            </div>
            <p class="plugin-details-desc">{e().description}</p>
            <Show when={e().latest.changes}>
              <div class="plugin-details-row">
                <span class="muted">What's new:</span> {e().latest.changes}
              </div>
            </Show>
            <Show when={e().tags.length}>
              <div class="plugin-details-row">
                <For each={e().tags}>{(t) => <span class="plg-tag">{t}</span>}</For>
              </div>
            </Show>
            <div class="plg-prompt-section">Permissions</div>
            <PermissionList lines={e().permissionLines} />
            <div class="plg-verify muted">
              <ShieldCheck size={13} /> Downloads are checked against SHA-256 {e().latest.sha256.slice(0, 12)}…
              {e().latest.signature ? " and the publisher's signature" : ""}.
            </div>
          </div>
        )}
      </Show>
    </div>
  );
}
