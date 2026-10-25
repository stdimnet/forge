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
        fallback={<div class="placeholder-panel">{loading() ? "Indexing WordPress hooks…" : "No WordPress project detected."}</div>}
      >
        {(info) => (
          <>
            <div class="panel-toolbar wp-toolbar">
              <label class="wp-search">
                <Search size={13} />
                <input
                  type="text"
                  placeholder="Filter hooks by name or file"
                  value={filter()}
                  onInput={(e) => setFilter(e.currentTarget.value)}
                  spellcheck={false}
                />
              </label>
              <div class="wp-kinds" role="radiogroup" aria-label="Hook kind">
                <For each={["all", "action", "filter"] as KindFilter[]}>
                  {(k) => (
                    <button role="radio" aria-checked={kind() === k} classList={{ "wp-kind": true, active: kind() === k }} onClick={() => setKind(k)}>
                      {k === "all" ? "All" : k === "action" ? "Actions" : "Filters"}
                    </button>
                  )}
                </For>
              </div>
              <span class="muted wp-count">{shown().length} hooks</span>
              <span class="wp-flex" />
              <span class="wp-chip" title={info().wpCli ? `WP-CLI: ${info().wpCli!.path}` : "WP-CLI was not found in vendor/bin or on PATH"}>
                WP-CLI: {info().wpCli ? (info().wpCli!.source === "project" ? info().wpCli!.path : "wp (PATH)") : "not found"}
              </span>
              <Show
                when={info().stubs}
                fallback={
                  <button class="link-btn wp-chip" title="composer require --dev php-stubs/wordpress-stubs" onClick={() => void installStubs()}>
                    <Package size={12} /> Install stubs…
                  </button>
                }
              >
                <span class="wp-chip" title={info().stubs}>
                  <Package size={12} /> Stubs installed
                </span>
              </Show>
            </div>
            <div class="wp-table-wrap">
              <table class="wp-table">
                <thead>
                  <tr>
                    <th style={{ width: "38%" }}>Hook</th>
                    <th style={{ width: "9%" }}>Kind</th>
                    <th style={{ width: "10%" }}>Callbacks</th>
                    <th style={{ width: "9%" }}>Fired</th>
                    <th>Fired in</th>
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
                          {h.invocations ? `${h.location.path}:${h.location.line + 1}` : h.summary ? "WordPress core" : "—"}
                        </td>
                      </tr>
                    )}
                  </For>
                </tbody>
              </table>
            </div>
            <div class="wp-footer muted">
              {describe(info())} · {info().hooks} hook references in {info().files} PHP files{info().core ? " (WordPress core included)" : ""}
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
      <button class="icon-btn" title="WP-CLI…" onClick={() => runWpCli()}>
        <Play size={14} />
      </button>
      <button class="icon-btn" classList={{ spinning: loading() }} title="Refresh" onClick={() => void loadWordPress(true)}>
        <RefreshCw size={14} />
      </button>
    </>
  );
}
