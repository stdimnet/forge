/**
 * Endpoints tool window, like IntelliJ IDEA Ultimate's: the HTTP endpoints of the project
 * (Spring MVC / WebFlux request mappings with class-level prefixes, JAX-RS resources under
 * their application path) with method, path and handler, plus the mappings only a running
 * application reports (actuator, library controllers). Click opens the handler; the row
 * menu and the toolbar open a request in the HTTP Client.
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { Copy, FileCode, Globe, RefreshCw, Search, Send, X } from "lucide-solid";
import type { EndpointRow } from "@/ipc/spring";
import { openMenu } from "@/features/run/PopupMenu";
import { endpointUrl, loadSpring, loading, openInHttpClient, overview, reveal } from "./store";
import { t } from "@/i18n";

const [query, setQuery] = createSignal("");
const [selected, setSelected] = createSignal(-1);
const [framework, setFramework] = createSignal<"all" | "spring" | "jaxrs" | "runtime">("all");

export { setQuery as setEndpointsQuery };

const copyText = (text: string) => navigator.clipboard?.writeText(text).catch(() => undefined);

const FRAMEWORKS: Record<string, string> = { spring: "Spring MVC", jaxrs: "JAX-RS", runtime: "Runtime" };

export function matchesEndpoint(e: EndpointRow, q: string): boolean {
  if (!q) return true;
  const hay = [e.method, e.path, e.handler, FRAMEWORKS[e.framework] ?? e.framework, ...(e.produces ?? [])].join(" ").toLowerCase();
  return q
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w));
}

function Path(props: { path: string }) {
  const parts = () => props.path.split(/(\{[^}]+\})/);
  return (
    <span class="sp-path">
      <For each={parts()}>{(p) => (p.startsWith("{") ? <span class="sp-path-param">{p}</span> : p)}</For>
    </span>
  );
}

function Handler(props: { handler: string }) {
  const dot = () => props.handler.lastIndexOf(".");
  return (
    <Show when={dot() > 0 && !props.handler.includes(" ")} fallback={<span class="sp-handler-text">{props.handler}</span>}>
      <span class="sp-handler" title={props.handler}>
        <span class="sp-handler-class">{props.handler.slice(0, dot())}</span>
        <span class="sp-handler-dot">.</span>
        <span class="sp-handler-method">{props.handler.slice(dot() + 1)}()</span>
      </span>
    </Show>
  );
}

function rowMenu(e: EndpointRow, ev: MouseEvent) {
  ev.preventDefault();
  openMenu({
    x: ev.clientX,
    y: ev.clientY,
    items: [
      { label: t("langs.spring.openInHttpClient"), icon: Send, iconColor: "var(--success)", run: () => void openInHttpClient(e) },
      { label: t("langs.spring.goToHandler"), icon: FileCode, disabled: !e.location, run: () => reveal(e.location) },
      { separator: true },
      { label: t("langs.spring.copyUrl"), icon: Copy, run: () => void copyText(endpointUrl(e)) },
      { label: t("langs.spring.copyPath"), run: () => void copyText(e.path) },
    ],
  });
}

export function EndpointsView() {
  const rows = () => overview()?.endpoints ?? [];
  const visible = createMemo(() => rows().filter((e) => (framework() === "all" || e.framework === framework()) && matchesEndpoint(e, query())));
  const present = createMemo(() => new Set(rows().map((e) => e.framework)));
  onMount(() => {
    if (!overview() && !loading()) void loadSpring();
  });
  const onKeyDown = (e: KeyboardEvent) => {
    const n = visible().length;
    if (!n) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      setSelected((i) => Math.max(0, Math.min(n - 1, i + (e.key === "ArrowDown" ? 1 : -1))));
      document.querySelector(`.sp-endpoints tbody tr:nth-child(${selected() + 1})`)?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter" && selected() >= 0) {
      e.preventDefault();
      reveal(visible()[selected()].location);
    }
  };
  return (
    <div class="sp-view" onKeyDown={onKeyDown} data-testid="endpoints">
      <div class="sp-toolbar">
        <div class="sp-search">
          <Search size={13} />
          <input
            class="sp-search-input"
            placeholder={t("langs.spring.searchEndpointsByMethodPath")}
            value={query()}
            onInput={(e) => {
              setQuery(e.currentTarget.value);
              setSelected(-1);
            }}
            spellcheck={false}
          />
          <Show when={query()}>
            <button class="icon-btn sp-clear" title={t("common.clear")} onClick={() => setQuery("")}>
              <X size={12} />
            </button>
          </Show>
        </div>
        <div class="sp-segmented" role="tablist">
          <For each={["all", "spring", "jaxrs", "runtime"] as const}>
            {(f) => (
              <Show when={f === "all" || present().has(f)}>
                <button class="sp-seg" classList={{ active: framework() === f }} onClick={() => setFramework(f)}>
                  {f === "all" ? t("common.all") : FRAMEWORKS[f]}
                </button>
              </Show>
            )}
          </For>
        </div>
        <span class="sp-summary muted">
          <Show when={loading() && !rows().length} fallback={`${visible().length} of ${rows().length} endpoints`}>
            {t("langs.spring.scanningSourcesEllipsis")}
          </Show>
        </span>
        <Show when={overview()?.runtime}>
          {(r) => (
            <span class="sp-live" title={t("langs.spring.requestMappingsOfTheRunning", { name: r().name })}>
              <span class="sp-live-dot" /> {r().name} {r().url ? `· ${r().url}` : ""}
            </span>
          )}
        </Show>
      </div>
      <div class="sp-scroll" tabIndex={0}>
        <table class="sp-table sp-endpoints">
          <thead>
            <tr>
              <th class="col-method">{t("common.method")}</th>
              <th class="col-path">{t("common.path")}</th>
              <th class="col-handler">{t("langs.spring.handler")}</th>
              <th class="col-media">{t("langs.spring.produces")}</th>
              <th class="col-fw">{t("common.framework")}</th>
              <th class="col-actions" />
            </tr>
          </thead>
          <tbody>
            <For each={visible()}>
              {(e, i) => (
                <tr
                  classList={{ selected: selected() === i(), runtime: e.framework === "runtime" }}
                  data-endpoint={`${e.method} ${e.path}`}
                  onClick={() => {
                    setSelected(i());
                    reveal(e.location);
                  }}
                  onContextMenu={(ev) => {
                    setSelected(i());
                    rowMenu(e, ev);
                  }}
                >
                  <td class="col-method">
                    <span class={`sp-verb verb-${e.method.toLowerCase()}`}>{e.method}</span>
                  </td>
                  <td class="col-path">
                    <Show when={e.live}>
                      <span class="sp-live-dot" title={t("langs.spring.servedByTheRunningApplication")} />
                    </Show>
                    <Path path={e.path} />
                  </td>
                  <td class="col-handler">
                    <Handler handler={e.handler} />
                  </td>
                  <td class="col-media">{(e.produces ?? []).join(", ")}</td>
                  <td class="col-fw">
                    <span class={`sp-badge fw-${e.framework}`}>{FRAMEWORKS[e.framework] ?? e.framework}</span>
                  </td>
                  <td class="col-actions">
                    <button
                      class="icon-btn sp-row-btn"
                      title={t("langs.spring.openInHttpClient")}
                      data-testid="open-in-http-client"
                      onClick={(ev) => {
                        ev.stopPropagation();
                        void openInHttpClient(e);
                      }}
                    >
                      <Send size={13} />
                    </button>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
        <Show when={!loading() && !visible().length}>
          <div class="sp-empty">
            <Globe size={22} />
            <div>{rows().length ? t("common.noEndpointsMatchTheSearch") : t("langs.spring.noEndpointsFoundRestcontrollerController")}</div>
          </div>
        </Show>
      </div>
    </div>
  );
}

export function EndpointsActions() {
  const current = () => {
    const i = selected();
    const list = (overview()?.endpoints ?? []).filter((e) => (framework() === "all" || e.framework === framework()) && matchesEndpoint(e, query()));
    return i >= 0 ? list[i] : undefined;
  };
  return (
    <>
      <button class="icon-btn" title={t("langs.spring.openInHttpClient")} disabled={!current()} onClick={() => current() && void openInHttpClient(current()!)}>
        <Send size={14} />
      </button>
      <button class="icon-btn" title={t("langs.spring.refreshEndpoints")} onClick={() => void loadSpring(true)} disabled={loading()}>
        <RefreshCw size={14} class={loading() ? "sp-spin" : undefined} />
      </button>
    </>
  );
}
