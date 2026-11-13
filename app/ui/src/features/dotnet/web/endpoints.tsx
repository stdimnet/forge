/**
 * ASP.NET Core in the shared Endpoints tool window, like Rider's: minimal API routes (with
 * route groups), controller actions, Blazor pages and Razor Pages, parsed in Rust
 * (`dotnet_endpoints`). Click opens the handler; Generate HTTP Request opens an `.http`
 * request against the project's launch profile URL in the HTTP Client.
 */
import { createMemo, createResource, createSignal, For, Show, createRoot } from "solid-js";
import { Copy, FileCode, Globe, Lock, RefreshCw, Search, Send, X } from "lucide-solid";
import { dotnetWebApi, type Endpoint } from "@/ipc/dotnetWeb";
import { openMenu } from "@/features/run/PopupMenu";
import { openHttpRequest } from "@/features/scratches/store";
import { openFileAt } from "@/editor/views";
import { dotnetActive, dotnetWorkspace, errorText, showDotnetBalloon } from "../state";
import { t } from "@/i18n";
import { writeClipboard } from "@/features/clipboard/system";

/** ASP.NET Core is used: a web project in the solution. */
export const aspnetActive = () => dotnetActive() && (dotnetWorkspace()?.projects ?? []).some((p) => p.web);

const [model, { refetch }] = createRoot(() => createResource(
  () => (aspnetActive() ? dotnetWorkspace() : null),
  () => dotnetWebApi.endpoints(),
));
export const refreshEndpoints = () => void refetch();

const [query, setQuery] = createSignal("");
const [kind, setKind] = createSignal("all");
const [selected, setSelected] = createSignal(-1);

export const FRAMEWORKS: Record<string, string> = { minimal: "Minimal API", controller: "Controller", blazor: "Blazor", razorPages: "Razor Pages" };

export function matchesEndpoint(e: Endpoint, q: string): boolean {
  if (!q) return true;
  const hay = [e.method, e.path, e.handler, e.name ?? "", FRAMEWORKS[e.framework] ?? e.framework].join(" ").toLowerCase();
  return q
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((w) => hay.includes(w));
}

const visible = createRoot(() => createMemo(() => (model()?.endpoints ?? []).filter((e) => (kind() === "all" || e.framework === kind()) && matchesEndpoint(e, query()))));

export async function generateHttpRequest(e: Endpoint): Promise<void> {
  try {
    await openHttpRequest(await dotnetWebApi.httpRequest(e));
  } catch (err) {
    showDotnetBalloon("Cannot generate the HTTP request", errorText(err), 6000);
  }
}

const reveal = (e: Endpoint) => void openFileAt(e.file, e.line + 1);
const urlOf = (e: Endpoint) => `${(e.project && model()?.baseUrls[e.project]) || "http://localhost:5000"}${e.path}`;
const copy = (text: string) => writeClipboard(text);

function rowMenu(e: Endpoint, ev: MouseEvent) {
  ev.preventDefault();
  openMenu({
    x: ev.clientX,
    y: ev.clientY,
    items: [
      { label: t("common.generateHttpRequest"), icon: Send, iconColor: "var(--success)", run: () => void generateHttpRequest(e) },
      { label: t("langs.dotnet.goToDeclaration"), icon: FileCode, run: () => reveal(e) },
      { separator: true },
      { label: t("langs.dotnet.copyUrl"), icon: Copy, run: () => void copy(urlOf(e)) },
      { label: t("langs.dotnet.copyRoute"), run: () => void copy(e.path) },
    ],
  });
}

function Route(props: { path: string }) {
  const parts = () => props.path.split(/(\{[^}]+\})/);
  return (
    <span class="dn-ep-path">
      <For each={parts()}>{(p) => (p.startsWith("{") ? <span class="dn-ep-param">{p}</span> : p)}</For>
    </span>
  );
}

export function AspNetEndpointsView() {
  const present = createMemo(() => new Set((model()?.endpoints ?? []).map((e) => e.framework)));
  const onKeyDown = (ev: KeyboardEvent) => {
    const n = visible().length;
    if (!n) return;
    if (ev.key === "ArrowDown" || ev.key === "ArrowUp") {
      ev.preventDefault();
      setSelected((i) => Math.max(0, Math.min(n - 1, i + (ev.key === "ArrowDown" ? 1 : -1))));
    } else if (ev.key === "Enter" && selected() >= 0) {
      ev.preventDefault();
      reveal(visible()[selected()]!);
    }
  };
  return (
    <div class="dn-ep" data-testid="aspnet-endpoints" onKeyDown={onKeyDown}>
      <div class="dn-ep-toolbar">
        <div class="dn-ep-search">
          <Search size={13} />
          <input placeholder={t("langs.dotnet.searchEndpointsByMethodRoute")} value={query()} spellcheck={false} onInput={(e) => (setQuery(e.currentTarget.value), setSelected(-1))} />
          <Show when={query()}>
            <button class="icon-btn" title={t("common.clear")} onClick={() => setQuery("")}>
              <X size={12} />
            </button>
          </Show>
        </div>
        <div class="dn-ep-segmented" role="tablist">
          <For each={["all", "minimal", "controller", "blazor", "razorPages"]}>
            {(f) => (
              <Show when={f === "all" || present().has(f)}>
                <button classList={{ active: kind() === f }} onClick={() => setKind(f)}>
                  {f === "all" ? t("common.all") : FRAMEWORKS[f]}
                </button>
              </Show>
            )}
          </For>
        </div>
        <span class="muted dn-ep-summary">{model.loading && !model() ? t("langs.dotnet.scanningSourcesEllipsis") : t("langs.dotnet.ofEndpoints", { count: visible().length, value: model()?.endpoints.length ?? 0 })}</span>
      </div>
      <div class="dn-ep-scroll" tabIndex={0}>
        <table class="dn-ep-table">
          <thead>
            <tr>
              <th class="dn-ep-method">{t("common.method")}</th>
              <th>{t("langs.dotnet.route")}</th>
              <th>{t("langs.dotnet.handler")}</th>
              <th class="dn-ep-kind">{t("langs.dotnet.kind")}</th>
              <th class="dn-ep-actions" />
            </tr>
          </thead>
          <tbody>
            <For each={visible()}>
              {(e, i) => (
                <tr
                  classList={{ selected: selected() === i() }}
                  data-endpoint={`${e.method} ${e.path}`}
                  onClick={() => (setSelected(i()), reveal(e))}
                  onContextMenu={(ev) => (setSelected(i()), rowMenu(e, ev))}
                >
                  <td class="dn-ep-method">
                    <span class={`dn-verb verb-${e.method.toLowerCase()}`}>{e.method}</span>
                  </td>
                  <td>
                    <Route path={e.path} />
                    <Show when={e.authorized}>
                      <span title={t("langs.dotnet.requiresAuthorization")}>
                        <Lock size={11} class="dn-ep-lock" />
                      </span>
                    </Show>
                    <Show when={e.name}>
                      <span class="muted dn-ep-name">{e.name}</span>
                    </Show>
                  </td>
                  <td class="dn-ep-handler" title={`${e.file}:${e.line + 1}`}>
                    {e.handler === "lambda" ? <span class="muted">{`λ ${e.file.split("/").pop()}:${e.line + 1}`}</span> : e.handler}
                  </td>
                  <td class="dn-ep-kind">
                    <span class={`dn-ep-badge fw-${e.framework}`}>{FRAMEWORKS[e.framework] ?? e.framework}</span>
                  </td>
                  <td class="dn-ep-actions">
                    <Show when={e.framework === "minimal" || e.framework === "controller"}>
                      <button
                        class="icon-btn"
                        title={t("common.generateHttpRequest")}
                        data-testid="generate-http-request"
                        onClick={(ev) => {
                          ev.stopPropagation();
                          void generateHttpRequest(e);
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
        <Show when={!model.loading && !visible().length}>
          <div class="dn-ep-empty">
            <Globe size={22} />
            <div>{model()?.endpoints.length ? t("common.noEndpointsMatchTheSearch") : t("langs.dotnet.noEndpointsFoundMapgetMappost")}</div>
          </div>
        </Show>
      </div>
    </div>
  );
}

export function AspNetEndpointsActions() {
  const current = () => (selected() >= 0 ? visible()[selected()] : undefined);
  return (
    <>
      <button class="icon-btn" title={t("common.generateHttpRequest")} disabled={!current()} onClick={() => current() && void generateHttpRequest(current()!)}>
        <Send size={14} />
      </button>
      <button class="icon-btn" title={t("langs.dotnet.refreshEndpoints")} onClick={() => void refetch()} disabled={model.loading}>
        <RefreshCw size={14} class={model.loading ? "dn-spin" : undefined} />
      </button>
    </>
  );
}
