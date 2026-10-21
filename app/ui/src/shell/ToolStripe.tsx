import { For, Show } from "solid-js";
import { toolWindowsFor, type ToolWindow, type ToolWindowAnchor } from "@/features/registry";
import { layout, toggleToolWindow } from "@/state/layout";

function StripeButton(props: { tw: ToolWindow }) {
  const active = () => layout.active[props.tw.anchor] === props.tw.id;
  return (
    <button
      class="icon-btn stripe-btn"
      classList={{ active: active() }}
      title={props.tw.shortcut ? `${props.tw.title} (${props.tw.shortcut})` : props.tw.title}
      data-tool-window={props.tw.id}
      onClick={() => toggleToolWindow(props.tw.anchor, props.tw.id)}
    >
      <props.tw.icon size={18} stroke-width={1.75} />
      <Show when={!active() && props.tw.badge}>
        {(badge) => <span class="stripe-badge">{badge()()}</span>}
      </Show>
    </button>
  );
}

/**
 * Vertical icon stripe at the window edge (PHPStorm New UI). The left stripe holds left
 * tool windows on top and bottom tool windows at its foot; the right one holds right windows.
 */
export function ToolStripe(props: { side: "left" | "right" }) {
  const top = (): ToolWindowAnchor => props.side;
  return (
    <nav class={`tool-stripe tool-stripe-${props.side}`}>
      <div class="stripe-group">
        <For each={toolWindowsFor(top())}>{(tw) => <StripeButton tw={tw} />}</For>
      </div>
      {props.side === "left" && (
        <div class="stripe-group">
          <For each={toolWindowsFor("bottom")}>{(tw) => <StripeButton tw={tw} />}</For>
        </div>
      )}
    </nav>
  );
}
