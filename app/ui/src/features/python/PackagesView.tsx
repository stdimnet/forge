/**
 * The Python Packages tool window, laid out like PyCharm's: a search field and the
 * packages on the left (installed ones with their version and the latest PyPI release,
 * PyPI search results below), the selected package on the right (summary, versions,
 * Install / Upgrade / Uninstall with pip, uv, Poetry or Pipenv, whichever manages the
 * environment). Everything comes from Rust (`python_packages_*`).
 */
import { createEffect, createMemo, createResource, createSignal, For, on, onCleanup, Show } from "solid-js";
import { ArrowUpCircle, ExternalLink, Loader, Package, RefreshCw, Search, Settings2 } from "lucide-solid";
import { pythonApi, type PypiPackage, type SearchHit } from "@/ipc/python";
import { openInBrowser } from "@/features/run/runStore";
import { openSettings } from "@/features/settings/dialogState";
import {
  currentInterpreter,
  listError,
  packageAction,
  packageListing,
  packagesLoading,
  packagesRunning,
  refreshInterpreter,
  refreshPackages,
} from "./state";

const norm = (n: string) => n.toLowerCase().replace(/[-_.]+/g, "-");

/** Compare two versions roughly (numbers numerically), enough to mark updates. */
export function newer(latest: string | undefined, installed: string): boolean {
  if (!latest || latest === installed) return false;
  const parts = (v: string) => v.split(/[.+-]/).map((p) => (/^\d+$/.test(p) ? Number(p) : p));
  const a = parts(latest);
  const b = parts(installed);
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const x = a[i] ?? 0;
    const y = b[i] ?? 0;
    if (x === y) continue;
    if (typeof x === "number" && typeof y === "number") return x > y;
    return String(x) > String(y);
  }
  return false;
}

export function PackagesView() {
  const [query, setQuery] = createSignal("");
  const [selected, setSelected] = createSignal<string | null>(null);
  const [latest, setLatest] = createSignal<Record<string, string>>({});
  const [hits, setHits] = createSignal<SearchHit[]>([]);
  const [searching, setSearching] = createSignal(false);
  const [searchError, setSearchError] = createSignal<string | null>(null);
  const [version, setVersion] = createSignal("");

  // Latest releases of the installed packages.
  createEffect(
    on(packageListing, (l) => {
      if (!l?.packages.length) return;
      void pythonApi
        .latest(l.packages.map((p) => p.name))
        .then((map) => setLatest(Object.fromEntries(Object.entries(map).map(([k, v]) => [norm(k), v]))))
        .catch(() => undefined);
    }),
  );

  // PyPI search, debounced.
  let timer: ReturnType<typeof setTimeout> | undefined;
  createEffect(
    on(query, (q) => {
      clearTimeout(timer);
      if (q.trim().length < 2) {
        setHits([]);
        setSearchError(null);
        return;
      }
      timer = setTimeout(() => {
        setSearching(true);
        pythonApi
          .search(q.trim())
          .then((h) => {
            setHits(h);
            setSearchError(null);
          })
          .catch((e) => setSearchError(String(e)))
          .finally(() => setSearching(false));
      }, 300);
    }),
  );
  onCleanup(() => clearTimeout(timer));

  const installed = createMemo(() => {
    const q = norm(query().trim());
    return (packageListing()?.packages ?? []).filter((p) => !q || norm(p.name).includes(q));
  });
  const installedNames = createMemo(() => new Set((packageListing()?.packages ?? []).map((p) => norm(p.name))));
  const remote = () => hits().filter((h) => !installedNames().has(norm(h.name)));
  const installedOf = (name: string) => packageListing()?.packages.find((p) => norm(p.name) === norm(name));

  const [details] = createResource(selected, (name) => pythonApi.packageInfo(name).catch((): PypiPackage | null => null));
  createEffect(on(details, (d) => setVersion(d?.version ?? "")));

  const busy = () => packagesRunning() > 0;
  const install = (name: string) => {
    const v = version().trim();
    const spec = v && details()?.version !== v ? `${name}==${v}` : name;
    void packageAction("install", [spec]);
  };

  const managerLabel = () => {
    const m = packageListing()?.manager;
    return m === "uv" ? "uv" : m === "poetry" ? "Poetry" : m === "pipenv" ? "Pipenv" : "pip";
  };

  return (
    <div class="py-packages" data-testid="python-packages">
      <div class="py-packages-left">
        <div class="py-packages-toolbar">
          <div class="py-packages-search">
            <Search size={13} />
            <input
              type="text"
              placeholder="Search for more packages"
              value={query()}
              onInput={(e) => setQuery(e.currentTarget.value)}
              data-testid="python-packages-search"
            />
            <Show when={searching()}>
              <Loader size={13} class="spin" />
            </Show>
          </div>
          <button class="icon-btn" title="Reload List of Packages" onClick={() => void refreshInterpreter()}>
            <RefreshCw size={14} />
          </button>
          <button class="icon-btn" title="Python Interpreter Settings" onClick={() => openSettings("project.python-interpreter")}>
            <Settings2 size={14} />
          </button>
        </div>
        <div class="py-packages-list">
          <Show
            when={currentInterpreter()}
            fallback={
              <div class="py-packages-empty muted">
                No Python interpreter is configured for the project.{" "}
                <button class="link-btn" onClick={() => openSettings("project.python-interpreter")}>
                  Configure Python interpreter
                </button>
              </div>
            }
          >
            <div class="py-packages-group">
              <span>Installed</span>
              <span class="muted">{currentInterpreter()!.label}</span>
              <Show when={packagesLoading()}>
                <Loader size={12} class="spin" />
              </Show>
            </div>
            <Show when={listError()}>
              <div class="py-packages-error">{listError()}</div>
            </Show>
            <For each={installed()}>
              {(p) => (
                <div
                  class="tree-row py-package-row"
                  classList={{ selected: selected() === p.name }}
                  data-package={p.name}
                  onClick={() => setSelected(p.name)}
                >
                  <Package size={13} class="py-package-icon" />
                  <span class="py-package-name">{p.name}</span>
                  <span class="py-package-version">{p.version}</span>
                  <Show when={newer(latest()[norm(p.name)], p.version)}>
                    <span class="py-package-latest" title={`Latest: ${latest()[norm(p.name)]}`}>
                      <ArrowUpCircle size={12} /> {latest()[norm(p.name)]}
                    </span>
                  </Show>
                </div>
              )}
            </For>
            <Show when={query().trim().length >= 2}>
              <div class="py-packages-group">
                <span>PyPI</span>
              </div>
              <Show when={searchError()}>
                <div class="py-packages-error">{searchError()}</div>
              </Show>
              <For each={remote()}>
                {(h) => (
                  <div
                    class="tree-row py-package-row"
                    classList={{ selected: selected() === h.name }}
                    data-package={h.name}
                    onClick={() => setSelected(h.name)}
                  >
                    <Package size={13} class="py-package-icon remote" />
                    <span class="py-package-name">{h.name}</span>
                    <span class="py-package-version muted">{h.version}</span>
                  </div>
                )}
              </For>
            </Show>
          </Show>
        </div>
      </div>
      <div class="py-packages-detail">
        <Show when={selected()} fallback={<div class="py-packages-placeholder muted">Select a package to view its documentation</div>}>
          {(name) => (
            <>
              <div class="py-detail-head">
                <span class="py-detail-name">{details()?.name || name()}</span>
                <div class="py-detail-actions">
                  <Show
                    when={installedOf(name())}
                    fallback={
                      <>
                        <select class="py-detail-versions" value={version()} onChange={(e) => setVersion(e.currentTarget.value)} title="Version">
                          <For each={details()?.versions.slice(0, 50) ?? []}>{(v) => <option value={v}>{v}</option>}</For>
                        </select>
                        <button class="btn-primary" disabled={busy()} onClick={() => install(name())} data-testid="python-package-install">
                          Install with {managerLabel()}
                        </button>
                      </>
                    }
                  >
                    {(p) => (
                      <>
                        <span class="muted">{p().version}</span>
                        <Show when={newer(details()?.version, p().version)}>
                          <button class="btn-primary" disabled={busy()} onClick={() => void packageAction("upgrade", [p().name])}>
                            Upgrade to {details()?.version}
                          </button>
                        </Show>
                        <button class="btn-secondary" disabled={busy()} onClick={() => void packageAction("uninstall", [p().name])}>
                          Delete Package
                        </button>
                      </>
                    )}
                  </Show>
                </div>
              </div>
              <Show when={details.loading}>
                <div class="muted py-detail-loading">Loading package information…</div>
              </Show>
              <Show when={details()}>
                {(d) => (
                  <div class="py-detail-body">
                    <p class="py-detail-summary">{d().summary || "No description"}</p>
                    <dl class="py-detail-meta">
                      <Show when={d().version}>
                        <dt>Latest version</dt>
                        <dd>{d().version}</dd>
                      </Show>
                      <Show when={d().requiresPython}>
                        <dt>Requires Python</dt>
                        <dd>{d().requiresPython}</dd>
                      </Show>
                      <Show when={d().license}>
                        <dt>License</dt>
                        <dd>{d().license}</dd>
                      </Show>
                      <Show when={d().author}>
                        <dt>Author</dt>
                        <dd>{d().author}</dd>
                      </Show>
                    </dl>
                    <Show when={d().homePage}>
                      <button class="link-btn py-detail-link" onClick={() => openInBrowser(d().homePage)}>
                        <ExternalLink size={12} /> {d().homePage}
                      </button>
                    </Show>
                  </div>
                )}
              </Show>
            </>
          )}
        </Show>
      </div>
    </div>
  );
}

/** Header buttons of the tool window. */
export function PackagesActions() {
  return (
    <button class="icon-btn" title="Reload List of Packages" onClick={() => void refreshPackages()}>
      <RefreshCw size={14} />
    </button>
  );
}
