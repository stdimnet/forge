/**
 * The Rails view of the Endpoints tool window (RubyMine's Endpoints / Rails routes): every
 * route as `bin/rails routes` prints it — from config/routes.rb, or from the command in
 * trusted projects — with speed search. Clicking a row opens the controller action; the
 * context menu goes to the route, generates an HTTP request or opens the page. A second tab
 * lists the models with their table, columns and associations.
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { Database, RefreshCw, Route, Search, Send, X } from "lucide-solid";
import type { RailsModel, RailsRoute } from "@/ipc/rails";
import { item, separator, showContextMenu } from "@/features/contextMenu/state";
import {
  handlerOf,
  loadRailsProject,
  openInHttpClient,
  openRouteInBrowser,
  railsError,
  railsLoading,
  railsProject,
  reveal,
  serverBase,
  sortedMethods,
} from "./state";

const [query, setQuery] = createSignal("");
const [tab, setTab] = createSignal<"routes" | "models">("routes");
const [selected, setSelected] = createSignal(-1);

export { setTab as setRailsEndpointsTab, setQuery as setRailsEndpointsQuery };

function matches(text: string, q: string): boolean {
  const hay = text.toLowerCase();
  return q
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w));
}

const routeText = (r: RailsRoute) => [sortedMethods(r).join(" "), r.path, r.name ?? "", handlerOf(r)].join(" ");

function PathText(props: { path: string }) {
  const parts = () => props.path.split(/(:[A-Za-z_]\w*|\*[A-Za-z_]\w*|\([^)]*\))/);
  return (
    <span class="routes-uri">
      <For each={parts()}>{(p) => (/^[:*(]/.test(p) ? <span class="routes-param">{p}</span> : p)}</For>
    </span>
  );
}

function Handler(props: { route: RailsRoute }) {
  const r = () => props.route;
  return (
    <span class="routes-action" title={handlerOf(r())}>
      <Show when={r().kind === "route"} fallback={<span class="rails-handler-other">{handlerOf(r())}</span>}>
        <span class="rails-controller">{r().controller}</span>
        <span class="muted">#</span>
        <span class="routes-method">{r().action}</span>
      </Show>
    </span>
  );
}

function openRoute(r: RailsRoute) {
  void reveal(r.actionLocation ?? r.location);
}

function routeMenu(r: RailsRoute, e: MouseEvent) {
  e.preventDefault();
  const http = r.kind !== "mount" || r.methods.length > 0;
  showContextMenu(
    [
      item("Jump to Controller Action", () => openRoute(r), undefined, { disabled: !r.actionLocation }),
      item("Go to Route Declaration", () => void reveal(r.location)),
      separator,
      item("Generate HTTP Request", () => void openInHttpClient(r), undefined, { disabled: !http }),
      item("Open in Browser", () => openRouteInBrowser(r), undefined, { disabled: !sortedMethods(r).some((m) => m === "GET" || m === "ANY") }),
      separator,
      item("Copy URL", () => void navigator.clipboard?.writeText(`${serverBase()}${r.samplePath}`)),
      item("Copy Path", () => void navigator.clipboard?.writeText(r.path)),
      item("Copy Helper", () => void navigator.clipboard?.writeText(`${r.name}_path`), undefined, { disabled: !r.name }),
    ],
    e.clientX,
    e.clientY,
  );
}

function RoutesTable() {
  const routes = () => railsProject()?.routes ?? [];
  const visible = createMemo(() => routes().filter((r) => matches(routeText(r), query())));
  const onKeyDown = (e: KeyboardEvent) => {
    const n = visible().length;
    if (!n) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      setSelected((i) => Math.max(0, Math.min(n - 1, i + (e.key === "ArrowDown" ? 1 : -1))));
    } else if (e.key === "Enter" && selected() >= 0) {
      e.preventDefault();
      openRoute(visible()[selected()]);
    }
  };
  const source = () => (railsProject()?.routesSource === "rails routes" ? "bin/rails routes" : "config/routes.rb");
  return (
    <>
      <div class="routes-toolbar">
        <div class="routes-search">
          <Search size={13} />
          <input
            class="routes-search-input"
            placeholder="Search routes by path, method, helper or controller"
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
          <Show when={railsLoading() && !routes().length} fallback={`${visible().length} of ${routes().length} routes`}>
            Loading routes…
          </Show>
        </span>
        <span class="routes-source muted" title="Where the routes come from (Settings | Languages & Frameworks | Rails)">
          {source()}
        </span>
      </div>
      <Show when={railsProject()?.routesError}>
        <div class="routes-error">{railsProject()!.routesError}</div>
      </Show>
      <div class="routes-scroll" tabIndex={0} onKeyDown={onKeyDown}>
        <Show when={visible().length} fallback={<div class="routes-empty">{routes().length ? "No routes match the search" : "No routes found in config/routes.rb"}</div>}>
          <table class="routes-table rails-table">
            <thead>
              <tr>
                <th class="col-method">Verb</th>
                <th class="col-uri">URI Pattern</th>
                <th class="col-name">Prefix</th>
                <th class="col-action">Controller#Action</th>
                <th class="col-open" />
              </tr>
            </thead>
            <tbody>
              <For each={visible()}>
                {(r, i) => (
                  <tr
                    classList={{ selected: selected() === i() }}
                    data-route={`${sortedMethods(r).join(",")} ${r.path}`}
                    onClick={() => {
                      setSelected(i());
                      openRoute(r);
                    }}
                    onContextMenu={(e) => {
                      setSelected(i());
                      routeMenu(r, e);
                    }}
                  >
                    <td class="col-method">
                      <For each={sortedMethods(r)}>{(m) => <span class={`routes-verb verb-${m.toLowerCase()}`}>{m}</span>}</For>
                    </td>
                    <td class="col-uri">
                      <PathText path={r.path} />
                    </td>
                    <td class="col-name" title={r.name ? `${r.name}_path, ${r.name}_url` : undefined}>
                      {r.name ?? ""}
                    </td>
                    <td class="col-action">
                      <Handler route={r} />
                    </td>
                    <td class="col-open">
                      <button
                        class="icon-btn routes-open"
                        title="Generate HTTP Request"
                        onClick={(e) => {
                          e.stopPropagation();
                          void openInHttpClient(r);
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
        </Show>
      </div>
    </>
  );
}

function ModelsTable() {
  const models = () => (railsProject()?.models ?? []).filter((m) => matches(`${m.name} ${m.table} ${m.columns.map((c) => c.name).join(" ")}`, query()));
  const open = (m: RailsModel) => void reveal(m.location);
  return (
    <>
      <div class="routes-toolbar">
        <div class="routes-search">
          <Search size={13} />
          <input class="routes-search-input" placeholder="Search models, tables and columns" value={query()} onInput={(e) => setQuery(e.currentTarget.value)} spellcheck={false} />
        </div>
        <span class="routes-summary muted">{`${models().length} models · ${railsProject()?.tables.length ?? 0} tables`}</span>
      </div>
      <div class="routes-scroll" tabIndex={0}>
        <Show when={models().length} fallback={<div class="routes-empty">No ActiveRecord models found in app/models</div>}>
          <table class="routes-table rails-table">
            <thead>
              <tr>
                <th class="rails-col-model">Model</th>
                <th class="rails-col-table">Table</th>
                <th>Columns</th>
                <th class="rails-col-assoc">Associations</th>
              </tr>
            </thead>
            <tbody>
              <For each={models()}>
                {(m) => (
                  <tr data-model={m.name} onClick={() => open(m)}>
                    <td class="rails-col-model">
                      <span class="routes-method">{m.name}</span>
                    </td>
                    <td class="rails-col-table">
                      <Database size={12} class="rails-table-icon" /> {m.table}
                    </td>
                    <td class="rails-columns">
                      <For each={m.columns}>
                        {(c) => (
                          <span class="rails-column" title={`${c.type}${c.notNull ? ", not null" : ""}`}>
                            {c.name}
                            <span class="muted">:{c.type}</span>
                          </span>
                        )}
                      </For>
                    </td>
                    <td class="rails-col-assoc muted">{m.associations.map((a) => `${a.kind} :${a.name}`).join(", ")}</td>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </Show>
      </div>
    </>
  );
}

export function RailsEndpointsView() {
  onMount(() => {
    if (!railsProject() && !railsLoading()) void loadRailsProject();
  });
  return (
    <div class="routes-view rails-view">
      <div class="rails-tabs" role="tablist">
        <button classList={{ active: tab() === "routes" }} onClick={() => setTab("routes")} data-tab="rails-routes">
          <Route size={12} /> Routes
        </button>
        <button classList={{ active: tab() === "models" }} onClick={() => setTab("models")} data-tab="rails-models">
          <Database size={12} /> Models
        </button>
      </div>
      <Show when={railsError()}>
        <div class="routes-error">{railsError()}</div>
      </Show>
      <Show when={tab() === "models"} fallback={<RoutesTable />}>
        <ModelsTable />
      </Show>
    </div>
  );
}

export function RailsEndpointsActions() {
  return (
    <button class="icon-btn" title="Refresh" onClick={() => void loadRailsProject(true)}>
      <RefreshCw size={14} classList={{ spin: railsLoading() }} />
    </button>
  );
}
