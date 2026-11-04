/**
 * The NuGet tool window, like Rider's: search the feed (nuget.org v3 through Rust,
 * `nuget_search`) or browse the solution's installed packages, with the upgrades the feed
 * knows (`nuget_updates`); the details of the selected package show its versions and every
 * project with the version it uses, with Install, Upgrade and Remove (`dotnet add|remove
 * package`, run in the Run tool window, so Safe Mode refuses them).
 */
import { createEffect, createMemo, createSignal, For, on, onMount, Show } from "solid-js";
import { ArrowUp, BadgeCheck, ExternalLink, Package, RefreshCw, Search, TriangleAlert } from "lucide-solid";
import { dotnetApi, type NuGetPackage, type PackageUpdate } from "@/ipc/dotnet";
import { getSetting } from "@/state/settings";
import { dotnetWorkspace, errorText, refreshSolution, runDotnetCommand, showDotnetBalloon } from "./state";
import { t } from "@/i18n";
import "./dotnet.css";

interface Entry {
  id: string;
  version: string;
  description: string;
  authors?: string;
  downloads?: number;
  verified?: boolean;
  deprecated?: boolean;
  vulnerable?: boolean;
  projectUrl?: string | null;
  versions?: string[];
  installed: boolean;
}

const [updates, setUpdates] = createSignal<PackageUpdate[]>([]);
export { updates as nugetUpdates };

export async function refreshUpdates(): Promise<void> {
  try {
    setUpdates(await dotnetApi.nugetUpdates());
  } catch {
    setUpdates([]);
  }
}

const shortCount = (n?: number) => {
  if (n == null) return "";
  if (n >= 1e9) return `${(n / 1e9).toFixed(1).replace(/\.0$/, "")}B`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(1).replace(/\.0$/, "")}M`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(1).replace(/\.0$/, "")}K`;
  return String(n);
};

/** The installed packages of the solution (one entry per id). */
function installedEntries(): Entry[] {
  const map = new Map<string, Entry>();
  for (const p of dotnetWorkspace()?.projects ?? [])
    for (const r of p.packages) {
      const e = map.get(r.id.toLowerCase());
      if (!e) map.set(r.id.toLowerCase(), { id: r.id, version: r.version, description: "", installed: true });
      else if (r.version && e.version !== r.version && !e.version.includes(r.version)) e.version = [e.version, r.version].filter(Boolean).join(", ");
    }
  return [...map.values()].sort((a, b) => a.id.localeCompare(b.id));
}

export function NuGetToolWindow() {
  const [query, setQuery] = createSignal("");
  const [results, setResults] = createSignal<Entry[] | null>(null);
  const [total, setTotal] = createSignal(0);
  const [searching, setSearching] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [prerelease, setPrerelease] = createSignal(getSetting<boolean>("dotnet.nuget.prerelease", false));
  const [tab, setTab] = createSignal<"packages" | "updates">("packages");
  const [selectedId, setSelectedId] = createSignal<string | null>(null);
  const [versions, setVersions] = createSignal<string[]>([]);
  const [version, setVersion] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  onMount(() => {
    if (!dotnetWorkspace()) void refreshSolution().then(() => refreshUpdates());
    else void refreshUpdates();
  });
  let timer: ReturnType<typeof setTimeout> | undefined;
  const search = async () => {
    const q = query().trim();
    if (!q) {
      setResults(null);
      setError(null);
      return;
    }
    setSearching(true);
    try {
      const r = await dotnetApi.nugetSearch(q, prerelease());
      const installed = new Set(installedEntries().map((e) => e.id.toLowerCase()));
      setTotal(r.totalHits);
      setResults(
        r.packages.map((p: NuGetPackage) => ({
          id: p.id,
          version: p.version,
          description: p.description,
          authors: p.authors,
          downloads: p.totalDownloads,
          verified: p.verified,
          deprecated: p.deprecated,
          vulnerable: p.vulnerable,
          projectUrl: p.projectUrl,
          versions: p.versions,
          installed: installed.has(p.id.toLowerCase()),
        })),
      );
      setError(null);
    } catch (e) {
      setError(errorText(e));
      setResults([]);
    } finally {
      setSearching(false);
    }
  };
  const onInput = (v: string) => {
    setQuery(v);
    clearTimeout(timer);
    timer = setTimeout(() => void search(), 350);
  };
  const entries = createMemo<Entry[]>(() => {
    if (tab() === "updates")
      return updates().map((u) => ({ id: u.id, version: u.installed, description: `${u.installed} → ${u.latest}`, installed: true }));
    return results() ?? installedEntries();
  });
  const selected = createMemo(() => entries().find((e) => e.id === selectedId()) ?? installedEntries().find((e) => e.id === selectedId()) ?? null);
  const updateOf = (id: string) => updates().find((u) => u.id.toLowerCase() === id.toLowerCase());
  createEffect(
    on(selectedId, async (id) => {
      setVersions([]);
      setVersion("");
      if (!id) return;
      const known = selected()?.versions;
      try {
        const list = known?.length ? known : await dotnetApi.nugetVersions(id);
        const sorted = [...list].reverse();
        setVersions(sorted);
        const stable = sorted.find((v) => prerelease() || !v.includes("-")) ?? sorted[0] ?? "";
        setVersion(updateOf(id)?.latest ?? stable);
      } catch {
        setVersions([]);
      }
    }),
  );
  const projectRows = () =>
    (dotnetWorkspace()?.projects ?? []).map((p) => ({ project: p, ref: p.packages.find((r) => r.id.toLowerCase() === (selectedId() ?? "").toLowerCase()) }));
  const run = async (action: "add" | "remove", project: string, pkg: string, v?: string) => {
    setBusy(true);
    try {
      const cmd = await dotnetApi.nugetCommand(action, project, pkg, action === "remove" ? undefined : v, prerelease());
      const code = await runDotnetCommand(cmd);
      if (code === 0) {
        await refreshSolution(true);
        await refreshUpdates();
      }
    } catch (e) {
      showDotnetBalloon(`Cannot ${action === "remove" ? "remove" : "install"} ${pkg}`, errorText(e), 8000);
    } finally {
      setBusy(false);
    }
  };
  const upgradeAll = async () => {
    for (const u of updates())
      for (const p of dotnetWorkspace()?.projects ?? [])
        if (p.packages.some((r) => r.id.toLowerCase() === u.id.toLowerCase() && r.version === u.installed)) await run("add", p.path, u.id, u.latest);
  };
  return (
    <div class="dn-tw nuget-tw" data-testid="nuget-window">
      <div class="dn-toolbar nuget-toolbar">
        <div class="nuget-search">
          <Search size={13} class="muted" />
          <input
            value={query()}
            placeholder={t("langs.dotnet.searchNugetOrg")}
            spellcheck={false}
            data-testid="nuget-search"
            onInput={(e) => onInput(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && void search()}
          />
        </div>
        <label class="nuget-check">
          <input
            type="checkbox"
            checked={prerelease()}
            onChange={(e) => {
              setPrerelease(e.currentTarget.checked);
              void search();
            }}
          />
          {t("langs.dotnet.prerelease")}
        </label>
        <button class="icon-btn" title={t("common.refresh")} onClick={() => void Promise.all([search(), refreshUpdates()])}>
          <RefreshCw size={14} classList={{ "dn-spin": searching() }} />
        </button>
        <span class="dn-sep" />
        <div class="nuget-tabs" role="tablist">
          <button role="tab" classList={{ active: tab() === "packages" }} onClick={() => setTab("packages")}>
            {t("langs.dotnet.packages")}
          </button>
          <button role="tab" data-testid="nuget-updates-tab" classList={{ active: tab() === "updates" }} onClick={() => setTab("updates")}>
            {t("langs.dotnet.updates")}
            <Show when={updates().length}>
              <span class="nuget-count">{updates().length}</span>
            </Show>
          </button>
        </div>
      </div>
      <div class="dn-split">
        <div class="dn-left nuget-list" data-testid="nuget-list">
          <div class="nuget-heading muted">
            {tab() === "updates" ? t("langs.dotnet.upgrades") : results() ? t("langs.dotnet.packagesOnTheFeed2", { total: total().toLocaleString() }) : t("langs.dotnet.installedInTheSolution")}
            <Show when={tab() === "updates" && updates().length}>
              <button class="link-btn" disabled={busy()} onClick={() => void upgradeAll()}>
                {t("langs.dotnet.upgradeAll")}
              </button>
            </Show>
          </div>
          <Show when={error()}>
            <div class="dn-message dn-warning">{error()}</div>
          </Show>
          <For each={entries()} fallback={<div class="dn-message muted">{tab() === "updates" ? t("langs.dotnet.everyPackageIsUpTo") : searching() ? t("common.searchingEllipsis") : t("langs.dotnet.noPackages")}</div>}>
            {(e) => (
              <div class="nuget-item" classList={{ selected: selectedId() === e.id }} data-package={e.id} onClick={() => setSelectedId(e.id)}>
                <Package size={16} class="nuget-icon" />
                <div class="nuget-item-text">
                  <div class="nuget-item-head">
                    <span class="nuget-id">{e.id}</span>
                    <Show when={e.verified}>
                      <span title={t("langs.dotnet.theIdPrefixIsReserved")}>
                        <BadgeCheck size={12} class="nuget-verified" />
                      </span>
                    </Show>
                    <Show when={e.deprecated || e.vulnerable}>
                      <span title={e.vulnerable ? t("langs.dotnet.hasKnownVulnerabilities") : t("langs.dotnet.deprecated")}>
                        <TriangleAlert size={12} class="dn-warn-icon" />
                      </span>
                    </Show>
                    <span class="dn-spacer" />
                    <Show when={updateOf(e.id) && tab() === "packages"}>
                      <span class="nuget-upgrade" title={t("langs.dotnet.upgradeTo", { latest: updateOf(e.id)!.latest })}>
                        <ArrowUp size={11} />
                        {updateOf(e.id)!.latest}
                      </span>
                    </Show>
                    <span class="muted nuget-version">{e.version || "implicit"}</span>
                  </div>
                  <Show when={e.description || e.downloads}>
                    <div class="muted nuget-desc">
                      {e.description}
                      <Show when={e.downloads}> {t("langs.dotnet.downloads", { value: shortCount(e.downloads) })}</Show>
                    </div>
                  </Show>
                </div>
              </div>
            )}
          </For>
        </div>
        <div class="nuget-details" data-testid="nuget-details">
          <Show when={selected()} fallback={<div class="dn-message muted">{t("langs.dotnet.selectAPackageToSee")}</div>}>
            {(s) => (
              <>
                <div class="nuget-title">
                  <span class="nuget-id">{s().id}</span>
                  <Show when={s().verified}>
                    <BadgeCheck size={14} class="nuget-verified" />
                  </Show>
                  <Show when={s().projectUrl}>
                    <a class="icon-btn" title={s().projectUrl!} href={s().projectUrl!} target="_blank" rel="noreferrer noopener">
                      <ExternalLink size={13} />
                    </a>
                  </Show>
                </div>
                <Show when={s().authors}>
                  <div class="muted">by {s().authors}</div>
                </Show>
                <Show when={s().description && tab() !== "updates"}>
                  <p class="nuget-about">{s().description}</p>
                </Show>
                <div class="nuget-version-row">
                  <span class="muted">{t("langs.dotnet.version")}</span>
                  <select value={version()} data-testid="nuget-version" onChange={(e) => setVersion(e.currentTarget.value)}>
                    <For each={versions().length ? versions() : [s().version]}>{(v) => <option value={v}>{v}</option>}</For>
                  </select>
                </div>
                <table class="nuget-projects">
                  <tbody>
                    <For each={projectRows()}>
                      {(row) => (
                        <tr data-project={row.project.name}>
                          <td>{row.project.name}</td>
                          <td class="muted">{row.ref ? row.ref.version || "implicit" : ""}</td>
                          <td>
                            <div class="nuget-actions">
                            <Show
                              when={row.ref}
                              fallback={
                                <button class="pill-btn" disabled={busy() || !version()} onClick={() => void run("add", row.project.path, s().id, version())}>
                                  {t("common.install")}
                                </button>
                              }
                            >
                              <Show when={row.ref!.version !== version() && row.ref!.source !== "implicit"}>
                                <button class="pill-btn" disabled={busy()} onClick={() => void run("add", row.project.path, s().id, version())}>
                                  {updateOf(s().id)?.latest === version() ? t("langs.dotnet.upgrade") : `Use ${version()}`}
                                </button>
                              </Show>
                              <Show when={row.ref!.source !== "implicit" && row.ref!.source !== "props"}>
                                <button class="pill-btn" disabled={busy()} onClick={() => void run("remove", row.project.path, s().id)}>
                                  {t("common.remove")}
                                </button>
                              </Show>
                            </Show>
                            </div>
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </>
            )}
          </Show>
        </div>
      </div>
    </div>
  );
}
