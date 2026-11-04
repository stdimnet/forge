/**
 * Renders the Markdown AST of assistant messages (computed by `forge-markdown`, `chat`
 * flavor) with Solid components; model output never becomes HTML. Only `http(s)` links
 * are clickable.
 */
import { createMemo, createSignal, For, Match, Show, Switch } from "solid-js";
import { Check, Copy, TextCursorInput } from "lucide-solid";
import { insertAtCaret } from "@/editor/views";
import type { MdBlock, MdInline, MdListItem } from "@/ipc/markdown";
import { highlight, type HighlightedLines } from "./highlight";
import { t } from "@/i18n";
import { writeClipboard } from "@/features/clipboard/system";

export function Inlines(props: { items: MdInline[] }) {
  return (
    <For each={props.items}>
      {(i) => (
        <Switch>
          <Match when={i.t === "text" && i}>{(x) => x().text}</Match>
          <Match when={(i.t === "url" || i.t === "fqcn") && i}>{(x) => x().text}</Match>
          <Match when={i.t === "code" && i}>{(x) => <code class="md-code">{x().text}</code>}</Match>
          <Match when={i.t === "strong" && i}>
            {(x) => (
              <strong>
                <Inlines items={x().children} />
              </strong>
            )}
          </Match>
          <Match when={i.t === "em" && i}>
            {(x) => (
              <em>
                <Inlines items={x().children} />
              </em>
            )}
          </Match>
          <Match when={i.t === "del" && i}>
            {(x) => (
              <del>
                <Inlines items={x().children} />
              </del>
            )}
          </Match>
          <Match when={i.t === "link" && /^https?:\/\//.test(i.href) && i}>
            {(x) => (
              <a class="md-link" href={x().href} target="_blank" rel="noreferrer noopener">
                <Inlines items={x().children} />
              </a>
            )}
          </Match>
          <Match when={i.t === "link" && i}>
            {(x) => (
              <span class="md-link">
                <Inlines items={x().children} />
              </span>
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

export function HighlightedCode(props: { lines: HighlightedLines }) {
  return (
    <For each={props.lines}>
      {(line) => (
        <div class="code-line">
          <For each={line}>{(s) => (s.cls ? <span class={s.cls}>{s.text}</span> : s.text)}</For>
          {line.length === 0 ? "​" : ""}
        </div>
      )}
    </For>
  );
}

export function CodeBlock(props: { lang: string; text: string }) {
  const [copied, setCopied] = createSignal(false);
  const lines = createMemo(() => highlight(props.text, props.lang));
  const copy = async () => {
    if (!(await writeClipboard(props.text))) return;
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };
  return (
    <div class="chat-codeblock">
      <div class="chat-codeblock-bar">
        <span class="chat-codeblock-lang">{props.lang || "text"}</span>
        <button class="chat-mini-btn" title={t("tools.aiChat.insertAtCaret")} onClick={() => insertAtCaret(props.text)}>
          <TextCursorInput size={13} />
          <span>{t("common.insert")}</span>
        </button>
        <button class="chat-mini-btn" title={t("common.copy")} onClick={copy}>
          {copied() ? <Check size={13} /> : <Copy size={13} />}
          <span>{copied() ? t("common.copied") : t("common.copy")}</span>
        </button>
      </div>
      <pre class="chat-code">
        <HighlightedCode lines={lines()} />
      </pre>
    </div>
  );
}

/** Inline content of blocks (paragraphs of a list item or quote), joined by spaces. */
function inlineOf(blocks: MdBlock[]): MdInline[] {
  const out: MdInline[] = [];
  for (const b of blocks) {
    if (b.t !== "paragraph" && b.t !== "plain" && b.t !== "heading") continue;
    if (out.length) out.push({ t: "text", text: " " });
    out.push(...b.inline);
  }
  return out;
}

/** Nested list items as one flat list with indentation depth (max 3), like a chat message. */
function flatItems(items: MdListItem[], depth = 0): { item: MdListItem; depth: number }[] {
  return items.flatMap((item) => [
    { item, depth: Math.min(3, depth) },
    ...item.blocks.flatMap((b) => (b.t === "list" ? flatItems(b.items, depth + 1) : [])),
  ]);
}

function Block(props: { block: MdBlock }) {
  const b = props.block;
  switch (b.t) {
    case "heading":
      return (
        <div class={`md-h md-h${Math.min(b.level, 4)}`}>
          <Inlines items={b.inline} />
        </div>
      );
    case "paragraph":
    case "plain":
      return (
        <p class="md-p">
          <Inlines items={b.inline} />
        </p>
      );
    case "quote":
      return (
        <blockquote class="md-quote">
          <Inlines items={inlineOf(b.blocks)} />
        </blockquote>
      );
    case "hr":
      return <hr class="md-hr" />;
    case "code":
      return <CodeBlock lang={b.lang} text={b.text} />;
    case "list": {
      const items = (
        <For each={flatItems(b.items)}>
          {({ item, depth }) => (
            <li class="md-li" classList={{ task: item.checked !== undefined }} style={{ "margin-left": `${depth * 16}px` }}>
              <Show when={item.checked !== undefined}>
                <span class="md-check" classList={{ done: item.checked }}>
                  <Show when={item.checked}>
                    <Check size={10} stroke-width={3} />
                  </Show>
                </span>
              </Show>
              <span>
                <Inlines items={inlineOf(item.blocks)} />
              </span>
            </li>
          )}
        </For>
      );
      return b.ordered ? (
        <ol class="md-list" start={b.start}>
          {items}
        </ol>
      ) : (
        <ul class="md-list">{items}</ul>
      );
    }
  }
}

export function Markdown(props: { blocks: MdBlock[]; streaming?: boolean }) {
  return (
    <div class="md" classList={{ streaming: props.streaming }}>
      <For each={props.blocks}>{(b) => <Block block={b} />}</For>
    </div>
  );
}
