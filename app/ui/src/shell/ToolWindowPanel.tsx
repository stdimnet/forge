import { Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { Minus } from "lucide-solid";
import { findToolWindow, type ToolWindowAnchor } from "@/features/registry";
import { layout, toggleToolWindow } from "@/state/layout";

export function ToolWindowPanel(props: { anchor: ToolWindowAnchor }) {
  const tw = () => findToolWindow(layout.active[props.anchor]);
  return (
    <Show when={tw()}>
      {(w) => (
        <section class={`tool-window tool-window-${props.anchor}`} data-tool-window-panel={w().id}>
          <header class="tool-window-header">
            <span class="tool-window-title">{w().title}</span>
            <div class="tool-window-actions">
              {w().actions?.()}
              <button class="icon-btn" title="Hide" onClick={() => toggleToolWindow(props.anchor, w().id)}>
                <Minus size={15} />
              </button>
            </div>
          </header>
          <div class="tool-window-body">
            <Dynamic component={w().component} />
          </div>
        </section>
      )}
    </Show>
  );
}
