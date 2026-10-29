/**
 * Endpoints tool window (PyCharm Professional's Endpoints): every HTTP endpoint of the
 * project's Django URL patterns, Flask routes and FastAPI path operations, with speed
 * search and a framework filter. Clicking a row opens the view; the context menu goes to
 * the URL pattern or decorator, opens the request in the HTTP client or the page in the
 * browser. A second tab lists the Celery tasks.
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { Braces, ExternalLink, FileCode, RefreshCw, Search, Send, X } from "lucide-solid";
import type { CeleryTask, WebRoute } from "@/ipc/pythonWeb";
import { item, separator, showContextMenu } from "@/features/contextMenu/state";
import {
  fastapiActive,
  loadWebProject,
  openFastApiDocs,
  openInHttpClient,
  openRouteInBrowser,
  reveal,
  serverBase,
  sortedMethods,
  webError,
  webLoading,
  webProject,
} from "./state";

const [query, setQuery] = createSignal("");
const [framework, setFramework] = createSignal<"all" | WebRoute["framework"]>("all");
const [tab, setTab] = createSignal<"endpoints" | "tasks">("endpoints");
const [selected, setSelected] = createSignal(-1);

export { setTab as setEndpointsTab, setQuery as setEndpointsQuery };

const FRAMEWORK_LABELS: Record<WebRoute["framework"], string> = { django: "Django", flask: "Flask", fastapi: "FastAPI" };

function matches(text: string, q: string): boolean {
  const hay = text.toLowerCase();
  return q
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w));
}

const routeText = (r: WebRoute) => [sortedMethods(r).join(" "), r.path, r.name ?? "", r.handler, r.framework].join(" ");

function PathText(props: { path: string }) {
  const parts = () => props.path.split(/(<[^>]+>|\{[^}]+\}|\(\?P<[^>]+>[^)]*\))/);
  return (
    <span class="routes-uri">
      <For each={parts()}>{(p) => (/^[<{(]/.test(p) ? <span class="routes-param">{p}</span> : p)}</For>
    </span>
  );
}

function Handler(props: { handler: string }) {
  const parts = () => {
    const i = props.handler.lastIndexOf(".");
    return i < 0 ? ["", props.handler] : [props.handler.slice(0, i + 1), props.handler.slice(i + 1)];
  };
  return (
    <span class="routes-action" title={props.handler}>
      <span class="pyweb-module">{parts()[0]}</span>
      <span class="routes-method">{parts()[1]}</span>
    </span>
  );
}

function openRoute(r: WebRoute) {
  void reveal(r.handlerLocation ?? r.location);
}

function routeMenu(r: WebRoute, e: MouseEvent) {
  e.preventDefault();
  const http = !r.methods.includes("WS");
  showContextMenu(
    [
      item("Jump to Source", () => openRoute(r)),
      item(r.framework === "django" ? "Go to URL Pattern" : "Go to Route Declaration", () => void reveal(r.location)),
      separator,
      item("Open in HTTP Client", () => void openInHttpClient(r), undefined, { disabled: !http }),
      item("Open in Browser", () => openRouteInBrowser(r), undefined, { disabled: !http || !sortedMethods(r).some((m) => m === "GET" || m === "ANY") }),
      separator,
      item("Copy URL", () => void navigator.clipboard?.writeText(`${serverBase(r.framework)}${r.samplePath}`)),
      item("Copy Path", () => void navigator.clipboard?.writeText(r.path)),
    ],
    e.clientX,
    e.clientY,
  );
}

function EndpointsTable() {
  const routes = () => webProject()?.routes ?? [];
  const frameworks = createMemo(() => [...new Set(routes().map((r) => r.framework))]);
  const visible = createMemo(() =>
    routes().filter((r) => (framework() === "all" || r.framework === framework()) && matches(routeText(r), query())),
  );
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
  return (
    <>
      <div class="routes-toolbar">
        <div class="routes-search">
          <Search size={13} />
          <input
            class="routes-search-input"
            placeholder="Search endpoints by path, method, name or view"
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
        <Show when={frameworks().length > 1}>
          <div class="pyweb-segmented" role="tablist">
            <button classList={{ active: framework() === "all" }} onClick={() => setFramework("all")}>
              All
            </button>
            <For each={frameworks()}>
              {(f) => (
                <button classList={{ active: framework() === f }} onClick={() => setFramework(f)}>
                  {FRAMEWORK_LABELS[f]}
                </button>
              )}
            </For>
          </div>
        </Show>
        <span class="routes-summary muted">
          <Show when={webLoading() && !routes().length} fallback={`${visible().length} of ${routes().length} endpoints`}>
            Loading endpoints…
          </Show>
        </span>
        <Show when={fastapiActive() && routes().some((r) => r.framework === "fastapi")}>
          <button class="link-btn pyweb-docs-link" title={`${serverBase("fastapi")}/docs`} onClick={openFastApiDocs}>
            <ExternalLink size={12} /> Open /docs
          </button>
        </Show>
      </div>
      <div class="routes-scroll" tabIndex={0} onKeyDown={onKeyDown}>
        <Show
          when={visible().length}
          fallback={<div class="routes-empty">{routes().length ? "No endpoints match the search" : "No endpoints found in urls.py, @app.route or @app.get"}</div>}
        >
          <table class="routes-table pyweb-table">
            <thead>
              <tr>
                <th class="col-method">Method</th>
                <th class="col-uri">Path</th>
                <th class="col-name">Name</th>
                <th class="col-action">View</th>
                <th class="pyweb-col-framework">Framework</th>
                <th class="col-open" />
              </tr>
            </thead>
            <tbody>
              <For each={visible()}>
                {(r, i) => (
                  <tr
                    classList={{ selected: selected() === i() }}
                    data-endpoint={`${sortedMethods(r).join(",")} ${r.path}`}
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
                    <td class="col-name" title={r.name}>
                      {r.name ?? ""}
                    </td>
                    <td class="col-action">
                      <Handler handler={r.handler} />
                    </td>
                    <td class="pyweb-col-framework muted">{FRAMEWORK_LABELS[r.framework]}</td>
                    <td class="col-open">
                      <Show when={!r.methods.includes("WS")}>
                        <button
                          class="icon-btn routes-open"
                          title="Open in HTTP Client"
                          onClick={(e) => {
                            e.stopPropagation();
                            void openInHttpClient(r);
                          }}
                        >
                          <Send size={13} />
                        </button>
                      </Show>
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

function TasksTable() {
  const tasks = () => (webProject()?.tasks ?? []).filter((t) => matches(`${t.name} ${t.function} ${t.decorator}`, query()));
  const open = (t: CeleryTask) => void reveal(t.location);
  return (
    <>
      <div class="routes-toolbar">
        <div class="routes-search">
          <Search size={13} />
          <input class="routes-search-input" placeholder="Search Celery tasks" value={query()} onInput={(e) => setQuery(e.currentTarget.value)} spellcheck={false} />
        </div>
        <span class="routes-summary muted">{`${tasks().length} tasks`}</span>
        <Show when={webProject()?.celery?.app}>
          <span class="routes-source muted">celery -A {webProject()!.celery!.app}</span>
        </Show>
      </div>
      <div class="routes-scroll" tabIndex={0}>
        <Show when={tasks().length} fallback={<div class="routes-empty">No @shared_task or @app.task functions found</div>}>
          <table class="routes-table pyweb-table">
            <thead>
              <tr>
                <th class="pyweb-col-task">Task</th>
                <th class="col-action">Function</th>
                <th class="pyweb-col-decorator">Decorator</th>
                <th>File</th>
              </tr>
            </thead>
            <tbody>
              <For each={tasks()}>
                {(t) => (
                  <tr data-task={t.name} onClick={() => open(t)}>
                    <td class="pyweb-col-task">
                      <span class="pyweb-task-name">{t.name}</span>
                    </td>
                    <td class="col-action">
                      <span class="routes-method">{t.function}</span>
                      <Show when={t.bound}>
                        <span class="muted"> (bound)</span>
                      </Show>
                    </td>
                    <td class="pyweb-col-decorator">
                      <code>@{t.decorator}</code>
                    </td>
                    <td class="muted">
                      {t.location.path}:{t.location.line + 1}
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

export function EndpointsView() {
  onMount(() => {
    if (!webProject() && !webLoading()) void loadWebProject();
  });
  const hasTasks = () => (webProject()?.tasks.length ?? 0) > 0 || webProject()?.frameworks.includes("celery");
  return (
    <div class="routes-view pyweb-view">
      <Show when={hasTasks()}>
        <div class="pyweb-tabs" role="tablist">
          <button classList={{ active: tab() === "endpoints" }} onClick={() => setTab("endpoints")}>
            <Braces size={12} /> HTTP Endpoints
          </button>
          <button classList={{ active: tab() === "tasks" }} onClick={() => setTab("tasks")} data-tab="celery-tasks">
            <FileCode size={12} /> Celery Tasks
          </button>
        </div>
      </Show>
      <Show when={webError()}>
        <div class="routes-error">{webError()}</div>
      </Show>
      <Show when={tab() === "tasks" && hasTasks()} fallback={<EndpointsTable />}>
        <TasksTable />
      </Show>
    </div>
  );
}

export function EndpointsActions() {
  return (
    <button class="icon-btn" title="Refresh" onClick={() => void loadWebProject(true)}>
      <RefreshCw size={14} classList={{ spin: webLoading() }} />
    </button>
  );
}
