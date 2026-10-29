/**
 * Cell outputs as Rust rendered them: console text with ANSI colors and file links,
 * sanitized HTML in a sandboxed frame (no scripts, no network), images, JSON trees,
 * Markdown and DataFrame tables.
 */
import { createResource, createSignal, For, Match, Show, Switch } from "solid-js";
import { ChevronDown, ChevronRight } from "lucide-solid";
import type { Rendered } from "@/ipc/jupyter";
import type { ConsoleLine } from "@/ipc/run";
import { markdownApi } from "@/ipc/markdown";
import { openFileLink, styleOf } from "@/features/run/Console";
import { Markdown } from "@/features/ai-chat/Markdown";
import { getSetting } from "@/state/settings";
import { DataFrameView } from "./DataFrameView";

export function TextLines(props: { lines: ConsoleLine[] }) {
  return (
    <For each={props.lines}>
      {(line) => (
        <div class="nb-out-line">
          <For each={line.segments}>
            {(p) =>
              p.link ? (
                <a class="run-link" style={styleOf(p.style)} onClick={() => void openFileLink(p.link!)}>
                  {p.text}
                </a>
              ) : (
                <span style={styleOf(p.style)}>{p.text}</span>
              )
            }
          </For>
          {"​"}
        </div>
      )}
    </For>
  );
}

/** CSS of an HTML output's frame: the theme's colors and Jupyter-like tables. */
function frameStyle(): string {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
  return `html,body{margin:0;padding:0;background:transparent;color:${v("--fg", "#222")};font:13px/1.45 system-ui,sans-serif}
body{padding:2px 0}a{color:${v("--info", "#36c")}}
table{border-collapse:collapse;font-size:12px;margin:2px 0}
th,td{padding:4px 10px;text-align:right;border:none;border-bottom:1px solid ${v("--border", "#ddd")}}
thead th{border-bottom:1px solid ${v("--border-strong", "#bbb")};font-weight:600}
tbody tr:nth-child(odd){background:${v("--bg-hover", "#f5f5f5")}}
pre,code{font-family:${v("--font-mono", "monospace")}}`;
}

const CSP = "default-src 'none'; img-src data:; style-src 'unsafe-inline'; font-src data:";

function HtmlOutput(props: { html: string }) {
  let frame!: HTMLIFrameElement;
  const [height, setHeight] = createSignal(24);
  const doc = () =>
    `<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${CSP}"><style>${frameStyle()}</style></head><body>${props.html}</body></html>`;
  // Same origin without scripts: the frame cannot run code, and Forge can measure it.
  const measure = () => {
    const h = frame.contentDocument?.documentElement.scrollHeight;
    if (h) setHeight(h + 2);
  };
  return <iframe ref={frame} class="nb-out-html" sandbox="allow-same-origin" srcdoc={doc()} style={{ height: `${height()}px` }} onLoad={measure} title="HTML output" />;
}

function JsonNode(props: { name?: string; value: unknown; depth: number }) {
  const [open, setOpen] = createSignal(props.depth < 2);
  const isObj = () => props.value !== null && typeof props.value === "object";
  const entries = () => (Array.isArray(props.value) ? props.value.map((v, i) => [String(i), v] as const) : Object.entries(props.value as object));
  const summary = () => (Array.isArray(props.value) ? `[${(props.value as unknown[]).length}]` : `{${Object.keys(props.value as object).length}}`);
  return (
    <div class="nb-json-node">
      <Show
        when={isObj()}
        fallback={
          <div class="nb-json-row">
            <Show when={props.name !== undefined}>
              <span class="nb-json-key">{props.name}: </span>
            </Show>
            <span class={`nb-json-${props.value === null ? "null" : typeof props.value}`}>{JSON.stringify(props.value)}</span>
          </div>
        }
      >
        <div class="nb-json-row nb-json-toggle" onClick={() => setOpen(!open())}>
          {open() ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
          <Show when={props.name !== undefined}>
            <span class="nb-json-key">{props.name}: </span>
          </Show>
          <span class="muted">{summary()}</span>
        </div>
        <Show when={open()}>
          <div class="nb-json-children">
            <For each={entries()}>{([k, v]) => <JsonNode name={k} value={v} depth={props.depth + 1} />}</For>
          </div>
        </Show>
      </Show>
    </div>
  );
}

function MarkdownOutput(props: { text: string }) {
  const [blocks] = createResource(() => props.text, (t) => markdownApi.parse(t, "preview"));
  return (
    <div class="nb-md">
      <Markdown blocks={blocks.latest ?? []} />
    </div>
  );
}

export function OutputView(props: { r: Rendered }) {
  return (
    <Switch>
      <Match when={props.r.kind === "text" && props.r}>
        {(r) => (
          <div class="nb-out-text" classList={{ error: r().error }}>
            <TextLines lines={r().lines} />
          </div>
        )}
      </Match>
      <Match when={props.r.kind === "html" && props.r}>{(r) => <HtmlOutput html={r().html} />}</Match>
      <Match when={props.r.kind === "image" && props.r}>
        {(r) => <img class="nb-out-image" src={r().src} alt={r().mime} width={r().width} height={r().height} />}
      </Match>
      <Match when={props.r.kind === "json" && props.r}>
        {(r) => (
          <div class="nb-out-json">
            <JsonNode value={r().value} depth={0} />
          </div>
        )}
      </Match>
      <Match when={props.r.kind === "markdown" && props.r}>{(r) => <MarkdownOutput text={r().text} />}</Match>
      <Match when={props.r.kind === "dataFrame" && props.r}>{(r) => <DataFrameView frame={r().frame} />}</Match>
    </Switch>
  );
}

export function Outputs(props: { rendered: Rendered[] }) {
  const limit = () => getSetting<boolean>("jupyter.outputs.limitHeight", true) !== false;
  return (
    <Show when={props.rendered.length}>
      <div class="nb-outputs" classList={{ limited: limit() }} data-testid="cell-outputs">
        <For each={props.rendered}>{(r) => <OutputView r={r} />}</For>
      </div>
    </Show>
  );
}
