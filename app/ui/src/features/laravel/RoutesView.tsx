/**
 * Routes tool window: every route of the application as a table (Method · URI · Name ·
 * Action · Middleware) with speed search. Clicking a row opens the controller method (or
 * the route definition for closures); the arrow button opens the definition.
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { FileCode, RefreshCw, Search, X } from "lucide-solid";
import type { LaravelRoute } from "@/ipc/laravel";
import { loadRoutes, reveal, routes, routesError, routesLoading, routesSource, shortAction } from "./state";

const [query, setQuery] = createSignal("");
const [selected, setSelected] = createSignal<number>(-1);

export { setQuery as setRoutesQuery };

const METHOD_ORDER = ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];

/** Methods to show: HEAD is implied by GET, `any` collapses to ANY. */
export function displayMethods(methods: string[]): string[] {
  const m = methods.filter((x) => x !== "HEAD");
  if (METHOD_ORDER.every((x) => m.includes(x))) return ["ANY"];
  return m.sort((a, b) => METHOD_ORDER.indexOf(a) - METHOD_ORDER.indexOf(b));
}

export function matchesRoute(r: LaravelRoute, q: string): boolean {
  if (!q) return true;
  const hay = [r.methods.join(" "), `/${r.uri}`, r.name ?? "", r.action, r.middleware.join(" ")].join(" ").toLowerCase();
  return q
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w));
}

function Uri(props: { uri: string }) {
  const parts = () => (props.uri === "/" ? ["/"] : `/${props.uri.replace(/^\//, "")}`.split(/(\{[^}]+\})/));
  return (
    <span class="routes-uri">
      <For each={parts()}>{(p) => (p.startsWith("{") ? <span class="routes-param">{p}</span> : p)}</For>
    </span>
  );
}

function Action(props: { route: LaravelRoute }) {
  const short = () => shortAction(props.route.action);
  return (
    <Show when={props.route.controller} fallback={<span class="routes-closure">{props.route.action}</span>}>
      <span class="routes-action" title={props.route.action}>
        <span class="routes-class">{short().split("@")[0]}</span>
        <span class="routes-at">@</span>
        <span class="routes-method">{short().split("@")[1]}</span>
      </span>
    </Show>
  );
}

function open(r: LaravelRoute) {
  const target = r.actionLocation ?? r.source;
  if (target) void reveal(target);
}

export function RoutesView() {
  const visible = createMemo(() => routes().filter((r) => matchesRoute(r, query())));
  onMount(() => {
    if (!routes().length && !routesLoading()) void loadRoutes();
  });
  const onKeyDown = (e: KeyboardEvent) => {
    const n = visible().length;
    if (!n) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const d = e.key === "ArrowDown" ? 1 : -1;
      setSelected((i) => Math.max(0, Math.min(n - 1, i + d)));
      document.querySelector(`.routes-table tbody tr:nth-child(${selected() + 1})`)?.scrollIntoView({ block: "nearest" });
    } else if (e.key === "Enter" && selected() >= 0) {
      e.preventDefault();
      open(visible()[selected()]);
    }
  };
  return (
    <div class="routes-view" onKeyDown={onKeyDown}>
      <div class="routes-toolbar">
        <div class="routes-search">
          <Search size={13} />
          <input
            class="routes-search-input"
            placeholder="Search routes by URI, name, action or middleware"
            value={query()}
            onInput={(e) => {
              setQuery(e.currentTarget.value);
              setSelected(-1);
            }}
            spellcheck={false}
          />
          <Show when={query()}>
            <button class="icon-btn routes-clear" title="Clear" onClick={() => setQuery("")}>
              <X size={12} />
            </button>
          </Show>
        </div>
        <span class="routes-summary muted">
          <Show when={routesLoading()} fallback={`${visible().length} of ${routes().length} routes`}>
            Loading routes…
          </Show>
        </span>
        <span class="routes-source muted" title="Settings | Languages & Frameworks | PHP | Laravel">
          {routesSource() === "artisan" ? "from artisan route:list" : routesSource() === "static" ? "from routes/*.php" : ""}
        </span>
      </div>
      <Show when={routesError()}>
        <div class="routes-error">{routesError()}</div>
      </Show>
      <div class="routes-scroll" tabIndex={0}>
        <table class="routes-table">
          <thead>
            <tr>
              <th class="col-method">Method</th>
              <th class="col-uri">URI</th>
              <th class="col-name">Name</th>
              <th class="col-action">Action</th>
              <th class="col-middleware">Middleware</th>
              <th class="col-open" />
            </tr>
          </thead>
          <tbody>
            <For each={visible()}>
              {(r, i) => (
                <tr
                  classList={{ selected: selected() === i() }}
                  data-route={r.name ?? r.uri}
                  onClick={() => {
                    setSelected(i());
                    open(r);
                  }}
                >
                  <td class="col-method">
                    <For each={displayMethods(r.methods)}>{(m) => <span class={`routes-verb verb-${m.toLowerCase()}`}>{m}</span>}</For>
                  </td>
                  <td class="col-uri">
                    <Uri uri={r.uri} />
                  </td>
                  <td class="col-name">{r.name ?? ""}</td>
                  <td class="col-action">
                    <Action route={r} />
                  </td>
                  <td class="col-middleware" title={r.middleware.join(", ")}>
                    {r.middleware.join(", ")}
                  </td>
                  <td class="col-open">
                    <Show when={r.source}>
                      {(s) => (
                        <button
                          class="icon-btn routes-open"
                          title={`Go to route definition (${s().path}:${s().line + 1})`}
                          onClick={(e) => {
                            e.stopPropagation();
                            void reveal(s());
                          }}
                        >
                          <FileCode size={13} />
                        </button>
                      )}
                    </Show>
                  </td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
        <Show when={!routesLoading() && !visible().length}>
          <div class="routes-empty">{routes().length ? "No routes match the search" : "No routes found"}</div>
        </Show>
      </div>
    </div>
  );
}

export function RoutesActions() {
  return (
    <button class="icon-btn" title="Refresh Routes" onClick={() => void loadRoutes()} disabled={routesLoading()}>
      <RefreshCw size={14} class={routesLoading() ? "laravel-spin" : undefined} />
    </button>
  );
}
