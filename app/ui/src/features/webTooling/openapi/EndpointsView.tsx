/**
 * The "OpenAPI" view of the shared Endpoints tool window: every operation of the project's
 * OpenAPI / Swagger documents, grouped by file, with speed search. Clicking a row opens the
 * operation in the file; the context menu generates an HTTP request or opens the preview.
 */
import { createMemo, createSignal, For, onMount, Show } from "solid-js";
import { FileCode, RefreshCw, Search, Send } from "lucide-solid";
import { item, showContextMenu } from "@/features/contextMenu/state";
import { openFileAt } from "@/editor/views";
import { setPreviewLayout } from "@/editor/TextEditorWithPreview";
import { generateHttpRequest, loadOpenApi, openApiFiles, openApiLoading } from "./store";
import { t } from "@/i18n";

const [query, setQuery] = createSignal("");

export function OpenApiEndpointsView() {
  onMount(() => {
    if (!openApiFiles().length) void loadOpenApi();
  });
  const groups = createMemo(() => {
    const q = query().toLowerCase().trim();
    return openApiFiles()
      .map((f) => ({
        file: f,
        endpoints: f.endpoints.filter((e) => !q || `${e.method} ${e.path} ${e.summary ?? ""} ${e.operationId ?? ""}`.toLowerCase().includes(q)),
      }))
      .filter((g) => g.endpoints.length);
  });
  return (
    <div class="wt-endpoints" data-testid="openapi-endpoints">
      <div class="wt-search">
        <Search size={13} class="muted" />
        <input placeholder={t("langs.webTooling.searchEndpoints")} value={query()} onInput={(e) => setQuery(e.currentTarget.value)} />
      </div>
      <Show when={groups().length} fallback={<div class="set-list-empty">{openApiLoading() ? t("langs.webTooling.scanningEllipsis") : t("langs.webTooling.noOpenapiDocumentsInThe")}</div>}>
        <For each={groups()}>
          {(g) => (
            <div class="wt-group">
              <div class="wt-group-head">
                <FileCode size={13} class="muted" /> {g.file.title} <span class="muted">{g.file.path}</span>
                <span class="wt-badge">{g.file.spec}</span>
              </div>
              <For each={g.endpoints}>
                {(e) => (
                  <div
                    class="wt-row"
                    classList={{ "oa-deprecated": e.deprecated }}
                    onClick={() => void openFileAt(g.file.path, e.line + 1)}
                    onContextMenu={(ev) => {
                      ev.preventDefault();
                      showContextMenu(
                        [
                          item("Jump to Source", () => void openFileAt(g.file.path, e.line + 1)),
                          item("Generate HTTP Request", () => void generateHttpRequest(g.file.path, e.method, e.path)),
                          item("Open Preview", () => {
                            setPreviewLayout(g.file.path, "split");
                            void openFileAt(g.file.path, e.line + 1);
                          }),
                        ],
                        ev.clientX,
                        ev.clientY,
                      );
                    }}
                  >
                    <span class={`oa-method oa-${e.method.toLowerCase()}`}>{e.method}</span>
                    <span class="oa-path">{e.path}</span>
                    <span class="muted wt-grow">{e.summary ?? e.operationId ?? ""}</span>
                    <button
                      class="icon-btn"
                      title={t("common.generateHttpRequest")}
                      onClick={(ev) => {
                        ev.stopPropagation();
                        void generateHttpRequest(g.file.path, e.method, e.path);
                      }}
                    >
                      <Send size={12} />
                    </button>
                  </div>
                )}
              </For>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
}

export function OpenApiEndpointsActions() {
  return (
    <button class="icon-btn" title={t("common.refresh")} aria-label={t("common.refresh")} onClick={() => void loadOpenApi()}>
      <RefreshCw size={14} />
    </button>
  );
}
