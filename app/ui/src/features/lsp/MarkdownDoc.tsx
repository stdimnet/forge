/**
 * Renders the Markdown AST of language server documentation (`docs` flavor, computed by
 * `forge-markdown`) with Solid components. Links are inert link-styled text with the target
 * in the tooltip; code fences get PHP syntax colors.
 */
import { createRoot, For, Match, Switch, type JSX } from "solid-js";
import type { MdBlock, MdInline } from "@/ipc/markdown";
import { highlightInto } from "./highlightCode";

export function DocInlines(props: { items: MdInline[] }): JSX.Element {
  return (
    <For each={props.items}>
      {(i) => (
        <Switch>
          <Match when={i.t === "text" && i}>{(x) => x().text}</Match>
          <Match when={i.t === "code" && i}>
            {(x) => <code>{x().parts ? <DocInlines items={x().parts!} /> : x().text}</code>}
          </Match>
          <Match when={i.t === "strong" && i}>
            {(x) => (
              <strong>
                <DocInlines items={x().children} />
              </strong>
            )}
          </Match>
          <Match when={i.t === "em" && i}>
            {(x) => (
              <em>
                <DocInlines items={x().children} />
              </em>
            )}
          </Match>
          <Match when={i.t === "del" && i}>
            {(x) => (
              <del>
                <DocInlines items={x().children} />
              </del>
            )}
          </Match>
          <Match when={i.t === "link" && i}>
            {(x) => (
              <span class="md-link" title={x().href}>
                <DocInlines items={x().children} />
              </span>
            )}
          </Match>
          <Match when={i.t === "url" && i}>
            {(x) => (
              <span class="md-link" title={x().text}>
                {x().text}
              </span>
            )}
          </Match>
          <Match when={i.t === "fqcn" && i}>
            {(x) => (
              <span class="md-fqcn" title={x().text.replace(/^\\/, "")}>
                {x().text}
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

function Code(props: { lang: string; text: string }) {
  return (
    <pre class="md-code">
      <code ref={(el) => highlightInto(el, props.text, props.lang || "php")} />
    </pre>
  );
}

export function DocBlocks(props: { blocks: MdBlock[]; inQuote?: boolean }): JSX.Element {
  return (
    <For each={props.blocks}>
      {(b) => (
        <Switch>
          <Match when={(b.t === "paragraph" || b.t === "plain") && b}>
            {(x) =>
              x().t === "plain" || props.inQuote ? (
                <DocInlines items={x().inline} />
              ) : (
                <p>
                  <DocInlines items={x().inline} />
                </p>
              )
            }
          </Match>
          <Match when={b.t === "heading" && b}>
            {(x) => (
              <div class={`md-h md-h${x().level}`}>
                <DocInlines items={x().inline} />
              </div>
            )}
          </Match>
          <Match when={b.t === "list" && b}>
            {(x) => {
              const items = (
                <For each={x().items}>
                  {(item) => (
                    <li>
                      <DocBlocks blocks={item.blocks} />
                    </li>
                  )}
                </For>
              );
              return x().ordered ? <ol>{items}</ol> : <ul>{items}</ul>;
            }}
          </Match>
          <Match when={b.t === "code" && b}>{(x) => <Code lang={x().lang} text={x().text} />}</Match>
          <Match when={b.t === "quote" && b}>
            {(x) => (
              <blockquote>
                <DocBlocks blocks={x().blocks} inQuote />
              </blockquote>
            )}
          </Match>
          <Match when={b.t === "hr"}>
            <hr />
          </Match>
        </Switch>
      )}
    </For>
  );
}

/** Static DOM for places outside Solid components (CodeMirror tooltips). */
function staticNodes(render: () => JSX.Element): Node[] {
  return createRoot((dispose) => {
    const holder = (<div>{render()}</div>) as HTMLDivElement;
    dispose();
    return [...holder.childNodes];
  });
}

/** Append rendered blocks to `parent`. */
export function appendBlocks(parent: HTMLElement, blocks: MdBlock[]): void {
  parent.append(...staticNodes(() => <DocBlocks blocks={blocks} />));
}

/** Append rendered inline content to `parent`. */
export function appendInlines(parent: HTMLElement, items: MdInline[]): void {
  parent.append(...staticNodes(() => <DocInlines items={items} />));
}
