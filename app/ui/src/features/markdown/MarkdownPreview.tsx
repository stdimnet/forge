/**
 * The Markdown preview: renders the safe AST computed in Rust (`markdown_document`) with
 * Solid components, so file content never becomes HTML. Updates live while typing, follows
 * the editor's scroll position (and the editor follows the preview), opens relative links
 * in Forge and external ones in the browser, shows project images (read through the
 * backend, so remote projects work) and renders Mermaid diagrams.
 */
import { createEffect, createMemo, createResource, createSignal, For, Match, on, onCleanup, onMount, Show, Switch } from "solid-js";
import { Dynamic } from "solid-js/web";
import type { EditorView } from "@codemirror/view";
import { Check, Info, Lightbulb, MessageSquareWarning, OctagonAlert, TriangleAlert } from "lucide-solid";
import { markdownApi, type MdBlock, type MdDocument, type MdInline, type MdListItem } from "@/ipc/markdown";
import { viewersApi } from "@/ipc/viewers";
import { editor, openFile, type EditorTab } from "@/state/editor";
import { openFileAt } from "@/editor/views";
import { GUEST_PREFIX } from "@/features/code-with-me/docs";
import { getSetting } from "@/state/settings";
import { theme } from "@/state/theme";
import { highlightFence } from "./highlight";
import { imageSource, lineAnchor, linkTarget } from "./links";
import { renderMermaid } from "./mermaid";
import { previewToEditor, editorToPreview } from "./syncScroll";
import { t as tr } from "@/i18n";

/** What the components below need from the preview they are in. */
interface Ctx {
  path: string;
  /** Data URL (or remote URL) of an image source. */
  image: (src: string) => Promise<string | null>;
  open: (href: string) => void;
}

function Inlines(props: { items: MdInline[]; ctx: Ctx }) {
  return (
    <For each={props.items}>
      {(i) => (
        <Switch>
          <Match when={i.t === "text" && i}>{(x) => x().text}</Match>
          <Match when={(i.t === "url" || i.t === "fqcn") && i}>{(x) => x().text}</Match>
          <Match when={i.t === "code" && i}>{(x) => <code class="mdp-code">{x().text}</code>}</Match>
          <Match when={i.t === "strong" && i}>
            {(x) => (
              <strong>
                <Inlines items={x().children} ctx={props.ctx} />
              </strong>
            )}
          </Match>
          <Match when={i.t === "em" && i}>
            {(x) => (
              <em>
                <Inlines items={x().children} ctx={props.ctx} />
              </em>
            )}
          </Match>
          <Match when={i.t === "del" && i}>
            {(x) => (
              <del>
                <Inlines items={x().children} ctx={props.ctx} />
              </del>
            )}
          </Match>
          <Match when={i.t === "link" && i}>
            {(x) => (
              <a
                class="mdp-link"
                href="#"
                title={x().href}
                onClick={(e) => {
                  e.preventDefault();
                  props.ctx.open(x().href);
                }}
              >
                <Inlines items={x().children} ctx={props.ctx} />
              </a>
            )}
          </Match>
          <Match when={i.t === "image" && i}>{(x) => <Image image={x()} ctx={props.ctx} />}</Match>
          <Match when={i.t === "footnoteRef" && i}>
            {(x) => (
              <sup class="mdp-fnref">
                <a
                  href="#"
                  onClick={(e) => {
                    e.preventDefault();
                    props.ctx.open(`#fn-${x().label}`);
                  }}
                >
                  {x().index}
                </a>
              </sup>
            )}
          </Match>
          <Match when={i.t === "break"}>
            <br />
          </Match>
        </Switch>
      )}
    </For>
  );
}

function Image(props: { image: Extract<MdInline, { t: "image" }>; ctx: Ctx }) {
  const [url] = createResource(
    () => props.image.src,
    (src) => props.ctx.image(src),
  );
  const size = (v?: string) => (v && /^\d+$/.test(v) ? `${v}px` : v);
  return (
    <Show when={url()} fallback={<span class="mdp-img-missing" title={props.image.src}>{props.image.alt || props.image.src}</span>}>
      {(src) => (
        <img
          class="mdp-img"
          src={src()}
          alt={props.image.alt}
          title={props.image.title ?? ""}
          style={{ width: size(props.image.width), height: size(props.image.height) }}
          draggable={false}
        />
      )}
    </Show>
  );
}

function CodeFence(props: { lang: string; text: string }) {
  const lines = createMemo(() => highlightFence(props.text, props.lang));
  return (
    <pre class="mdp-pre" data-lang={props.lang || undefined}>
      <For each={lines()}>
        {(line) => (
          <div class="mdp-code-line">
            <For each={line}>{(s) => (s.cls ? <span class={s.cls}>{s.text}</span> : s.text)}</For>
            {line.length === 0 ? "​" : ""}
          </div>
        )}
      </For>
    </pre>
  );
}

function Mermaid(props: { code: string }) {
  const [state, setState] = createSignal<{ url?: string; error?: string }>({});
  createEffect(() => {
    const code = props.code;
    const dark = !theme().endsWith("light");
    renderMermaid(code, dark).then(
      (url) => props.code === code && setState({ url }),
      (e) => props.code === code && setState({ error: e instanceof Error ? e.message : String(e) }),
    );
  });
  return (
    <div class="mdp-mermaid">
      <Switch fallback={<span class="mdp-muted">{tr("tools.markdown.renderingDiagramEllipsis")}</span>}>
        <Match when={state().url}>{(url) => <img src={url()} alt={tr("tools.markdown.mermaidDiagram")} draggable={false} />}</Match>
        <Match when={state().error}>
          {(err) => (
            <div class="mdp-mermaid-error">
              <div class="mdp-mermaid-error-title">{tr("tools.markdown.mermaid", { value: err().split("\n")[0] })}</div>
              <CodeFence lang="" text={props.code} />
            </div>
          )}
        </Match>
      </Switch>
    </div>
  );
}

const ALERTS: Record<string, { title: string; icon: typeof Info }> = {
  note: { title: tr("tools.markdown.note"), icon: Info },
  tip: { title: tr("tools.markdown.tip"), icon: Lightbulb },
  important: { title: tr("tools.markdown.important"), icon: MessageSquareWarning },
  warning: { title: tr("tools.markdown.warning"), icon: TriangleAlert },
  caution: { title: tr("tools.markdown.caution"), icon: OctagonAlert },
};

function ListItem(props: { item: MdListItem; ctx: Ctx }) {
  return (
    <li classList={{ "mdp-task": props.item.checked !== undefined }}>
      <Show when={props.item.checked !== undefined}>
        <span class="mdp-check" classList={{ done: props.item.checked }} role="checkbox" aria-checked={props.item.checked}>
          <Show when={props.item.checked}>
            <Check size={10} stroke-width={3} />
          </Show>
        </span>
      </Show>
      <Blocks blocks={props.item.blocks} ctx={props.ctx} />
    </li>
  );
}

function Block(props: { block: MdBlock; ctx: Ctx }) {
  const b = props.block;
  switch (b.t) {
    case "heading": {
      const level = Math.min(Math.max(b.level, 1), 6);
      return (
        <Dynamic component={`h${level}`} class="mdp-h" data-anchor={b.id}>
          <Inlines items={b.inline} ctx={props.ctx} />
        </Dynamic>
      );
    }
    case "paragraph":
      return (
        <p>
          <Inlines items={b.inline} ctx={props.ctx} />
        </p>
      );
    case "plain":
      return (
        <span class="mdp-plain">
          <Inlines items={b.inline} ctx={props.ctx} />
        </span>
      );
    case "quote": {
      const alert = b.kind ? ALERTS[b.kind] : undefined;
      return (
        <blockquote class="mdp-quote" classList={{ [`mdp-alert mdp-alert-${b.kind}`]: !!alert }}>
          <Show when={alert}>
            {(a) => {
              const Icon = a().icon;
              return (
                <div class="mdp-alert-title">
                  <Icon size={14} />
                  {a().title}
                </div>
              );
            }}
          </Show>
          <Blocks blocks={b.blocks} ctx={props.ctx} />
        </blockquote>
      );
    }
    case "hr":
      return <hr class="mdp-hr" />;
    case "code":
      return (
        <Show when={b.lang === "mermaid" && getSetting<boolean>("markdown.mermaid", true) !== false} fallback={<CodeFence lang={b.lang} text={b.text} />}>
          <Mermaid code={b.text} />
        </Show>
      );
    case "list":
      return b.ordered ? (
        <ol class="mdp-list" start={b.start}>
          <For each={b.items}>{(item) => <ListItem item={item} ctx={props.ctx} />}</For>
        </ol>
      ) : (
        <ul class="mdp-list" classList={{ "mdp-tasks": b.items.some((i) => i.checked !== undefined) }}>
          <For each={b.items}>{(item) => <ListItem item={item} ctx={props.ctx} />}</For>
        </ul>
      );
    case "table": {
      const align = (i: number) => (b.align[i] || undefined) as "left" | "center" | "right" | undefined;
      return (
        <div class="mdp-table-wrap">
          <table class="mdp-table">
            <thead>
              <tr>
                <For each={b.head}>
                  {(cell, i) => (
                    <th style={{ "text-align": align(i()) }}>
                      <Inlines items={cell} ctx={props.ctx} />
                    </th>
                  )}
                </For>
              </tr>
            </thead>
            <tbody>
              <For each={b.rows}>
                {(row) => (
                  <tr>
                    <For each={row}>
                      {(cell, i) => (
                        <td style={{ "text-align": align(i()) }}>
                          <Inlines items={cell} ctx={props.ctx} />
                        </td>
                      )}
                    </For>
                  </tr>
                )}
              </For>
            </tbody>
          </table>
        </div>
      );
    }
  }
}

function Blocks(props: { blocks: MdBlock[]; ctx: Ctx }) {
  return <For each={props.blocks}>{(b) => <Block block={b} ctx={props.ctx} />}</For>;
}

/** Scroll `container` so the element with `data-anchor` (or footnote id) is at the top. */
function scrollToAnchor(container: HTMLElement, anchor: string): boolean {
  const id = anchor.toLowerCase();
  const el = [...container.querySelectorAll<HTMLElement>("[data-anchor]")].find((e) => e.dataset.anchor === id);
  if (!el) return false;
  container.scrollTop = el.getBoundingClientRect().top - container.getBoundingClientRect().top + container.scrollTop - 8;
  return true;
}

/** Open a project file from a preview link: headings of Markdown files and `#L12` lines. */
async function openLinkedFile(path: string, anchor?: string): Promise<void> {
  const line = lineAnchor(anchor);
  if (line) {
    await openFileAt(path, line);
    return;
  }
  if (anchor && /\.(md|markdown)$/i.test(path)) {
    await openFile(path);
    const text = editor.tabs.find((t) => t.path === path)?.content ?? "";
    const target = await markdownApi.anchorLine(text, anchor).catch(() => null);
    if (target !== null && target !== undefined) await openFileAt(path, target + 1);
    return;
  }
  await openFile(path);
}

export function MarkdownPreview(props: { tab: EditorTab; editorView: () => EditorView | undefined }) {
  const [doc, setDoc] = createSignal<MdDocument | null>(null);
  let container!: HTMLDivElement;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let seq = 0;

  // Live update while typing (debounced; the last answer wins).
  createEffect(
    on(
      () => props.tab.content,
      (text) => {
        clearTimeout(timer);
        const run = () => {
          const mine = ++seq;
          markdownApi.document(text).then(
            (d) => mine === seq && setDoc(d),
            (e) => console.warn("markdown preview failed", e),
          );
        };
        if (doc() === null) run();
        else timer = setTimeout(run, 120);
      },
    ),
  );
  onCleanup(() => clearTimeout(timer));

  // A Code With Me guest reads the host's file as `cwm://<path>`: relative links and images
  // are resolved in the host's project. Its images cannot be read from here.
  const guest = props.tab.path.startsWith(GUEST_PREFIX);
  const docPath = guest ? props.tab.path.slice(GUEST_PREFIX.length) : props.tab.path;
  const images = new Map<string, Promise<string | null>>();
  const ctx: Ctx = {
    path: props.tab.path,
    image: (src) => {
      const source = imageSource(docPath, src);
      if (source.kind === "url") return Promise.resolve(source.url);
      if (source.kind === "none" || guest) return Promise.resolve(null);
      let p = images.get(source.path);
      if (!p) {
        p = viewersApi.imageRead(source.path).then(
          (img) => `data:${img.info.mime};base64,${img.base64}`,
          () => null,
        );
        images.set(source.path, p);
      }
      return p;
    },
    open: (href) => {
      const target = linkTarget(docPath, href);
      switch (target.kind) {
        case "anchor":
          scrollToAnchor(container, target.anchor);
          break;
        case "external":
          window.open(target.url, "_blank", "noopener");
          break;
        case "file":
          void openLinkedFile(guest ? GUEST_PREFIX + target.path : target.path, target.anchor);
          break;
      }
    },
  };

  // ---- Synchronized scrolling (Settings | Languages & Frameworks | Markdown).
  const syncOn = () => getSetting<boolean>("markdown.syncScroll", true) !== false;
  /** The side that scrolled last moves the other one; its echo is ignored. */
  let driver: "editor" | "preview" | null = null;
  let release: ReturnType<typeof setTimeout> | undefined;
  const drive = (side: "editor" | "preview", fn: () => void) => {
    if (driver && driver !== side) return;
    driver = side;
    fn();
    clearTimeout(release);
    release = setTimeout(() => (driver = null), 80);
  };
  const followEditor = () => {
    const view = props.editorView();
    if (view && syncOn() && doc()) drive("editor", () => editorToPreview(view, container, doc()!.lines.length));
  };
  const followPreview = () => {
    const view = props.editorView();
    if (view && syncOn() && doc()) drive("preview", () => previewToEditor(view, container));
  };

  createEffect(() => {
    const view = props.editorView();
    if (!view) return;
    const onScroll = () => followEditor();
    view.scrollDOM.addEventListener("scroll", onScroll, { passive: true });
    onCleanup(() => view.scrollDOM.removeEventListener("scroll", onScroll));
  });
  // After a re-render (typing), keep the preview at the editor's position.
  createEffect(on(doc, () => requestAnimationFrame(() => (driver === "preview" ? undefined : followEditor())), { defer: true }));
  onMount(() => requestAnimationFrame(followEditor));

  const frontMatter = () => (getSetting<string>("markdown.frontMatter", "table") === "table" ? doc()?.frontMatter : undefined);

  return (
    <div class="mdp-scroll" ref={container} onScroll={() => (driver === "editor" ? undefined : followPreview())}>
      <div class="mdp">
        <Show when={frontMatter()}>
          {(fm) => (
            <table class="mdp-table mdp-front-matter" data-start={0} data-end={fm().endLine}>
              <tbody>
                <For each={fm().entries}>
                  {(e) => (
                    <tr>
                      <th>{e.key}</th>
                      <td>{e.value}</td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          )}
        </Show>
        <For each={doc()?.blocks ?? []}>
          {(b, i) => (
            <div class="mdp-block" data-start={doc()?.lines[i()]?.start} data-end={doc()?.lines[i()]?.end}>
              <Block block={b} ctx={ctx} />
            </div>
          )}
        </For>
        <Show when={doc()?.footnotes?.length}>
          <section class="mdp-footnotes">
            <hr class="mdp-hr" />
            <ol>
              <For each={doc()!.footnotes}>
                {(f) => (
                  <li data-anchor={`fn-${f.label}`} value={f.index}>
                    <Blocks blocks={f.blocks} ctx={ctx} />
                  </li>
                )}
              </For>
            </ol>
          </section>
        </Show>
        <Show when={doc() && !doc()!.blocks.length && !frontMatter()}>
          <div class="mdp-empty mdp-muted">{tr("tools.markdown.nothingToPreview")}</div>
        </Show>
      </div>
    </div>
  );
}
