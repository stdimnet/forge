/**
 * The preview of an OpenAPI document, laid out like Swagger UI but rendered natively from
 * the model Rust builds (`openapi_preview`): title, version and servers, operations grouped
 * by tag with their parameters, request body and responses (examples generated from the
 * schemas), and the schemas. No remote JavaScript is loaded. "Try it" opens the operation
 * as a request in the HTTP client.
 */
import { createEffect, createResource, createSignal, For, on, onCleanup, Show } from "solid-js";
import { ChevronDown, ChevronRight, Send } from "lucide-solid";
import { openApiApi, type OpenApiOperation } from "@/ipc/webTooling";
import type { EditorTab } from "@/state/editor";
import { openFileAt } from "@/editor/views";
import { generateHttpRequest, markOpenApi } from "./store";
import { t as tr } from "@/i18n";

const METHOD_CLASS = (m: string) => `oa-method oa-${m.toLowerCase()}`;

function Operation(props: { path: string; op: OpenApiOperation }) {
  const [open, setOpen] = createSignal(false);
  const op = () => props.op;
  return (
    <div class="oa-op" classList={{ "oa-deprecated": op().deprecated }} data-op={`${op().method} ${op().path}`}>
      <div class="oa-op-head" onClick={() => setOpen(!open())}>
        {open() ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
        <span class={METHOD_CLASS(op().method)}>{op().method}</span>
        <span class="oa-path">{op().path}</span>
        <span class="oa-summary">{op().summary ?? op().operationId ?? ""}</span>
        <button
          class="icon-btn oa-try"
          title={tr("common.generateHttpRequest")}
          onClick={(e) => {
            e.stopPropagation();
            void generateHttpRequest(props.path, op().method, op().path);
          }}
        >
          <Send size={13} />
        </button>
      </div>
      <Show when={open()}>
        <div class="oa-op-body">
          <Show when={op().description}>
            <p class="oa-desc">{op().description}</p>
          </Show>
          <Show when={op().parameters.length}>
            <div class="oa-section">{tr("langs.webTooling.parameters")}</div>
            <table class="oa-table">
              <tbody>
                <For each={op().parameters}>
                  {(p) => (
                    <tr>
                      <td class="oa-name">
                        {p.name}
                        {p.required ? <span class="oa-required">*</span> : null}
                      </td>
                      <td class="muted">{p.location}</td>
                      <td class="oa-type">{p.typeText}</td>
                      <td>{p.description ?? ""}</td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          </Show>
          <Show when={op().requestBody}>
            {(b) => (
              <>
                <div class="oa-section">
                  {tr("langs.webTooling.requestBody")} <span class="muted">{b().contentType}</span> <span class="oa-type">{b().typeText}</span>
                </div>
                <pre class="oa-example">{b().example}</pre>
              </>
            )}
          </Show>
          <div class="oa-section">{tr("langs.webTooling.responses")}</div>
          <For each={op().responses}>
            {(r) => (
              <div class="oa-response">
                <span class="oa-status" classList={{ "oa-ok": r.status.startsWith("2"), "oa-err": /^[45]/.test(r.status) }}>
                  {r.status}
                </span>
                <span>{r.description}</span>
                <For each={r.bodies}>
                  {(b) => (
                    <div class="oa-response-body">
                      <span class="muted">{b.contentType}</span> <span class="oa-type">{b.typeText}</span>
                      <Show when={b.example}>
                        <pre class="oa-example">{b.example}</pre>
                      </Show>
                    </div>
                  )}
                </For>
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

export function OpenApiPreview(props: { tab: EditorTab }) {
  // Re-render a moment after typing stops.
  const [content, setContent] = createSignal(props.tab.content);
  createEffect(
    on(
      () => props.tab.content,
      (c) => {
        const t = setTimeout(() => setContent(c), 300);
        onCleanup(() => clearTimeout(t));
      },
    ),
  );
  const [preview] = createResource(content, async (c) => {
    const p = await openApiApi.preview(props.tab.path, c).catch(() => null);
    markOpenApi(props.tab.path, !!p);
    return p;
  });
  return (
    <div class="oa-preview" data-testid="openapi-preview">
      <Show when={preview()} fallback={<div class="set-list-empty">{preview.loading ? tr("common.loadingEllipsis") : tr("langs.webTooling.notAnOpenapiDocument")}</div>}>
        {(p) => (
          <>
            <div class="oa-info">
              <h2>
                {p().title} <span class="oa-version">{p().version}</span> <span class="oa-spec">{p().spec}</span>
              </h2>
              <Show when={p().description}>
                <p class="oa-desc">{p().description}</p>
              </Show>
              <Show when={p().servers.length}>
                <div class="oa-servers">
                  {tr("langs.webTooling.serversLabel")}
                  <For each={p().servers}>{(s) => <code title={s.description}>{s.url}</code>}</For>
                </div>
              </Show>
            </div>
            <For each={p().groups}>
              {(g) => (
                <div class="oa-group">
                  <div class="oa-tag">
                    <b>{g.name}</b> <span class="muted">{g.description ?? ""}</span>
                  </div>
                  <For each={g.operations}>{(op) => <Operation path={props.tab.path} op={op} />}</For>
                </div>
              )}
            </For>
            <Show when={p().schemas.length}>
              <div class="oa-group">
                <div class="oa-tag">
                  <b>{tr("langs.webTooling.schemas")}</b>
                </div>
                <For each={p().schemas}>
                  {(s) => (
                    <div class="oa-schema">
                      <div class="oa-schema-head" onClick={() => void openFileAt(props.tab.path, s.line + 1)}>
                        <b>{s.name}</b> <span class="oa-type">{s.typeText}</span>
                      </div>
                      <For each={s.properties}>
                        {(prop) => (
                          <div class="oa-prop">
                            <span class="oa-name">
                              {prop.name}
                              {prop.required ? <span class="oa-required">*</span> : null}
                            </span>
                            <span class="oa-type">{prop.typeText}</span>
                            <span class="muted">{prop.description ?? ""}</span>
                          </div>
                        )}
                      </For>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </>
        )}
      </Show>
    </div>
  );
}
