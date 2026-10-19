import { createMemo, createSignal, For, Match, Show, Switch } from "solid-js";
import { Check, Copy, TextCursorInput } from "lucide-solid";
import { insertAtCaret } from "@/editor/views";
import { parseMarkdown, type Inline, type MdBlock } from "./markdown";
import { highlight, type HighlightedLines } from "./highlight";

export function Inlines(props: { items: Inline[] }) {
  return (
    <For each={props.items}>
      {(i) => (
        <Switch fallback={i.text}>
          <Match when={i.t === "code"}>
            <code class="md-code">{i.text}</code>
          </Match>
          <Match when={i.t === "strong"}>
            <strong>{i.text}</strong>
          </Match>
          <Match when={i.t === "em"}>
            <em>{i.text}</em>
          </Match>
          <Match when={i.t === "link" && /^https?:\/\//.test(i.href)}>
            <a class="md-link" href={i.t === "link" ? i.href : undefined} target="_blank" rel="noreferrer noopener">
              {i.text}
            </a>
          </Match>
          <Match when={i.t === "link"}>
            <span class="md-link">{i.text}</span>
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
    try {
      await navigator.clipboard.writeText(props.text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      /* clipboard unavailable */
    }
  };
  return (
    <div class="chat-codeblock">
      <div class="chat-codeblock-bar">
        <span class="chat-codeblock-lang">{props.lang || "text"}</span>
        <button class="chat-mini-btn" title="Insert at caret" onClick={() => insertAtCaret(props.text)}>
          <TextCursorInput size={13} />
          <span>Insert</span>
        </button>
        <button class="chat-mini-btn" title="Copy" onClick={copy}>
          {copied() ? <Check size={13} /> : <Copy size={13} />}
          <span>{copied() ? "Copied" : "Copy"}</span>
        </button>
      </div>
      <pre class="chat-code">
        <HighlightedCode lines={lines()} />
      </pre>
    </div>
  );
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
      return (
        <p class="md-p">
          <Inlines items={b.inline} />
        </p>
      );
    case "quote":
      return (
        <blockquote class="md-quote">
          <Inlines items={b.inline} />
        </blockquote>
      );
    case "hr":
      return <hr class="md-hr" />;
    case "code":
      return <CodeBlock lang={b.lang} text={b.text} />;
    case "list": {
      const items = (
        <For each={b.items}>
          {(item) => (
            <li class="md-li" classList={{ task: item.checked !== undefined }} style={{ "margin-left": `${item.depth * 16}px` }}>
              <Show when={item.checked !== undefined}>
                <span class="md-check" classList={{ done: item.checked }}>
                  <Show when={item.checked}>
                    <Check size={10} stroke-width={3} />
                  </Show>
                </span>
              </Show>
              <span>
                <Inlines items={item.inline} />
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

export function Markdown(props: { text: string; streaming?: boolean }) {
  const blocks = createMemo(() => parseMarkdown(props.text));
  return (
    <div class="md" classList={{ streaming: props.streaming }}>
      <For each={blocks()}>{(b) => <Block block={b} />}</For>
    </div>
  );
}
