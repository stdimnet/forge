/**
 * WordPress tool window: the project's hooks (actions and filters) with their callback and
 * invocation counts, filterable; double-click opens where a hook fires. The header shows
 * what was detected (site, plugin or theme), WP-CLI and the stubs.
 */
import { createMemo, createSignal, For, Show } from "solid-js";
import { Anchor, Package, Play, RefreshCw, Search, Zap } from "lucide-solid";
import type { HookSummary } from "@/ipc/wordpress";
import { revealSymbol } from "@/features/php/editorViews";
import { describe, hooks, installStubs, loading, loadWordPress, wordpressInfo } from "./store";
import { runWpCli } from "./wpcli";
import { t } from "@/i18n";

type KindFilter = "all" | "action" | "filter";

function matches(filter: string, h: HookSummary): boolean {
  const f = filter.trim().toLowerCase();
  return !f || h.name.toLowerCase().includes(f) || h.location.path.toLowerCase().includes(f);
}

export function HooksView() {
  const [filter, setFilter] = createSignal("");
  const [kind, setKind] = createSignal<KindFilter>("all");
  const [selected, setSelected] = createSignal<string | null>(null);
  const shown = createMemo(() => hooks().filter((h) => (kind() === "all" || h.kind === kind()) && matches(filter(), h)));
  const key = (h: HookSummary) => `${h.kind}:${h.name}`;
  return (
    <div class="wp-panel">
      <Show
        when={wordpressInfo()}
        fallback={<div class="placeholder-panel">{loading() ? t("langs.wordpress.indexingWordpressHooksEllipsis") : t("langs.wordpress.noWordpressProjectDetected")}</div>}
      >
        {(info) => (
          <>
            <div class="panel-toolbar wp-toolbar">
              <label class="wp-search">
                <Search size={13} />
                <input
                  type="text"
                  placeholder={t("langs.wordpress.filterHooksByNameOr")}
                  value={filter()}
                  onInput={(e) => setFilter(e.currentTarget.value)}
                  spellcheck={false}
                />
              </label>
              <div class="wp-kinds" role="radiogroup" aria-label={t("langs.wordpress.hookKind")}>
                <For each={["all", "action", "filter"] as KindFilter[]}>
                  {(k) => (
                    <button role="radio" aria-checked={kind() === k} classList={{ "wp-kind": true, active: kind() === k }} onClick={() => setKind(k)}>
                      {k === "all" ? t("common.all") : k === "action" ? t("langs.wordpress.actions") : t("langs.wordpress.filters")}
                    </button>
                  )}
                </For>
              </div>
              <span class="muted wp-count">{shown().length} hooks</span>
              <span class="wp-flex" />
              <span class="wp-chip" title={info().wpCli ? `WP-CLI: ${info().wpCli!.path}` : t("langs.wordpress.wpCliWasNotFound")}>
                {t("langs.wordpress.wpCli", { value: info().wpCli ? (info().wpCli!.source === "project" ? info().wpCli!.path : t("langs.wordpress.wpPath")) : t("common.notFound") })}
              </span>
              <Show
                when={info().stubs}
                fallback={
                  <button class="link-btn wp-chip" title={t("langs.wordpress.composerRequireDevPhpStubs")} onClick={() => void installStubs()}>
                    <Package size={12} /> {t("langs.wordpress.installStubsEllipsis")}
                  </button>
                }
              >
                <span class="wp-chip" title={info().stubs}>
                  <Package size={12} /> {t("langs.wordpress.stubsInstalled")}
                </span>
              </Show>
            </div>
            <div class="wp-table-wrap">
              <table class="wp-table">
                <thead>
                  <tr>
                    <th style={{ width: "38%" }}>{t("langs.wordpress.hook")}</th>
                    <th style={{ width: "9%" }}>{t("langs.wordpress.kind")}</th>
                    <th style={{ width: "10%" }}>{t("langs.wordpress.callbacks")}</th>
                    <th style={{ width: "9%" }}>{t("langs.wordpress.fired")}</th>
                    <th>{t("langs.wordpress.firedIn")}</th>
                  </tr>
                </thead>
                <tbody>
                  <For each={shown()}>
                    {(h) => (
                      <tr
                        classList={{ selected: selected() === key(h) }}
                        data-hook={h.name}
                        onClick={() => setSelected(key(h))}
                        onDblClick={() => void revealSymbol(h.location.path, { line: h.location.line })}
                        title={h.summary ?? ""}
                      >
                        <td class="wp-name">
                          {h.kind === "action" ? <Zap size={13} class="wp-icon-action" /> : <Anchor size={13} class="wp-icon-filter" />}
                          <span classList={{ "wp-dynamic": !!h.dynamic }}>{h.name}</span>
                        </td>
                        <td class="muted">{h.kind}</td>
                        <td>{h.registrations || ""}</td>
                        <td>{h.invocations || ""}</td>
                        <td class="muted wp-ellipsis">
                          {h.invocations ? `${h.location.path}:${h.location.line + 1}` : h.summary ? t("langs.wordpress.wordpressCore") : "—"}
                        </td>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </div>
            <div class="wp-footer muted">
              {t("langs.wordpress.hookReferencesInPhpFiles", { value: describe(info()), hooks: info().hooks, files: info().files, value2: info().core ? t("langs.wordpress.wordpressCoreIncluded") : "" })}
            </div>
          </>
        )}
      </Show>
    </div>
  );
}

export function HooksActions() {
  return (
    <>
      <button class="icon-btn" title={t("langs.wordpress.wpCliEllipsis")} onClick={() => runWpCli()}>
        <Play size={14} />
      </button>
      <button class="icon-btn" classList={{ spinning: loading() }} title={t("common.refresh")} onClick={() => void loadWordPress(true)}>
        <RefreshCw size={14} />
      </button>
    </>
  );
}
