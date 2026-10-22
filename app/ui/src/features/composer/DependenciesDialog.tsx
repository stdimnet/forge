/**
 * Tools | Composer | Add Dependency… (PHPStorm's "Manage Composer Dependencies"): search
 * Packagist (name, description, downloads), the package's description and links, the
 * version to install (the recommended constraint first, then the releases), "Development
 * (require-dev)", Install (`composer require`), and the installed packages with Update and
 * Remove. Search and versions come from Rust (Packagist, cached); Composer runs in the Run
 * tool window and the lists reload when it is done.
 */
import { createEffect, createResource, createSignal, For, on, onCleanup, Show } from "solid-js";
import { Download, ExternalLink, Search, Star } from "lucide-solid";
import { composerApi, type ComposerProject, type InstalledPackage, type PackageDetails, type SearchHit } from "@/ipc/composer";
import { suspendKeymap } from "@/keymap";
import { composerRevision, composerRunning, requireCommand, runComposer } from "./state";
import { formatDownloads } from "./model";

const [open, setOpen] = createSignal(false);
export const openDependenciesDialog = () => setOpen(true);
export const dependenciesDialogOpen = open;

const message = (e: unknown) => String(e instanceof Error ? e.message : e);
const openLink = (url: string) => window.open(url, "_blank", "noopener");

interface Row {
  name: string;
  installed?: InstalledPackage;
  constraint?: string;
  dev: boolean;
}

/** Installed packages, the direct requirements first (like PHPStorm's list). */
function installedRows(p: ComposerProject | undefined): Row[] {
  if (!p) return [];
  const rows: Row[] = p.requirements
    .filter((r) => r.name.includes("/"))
    .map((r) => ({ name: r.name, constraint: r.constraint, dev: r.dev, installed: p.installed.find((i) => i.name === r.name) }));
  return rows;
}

function DependenciesDialog() {
  const release = suspendKeymap();
  onCleanup(release);

  const [query, setQuery] = createSignal("");
  const [hits, setHits] = createSignal<SearchHit[]>([]);
  const [searching, setSearching] = createSignal(false);
  const [searchError, setSearchError] = createSignal<string | null>(null);
  const [selected, setSelected] = createSignal<string | null>(null);
  const [version, setVersion] = createSignal("");
  const [dev, setDev] = createSignal(false);
  const [status, setStatus] = createSignal<string | null>(null);
  const [selectedInstalled, setSelectedInstalled] = createSignal<string | null>(null);

  const [project, { refetch }] = createResource(() => composerApi.project().catch(() => undefined));
  createEffect(on(composerRevision, () => void refetch(), { defer: true }));

  const [details] = createResource(selected, (name) => composerApi.package(name).catch((e) => ({ error: message(e) })));
  const info = (): PackageDetails | null => {
    const d = details();
    return d && !("error" in d) ? d : null;
  };
  createEffect(on(info, (d) => setVersion(d?.suggested ?? ""), { defer: true }));

  let timer: ReturnType<typeof setTimeout> | undefined;
  let seq = 0;
  const search = (q: string) => {
    setQuery(q);
    clearTimeout(timer);
    timer = setTimeout(async () => {
      const id = ++seq;
      if (!q.trim()) {
        setHits([]);
        setSearchError(null);
        return;
      }
      setSearching(true);
      try {
        const found = await composerApi.search(q);
        if (id !== seq) return;
        setHits(found);
        setSearchError(null);
        if (found.length && !found.some((h) => h.name === selected())) setSelected(found[0].name);
      } catch (e) {
        if (id === seq) setSearchError(message(e));
      } finally {
        if (id === seq) setSearching(false);
      }
    }, 250);
  };
  onCleanup(() => clearTimeout(timer));

  const rows = () => installedRows(project());
  const isRequired = (name: string | null) => !!name && rows().some((r) => r.name === name);

  const run = async (label: string, command: string) => {
    setStatus(`${label}… (see the Run tool window)`);
    const code = await runComposer(command);
    setStatus(code === 0 ? `${label}: done` : `${label}: Composer exited with code ${code ?? "?"}`);
  };

  const install = () => {
    const name = selected();
    if (!name) return;
    void run(`Installing ${name}`, requireCommand(name, version(), dev()));
  };

  const move = (delta: number) => {
    const list = hits();
    if (!list.length) return;
    const i = list.findIndex((h) => h.name === selected());
    const next = list[Math.max(0, Math.min(list.length - 1, i + delta))];
    setSelected(next.name);
    document.querySelector(`[data-composer-package="${CSS.escape(next.name)}"]`)?.scrollIntoView({ block: "nearest" });
  };

  const close = () => setOpen(false);

  return (
    <div class="popup-backdrop modal">
      <div
        class="dialog composer-dialog"
        role="dialog"
        aria-label="Manage Composer Dependencies"
        onKeyDown={(e) => {
          if (e.key === "Escape") close();
          else return;
          e.preventDefault();
          e.stopPropagation();
        }}
      >
        <div class="dialog-title">Manage Composer Dependencies</div>
        <div class="composer-dialog-body">
          <div class="composer-search">
            <Search size={14} class="composer-search-icon" />
            <input
              ref={(el) => queueMicrotask(() => el.focus())}
              placeholder="Search for packages on Packagist"
              value={query()}
              spellcheck={false}
              onInput={(e) => search(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") move(1);
                else if (e.key === "ArrowUp") move(-1);
                else if (e.key === "Enter") install();
                else return;
                e.preventDefault();
              }}
            />
          </div>
          <div class="composer-panes">
            <div class="composer-pane composer-results">
              <div class="composer-pane-title">Available Packages</div>
              <div class="composer-list" role="listbox">
                <Show when={!searchError()} fallback={<div class="composer-empty">{searchError()}</div>}>
                  <Show
                    when={hits().length}
                    fallback={<div class="composer-empty">{searching() ? "Searching…" : query().trim() ? "No packages found" : "Type to search Packagist"}</div>}
                  >
                    <For each={hits()}>
                      {(h) => (
                        <div
                          class="composer-hit"
                          classList={{ selected: selected() === h.name }}
                          role="option"
                          data-composer-package={h.name}
                          onClick={() => setSelected(h.name)}
                          onDblClick={install}
                        >
                          <div class="composer-hit-head">
                            <span class="composer-hit-name">{h.name}</span>
                            <Show when={isRequired(h.name)}>
                              <span class="composer-tag">installed</span>
                            </Show>
                            <Show when={h.abandoned}>
                              <span class="composer-tag warn">abandoned</span>
                            </Show>
                            <span class="composer-hit-stats">
                              <Show when={h.downloads}>
                                <Download size={11} />
                                {formatDownloads(h.downloads)}
                              </Show>
                              <Show when={h.favers}>
                                <Star size={11} />
                                {formatDownloads(h.favers)}
                              </Show>
                            </span>
                          </div>
                          <div class="composer-hit-desc">{h.description}</div>
                        </div>
                      )}
                    </For>
                  </Show>
                </Show>
              </div>
            </div>
            <div class="composer-pane composer-details">
              <div class="composer-pane-title">Description</div>
              <Show when={selected()} fallback={<div class="composer-empty">Select a package</div>}>
                <div class="composer-details-body">
                  <div class="composer-details-name">{selected()}</div>
                  <Show when={details.loading}>
                    <div class="composer-muted">Loading…</div>
                  </Show>
                  <Show when={details() && "error" in details()! ? (details() as { error: string }).error : null}>
                    {(err) => <div class="composer-muted">{err()}</div>}
                  </Show>
                  <Show when={info()}>
                    {(d) => (
                      <>
                        <div class="composer-details-desc">{d().description || "No description"}</div>
                        <Show when={d().abandoned !== undefined && d().abandoned !== null}>
                          <div class="composer-warning">
                            Abandoned{d().abandoned ? `, use ${d().abandoned} instead` : ""}
                          </div>
                        </Show>
                        <div class="composer-links">
                          <Show when={d().homepage}>
                            <a class="composer-link" href={d().homepage} onClick={(e) => (e.preventDefault(), openLink(d().homepage!))}>
                              Homepage <ExternalLink size={11} />
                            </a>
                          </Show>
                          <Show when={d().repository}>
                            <a class="composer-link" href={d().repository} onClick={(e) => (e.preventDefault(), openLink(d().repository!))}>
                              Source <ExternalLink size={11} />
                            </a>
                          </Show>
                          <a class="composer-link" href={d().url} onClick={(e) => (e.preventDefault(), openLink(d().url))}>
                            Packagist <ExternalLink size={11} />
                          </a>
                        </div>
                      </>
                    )}
                  </Show>
                  <label class="form-label composer-version-label" for="composer-version">
                    Version to install:
                  </label>
                  <select id="composer-version" value={version()} onChange={(e) => setVersion(e.currentTarget.value)}>
                    <option value="">Latest (Composer chooses)</option>
                    <Show when={info()?.suggested}>
                      {(s) => <option value={s()}>{s()} (recommended)</option>}
                    </Show>
                    <For each={info()?.versions ?? []}>{(v) => <option value={v}>{v}</option>}</For>
                  </select>
                  <label class="composer-check">
                    <input type="checkbox" checked={dev()} onChange={(e) => setDev(e.currentTarget.checked)} />
                    Development (require-dev)
                  </label>
                  <div>
                    <button class="btn-primary" disabled={composerRunning() || !selected()} onClick={install}>
                      Install
                    </button>
                  </div>
                </div>
              </Show>
            </div>
          </div>
          <div class="composer-pane composer-installed">
            <div class="composer-pane-title">
              Installed Packages
              <Show when={project() && !project()!.hasLock}>
                <span class="composer-muted"> · composer.lock is missing, run Install</span>
              </Show>
            </div>
            <div class="composer-installed-list">
              <Show when={rows().length} fallback={<div class="composer-empty">{project.loading ? "Loading…" : "No packages are required"}</div>}>
                <table class="composer-table">
                  <thead>
                    <tr>
                      <th>Package</th>
                      <th>Constraint</th>
                      <th>Installed</th>
                      <th />
                    </tr>
                  </thead>
                  <tbody>
                    <For each={rows()}>
                      {(r) => (
                        <tr
                          classList={{ selected: selectedInstalled() === r.name }}
                          data-composer-installed={r.name}
                          onClick={() => setSelectedInstalled(r.name)}
                        >
                          <td>
                            {r.name}
                            <Show when={r.dev}>
                              <span class="composer-tag">dev</span>
                            </Show>
                          </td>
                          <td class="composer-mono">{r.constraint}</td>
                          <td class="composer-mono">{r.installed?.version ?? "not installed"}</td>
                          <td class="composer-row-actions">
                            <button class="link-btn" disabled={composerRunning()} onClick={() => void run(`Updating ${r.name}`, `update ${r.name} --with-dependencies`)}>
                              Update
                            </button>
                            <button class="link-btn" disabled={composerRunning()} onClick={() => void run(`Removing ${r.name}`, `remove ${r.name}${r.dev ? " --dev" : ""}`)}>
                              Remove
                            </button>
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </Show>
            </div>
          </div>
        </div>
        <div class="dialog-buttons">
          <span class="composer-status">{status()}</span>
          <button class="btn-secondary" onClick={close}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}

export function DependenciesDialogHost() {
  return (
    <Show when={open()}>
      <DependenciesDialog />
    </Show>
  );
}
